//! Join small untracked texture reads within one submitted heap. The ranges
//! are planned anew each submission. Only complete, kernel-checked snapshots
//! are passed to a texture, including when a gap has become unreadable.
use std::cell::RefCell;
use std::ops::Range;

pub(super) struct TextureRange {
    pub(super) job: usize,
    pub(super) start: usize,
    pub(super) end: usize,
}

pub(super) struct Group {
    members: Range<usize>,
    start: usize,
    end: usize,
}

pub(super) const LIMIT: usize = 4 << 20;

pub(super) fn groups(ranges: &mut [TextureRange]) -> Vec<Group> {
    const MAX_GAP: usize = 4 << 10;
    ranges.sort_unstable_by_key(|r| (r.start, r.end));
    let mut result: Vec<Group> = Vec::new();
    let mut padding = 0;
    for (i, r) in ranges.iter().enumerate() {
        if let Some(group) = result.last_mut() {
            let gap = r.start.saturating_sub(group.end);
            let end = group.end.max(r.end);
            let bytes = end - group.start;
            let unused = padding + gap;
            if bytes <= LIMIT && gap <= MAX_GAP && unused <= (bytes - unused) / 4 {
                group.members.end = i + 1;
                group.end = end;
                padding = unused;
                continue;
            }
        }
        result.push(Group {
            members: i..i + 1,
            start: r.start,
            end: r.end,
        });
        padding = 0;
    }
    result
}

/// Return the logical bytes checked. A singleton takes its usual path. A
/// failed group discards even the readable prefix and retries each texture.
pub(super) fn check(
    group: &Group,
    ranges: &[TextureRange],
    mut accept: impl FnMut(usize, Option<&[u8]>) -> usize,
) -> usize {
    let members = &ranges[group.members.clone()];
    if members.len() > 1 {
        thread_local! { static BYTES: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) }; }
        let checked = BYTES.with_borrow_mut(|buf| {
            let size = group.end - group.start;
            if buf.len() < size {
                buf.resize(size, 0);
            }
            if !crate::host::read_into(group.start as *const u8, &mut buf[..size]) {
                return None;
            }
            Some(
                members
                    .iter()
                    .map(|r| {
                        let snapshot = &buf[r.start - group.start..r.end - group.start];
                        accept(r.job, Some(snapshot))
                    })
                    .sum(),
            )
        });
        if let Some(bytes) = checked {
            return bytes;
        }
    }
    members.iter().map(|r| accept(r.job, None)).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overlaps_do_not_inflate_the_gap_budget_and_large_textures_stay_whole() {
        const M: usize = 1 << 20;
        let mut ranges: Vec<_> = [
            (320, 420),
            (0, 100),
            (120, 220),
            (0, 100),
            (255, 355),
            (M, 3 * M),
            (3 * M, 5 * M),
            (5 * M, 5 * M + 1),
            (10 * M, 16 * M),
            (16 * M + 1, 16 * M + 2),
        ]
        .into_iter()
        .enumerate()
        .map(|(job, (start, end))| TextureRange { job, start, end })
        .collect();
        let planned = groups(&mut ranges);
        let bounds: Vec<_> = planned.iter().map(|g| (g.start, g.end)).collect();
        assert_eq!(
            bounds,
            [
                (0, 420),
                (M, 5 * M),
                (5 * M, 5 * M + 1),
                (10 * M, 16 * M),
                (16 * M + 1, 16 * M + 2)
            ]
        );

        let mut duplicates: Vec<_> = [(0, 100), (0, 100), (151, 251)]
            .into_iter()
            .enumerate()
            .map(|(job, (start, end))| TextureRange { job, start, end })
            .collect();
        assert_eq!(groups(&mut duplicates).len(), 2);
        let mut wide: Vec<_> = [(0, M), (M + 4097, M * 2)]
            .into_iter()
            .enumerate()
            .map(|(job, (start, end))| TextureRange { job, start, end })
            .collect();
        assert_eq!(groups(&mut wide).len(), 2);
    }

    #[test]
    fn overlapping_textures_retry_individually_when_streamed_pages_disappear() {
        use windows_sys::Win32::System::Memory::{
            MEM_COMMIT, MEM_DECOMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_READWRITE, VirtualAlloc,
            VirtualFree,
        };
        const P: usize = 4096;
        let locations = [(7, 10), (1, 3), (5, 6), (0, 5)];
        let mut previous = vec![Vec::new(); locations.len()];
        // SAFETY: the test owns ten pages. Only checked reads touch holes.
        unsafe {
            let base = VirtualAlloc(
                std::ptr::null(),
                10 * P,
                MEM_COMMIT | MEM_RESERVE,
                PAGE_READWRITE,
            )
            .cast::<u8>();
            assert!(!base.is_null());
            for i in 0..10 {
                base.add(i * P).write_bytes(i as u8, P);
            }
            let mut ranges: Vec<_> = locations
                .iter()
                .enumerate()
                .map(|(job, &(a, b))| TextureRange {
                    job,
                    start: base as usize + a * P,
                    end: base as usize + b * P,
                })
                .collect();
            let planned = groups(&mut ranges);
            assert_eq!(planned.len(), 1);
            let mut refresh = |wanted: &[usize], fallbacks| {
                let mut changed = Vec::new();
                let mut single = 0;
                check(&planned[0], &ranges, |job, snapshot| {
                    let (from, to) = locations[job];
                    let size = (to - from) * P;
                    let update = if let Some(bytes) = snapshot {
                        assert_eq!(bytes.len(), size);
                        let changed = previous[job] != bytes;
                        previous[job] = bytes.to_vec();
                        changed
                    } else {
                        single += 1;
                        super::super::read_changed(&mut previous[job], base.add(from * P), size)
                            .unwrap_or(false)
                    };
                    if update {
                        changed.push(job);
                    }
                    size
                });
                changed.sort_unstable();
                assert_eq!(changed, wanted);
                assert_eq!(single, fallbacks);
            };
            refresh(&[0, 1, 2, 3], 0);
            refresh(&[], 0);
            // Page 6 lies in unused padding. Other textures must still update.
            assert_ne!(VirtualFree(base.add(6 * P).cast(), P, MEM_DECOMMIT), 0);
            base.add(P + 16).write(46);
            refresh(&[1, 3], 4);
            assert_ne!(VirtualFree(base.add(5 * P).cast(), P, MEM_DECOMMIT), 0);
            base.add(9 * P).write(47);
            refresh(&[0], 4);
            assert_eq!(
                VirtualAlloc(base.add(5 * P).cast(), 2 * P, MEM_COMMIT, PAGE_READWRITE),
                base.add(5 * P).cast()
            );
            base.add(5 * P).write(48);
            refresh(&[2], 0);
            refresh(&[], 0);
            assert_ne!(VirtualFree(base.cast(), 0, MEM_RELEASE), 0);
        }
    }
}
