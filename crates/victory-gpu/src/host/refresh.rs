//! Ranges of one host buffer refreshed between title writes.

type Span = (usize, usize);

#[derive(Default)]
pub(crate) struct Ranges {
    /// Sorted disjoint half-open intervals already copied.
    current: Vec<Span>,
    requested: Vec<Span>,
}

impl Ranges {
    /// Returns true only when the buffer first becomes due for a flush.
    pub(crate) fn request(&mut self, from: usize, len: usize) -> bool {
        let to = from.saturating_add(len);
        if from == to {
            return false;
        }
        let first = self.current.partition_point(|&(_, end)| end <= from);
        if self
            .current
            .get(first)
            .is_some_and(|&(a, b)| a <= from && to <= b)
            || self
                .requested
                .last()
                .is_some_and(|&(a, b)| a <= from && to <= b)
        {
            return false;
        }
        let was_empty = self.requested.is_empty();
        self.requested.push((from, to));
        was_empty
    }

    pub(crate) fn flush(&mut self, runs: &mut Vec<Span>, mut copy: impl FnMut(usize, usize)) {
        if self.requested.is_empty() {
            return;
        }
        merge_overlaps(&mut self.requested);
        runs.clear();
        // The next current interval names the gap in which a copy can run.
        // Never join two copies across bytes synchronized by an earlier flush.
        let mut last_gap = None;
        let mut payload = 0usize;
        let mut add = |from, to, gap: usize| {
            let bytes = to - from;
            if let Some((start, end)) = runs.last_mut()
                && last_gap == Some(gap)
                && from - *end <= 4 << 20
                && to - *start - (payload + bytes) <= (payload + bytes) / 4
            {
                // Pay for one larger copy only when all padding combined adds
                // at most a quarter of the requested bytes in this run.
                *end = to;
                payload += bytes;
            } else {
                runs.push((from, to));
                payload = bytes;
                last_gap = Some(gap);
            }
        };
        let mut i = 0;
        for (mut from, to) in self.requested.drain(..) {
            i += self.current[i..].partition_point(|&(_, end)| end <= from);
            while let Some(&(start, end)) = self.current.get(i) {
                if to <= start {
                    break;
                }
                if from < start {
                    add(from, start, i);
                }
                from = end.min(to);
                if from == to {
                    break;
                }
                i += 1;
            }
            if from < to {
                add(from, to, i);
            }
        }
        for &(from, to) in runs.iter() {
            copy(from, to);
        }
        if self.current.is_empty() {
            std::mem::swap(&mut self.current, runs);
        } else if !runs.is_empty() {
            self.current.extend(runs.iter().copied());
            merge_overlaps(&mut self.current);
        }
    }
}

fn merge_overlaps(spans: &mut Vec<Span>) {
    if spans.len() < 2 {
        return;
    }
    spans.sort_unstable();
    let mut last = 0;
    for i in 1..spans.len() {
        let (from, to) = spans[i];
        if from <= spans[last].1 {
            spans[last].1 = spans[last].1.max(to);
        } else {
            last += 1;
            spans[last] = (from, to);
        }
    }
    spans.truncate(last + 1);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flush(ranges: &mut Ranges) -> Vec<Span> {
        let mut seen = Vec::new();
        ranges.flush(&mut Vec::new(), |a, b| seen.push((a, b)));
        seen
    }

    #[test]
    fn wide_holes_stay_unread_and_small_gaps_have_a_byte_budget() {
        const M: usize = 1 << 20;
        let mut r = Ranges::default();
        for (a, n) in [
            (120 * M, 128),
            (M + M / 8, M),
            (0, M),
            (100 * M, 64),
            (101 * M, 64),
        ] {
            r.request(a, n);
        }
        let copied = flush(&mut r);
        assert_eq!(
            copied,
            [
                (0, 2 * M + M / 8),
                (100 * M, 100 * M + 64),
                (101 * M, 101 * M + 64),
                (120 * M, 120 * M + 128)
            ]
        );
        assert_eq!(
            copied.iter().map(|(a, b)| b - a).sum::<usize>(),
            2 * M + M / 8 + 256
        );
    }

    #[test]
    fn intersecting_requests_only_copy_each_missing_byte_once() {
        let mut r = Ranges::default();
        assert!(r.request(40, 2));
        assert!(!r.request(60, 2));
        assert_eq!(flush(&mut r), [(40, 42), (60, 62)]);
        assert!(r.request(42, 45));
        assert!(!r.request(0, 64));
        assert!(!r.request(87, 13));
        assert_eq!(flush(&mut r), [(0, 40), (42, 60), (62, 100)]);
        assert!(!r.request(0, 100));
        assert!(flush(&mut r).is_empty());
        assert!(!r.request(6, 0));
        assert!(!r.request(usize::MAX, 8));
        assert!(r.request(usize::MAX - 3, 4));
        assert_eq!(flush(&mut r), [(usize::MAX - 3, usize::MAX)]);
    }

    #[test]
    fn long_runs_stop_at_four_mib_per_gap_and_a_quarter_total_overhead() {
        const M: usize = 1 << 20;
        let mut r = Ranges::default();
        for (a, n) in [
            (0, 16 * M),
            (20 * M, 16 * M),
            (41 * M, 16 * M),
            (103 * M, M),
            (105 * M, M),
            (107 * M, M),
        ] {
            r.request(a, n);
        }
        assert_eq!(
            flush(&mut r),
            [
                (0, 36 * M),
                (41 * M, 57 * M),
                (103 * M, 104 * M),
                (105 * M, 106 * M),
                (107 * M, 108 * M)
            ]
        );
    }
}
