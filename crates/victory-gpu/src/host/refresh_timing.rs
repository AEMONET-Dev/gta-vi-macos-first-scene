//! Opt-in timing of buffer-refresh batches (`VICTORY_REFRESH_TIMING=1`).
//!
//! A refresh batch runs on the submitting thread and its workers, and the
//! submission waits for all of them, so a batch's wall time is time the
//! frame waits. This splits that time on the worker that finished last (the
//! one the batch actually waited for) into what the copy path spent it on:
//!
//! * `first`: the optimistic whole-span `ReadProcessMemory`, split into
//!   reads that succeeded and reads that failed on an unmapped page (Wine
//!   copies the bytes before the hole and then reports failure, so the
//!   failed time is wasted work);
//! * `query`: `VirtualQuery` calls finding the readable regions;
//! * `retry`: the per-region re-reads and their 64 KiB / 4 KiB salvage;
//! * `zero`: zero-filling unreadable regions in the host copy;
//! * `other`: the rest of the jobs (planning, tracking, snapshots).
//!
//! One `refresh batch timing` line per batch; the first timestamp in the
//! line is when the batch ended, its wall time is `wall_us`. Off, each probe
//! costs one relaxed load.

use std::cell::Cell;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU8, Ordering};
use std::time::Instant;

/// What a probe timed.
#[derive(Clone, Copy)]
pub(crate) enum Kind {
    FirstOk = 0,
    FirstFailed = 1,
    Query = 2,
    Retry = 3,
    Zero = 4,
}

const KINDS: usize = 5;
const NAMES: [&str; KINDS] = ["first_ok", "first_failed", "query", "retry", "zero"];

/// Per-thread running totals: microseconds, calls and bytes per kind.
#[derive(Clone, Copy, Default)]
pub(crate) struct Totals {
    us: [u64; KINDS],
    calls: [u64; KINDS],
    bytes: [u64; KINDS],
    /// Holes a copy could not remember because its list was full (they
    /// are walked with `VirtualQuery` again on every later copy).
    capped: u64,
}

impl Totals {
    fn minus(&self, o: &Totals) -> Totals {
        let mut d = Totals::default();
        for k in 0..KINDS {
            d.us[k] = self.us[k] - o.us[k];
            d.calls[k] = self.calls[k] - o.calls[k];
            d.bytes[k] = self.bytes[k] - o.bytes[k];
        }
        d.capped = self.capped - o.capped;
        d
    }
    fn add(&mut self, o: &Totals) {
        for k in 0..KINDS {
            self.us[k] += o.us[k];
            self.calls[k] += o.calls[k];
            self.bytes[k] += o.bytes[k];
        }
        self.capped += o.capped;
    }
    fn timed_us(&self) -> u64 {
        self.us.iter().sum()
    }
    fn describe(&self) -> String {
        let mut s = String::new();
        for k in 0..KINDS {
            s.push_str(&format!(
                "{}={}us/{}x/{}MB ",
                NAMES[k],
                self.us[k],
                self.calls[k],
                self.bytes[k] / 1_000_000
            ));
        }
        s.push_str(&format!("capped={}", self.capped));
        s
    }
}

thread_local! {
    static TOTALS: Cell<Totals> = Cell::new(Totals::default());
}

static STATE: AtomicU8 = AtomicU8::new(0);

/// Whether timing is on (`VICTORY_REFRESH_TIMING=1`), read once.
pub(crate) fn on() -> bool {
    match STATE.load(Ordering::Relaxed) {
        1 => false,
        2 => true,
        _ => {
            let on = crate::switch::on("VICTORY_REFRESH_TIMING");
            STATE.store(if on { 2 } else { 1 }, Ordering::Relaxed);
            on
        }
    }
}

/// Tests: turn timing on or off regardless of the environment.
#[cfg(test)]
pub(crate) fn force(on: bool) {
    STATE.store(if on { 2 } else { 1 }, Ordering::Relaxed);
}

/// A started probe; `done` adds its time to this thread's totals.
pub(crate) struct Probe(Option<Instant>);

pub(crate) fn start() -> Probe {
    Probe(on().then(Instant::now))
}

impl Probe {
    pub(crate) fn done(self, kind: Kind, bytes: usize) {
        if let Some(t) = self.0 {
            let us = t.elapsed().as_micros() as u64;
            TOTALS.with(|c| {
                let mut v = c.get();
                v.us[kind as usize] += us;
                v.calls[kind as usize] += 1;
                v.bytes[kind as usize] += bytes as u64;
                c.set(v);
            });
        }
    }
}

/// A copy's hole list reached its cap.
pub(crate) fn capped() {
    if on() {
        TOTALS.with(|c| {
            let mut v = c.get();
            v.capped += 1;
            c.set(v);
        });
    }
}

fn snapshot() -> Totals {
    TOTALS.with(Cell::get)
}

/// One job of a batch: which thread ran it, when it ended (µs after the
/// batch began), and what its probes saw.
struct Job {
    thread: std::thread::ThreadId,
    end_us: u64,
    busy_us: u64,
    totals: Totals,
}

/// The jobs of one batch being timed.
pub(crate) struct Batch {
    began: Instant,
    jobs: Mutex<Vec<Job>>,
}

/// What [`Batch::finish`] found (read back by tests; logged otherwise).
#[derive(Clone, Copy)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct Summary {
    pub(crate) wall_us: u64,
    pub(crate) workers: usize,
    pub(crate) jobs: usize,
    pub(crate) critical: Totals,
    pub(crate) critical_busy_us: u64,
    pub(crate) all: Totals,
}

#[cfg(test)]
impl Summary {
    pub(crate) fn critical_us(&self, kind: Kind) -> u64 {
        self.critical.us[kind as usize]
    }
    pub(crate) fn all_calls(&self, kind: Kind) -> u64 {
        self.all.calls[kind as usize]
    }
    pub(crate) fn all_bytes(&self, kind: Kind) -> u64 {
        self.all.bytes[kind as usize]
    }
}

pub(crate) fn batch() -> Option<Batch> {
    on().then(|| Batch {
        began: Instant::now(),
        jobs: Mutex::default(),
    })
}

impl Batch {
    /// Runs one job of the batch, timing it.
    pub(crate) fn job<R>(&self, f: impl FnOnce() -> R) -> R {
        let before = snapshot();
        let started = Instant::now();
        let r = f();
        let busy_us = started.elapsed().as_micros() as u64;
        let job = Job {
            thread: std::thread::current().id(),
            end_us: self.began.elapsed().as_micros() as u64,
            busy_us,
            totals: snapshot().minus(&before),
        };
        self.jobs.lock().unwrap_or_else(|e| e.into_inner()).push(job);
        r
    }

    /// Logs the batch: the totals of the worker that finished last (the
    /// one the submission waited for) and of all workers.
    pub(crate) fn finish(self, workers: usize, bytes: usize) -> Summary {
        let wall_us = self.began.elapsed().as_micros() as u64;
        let jobs = self.jobs.into_inner().unwrap_or_else(|e| e.into_inner());
        let last = jobs.iter().max_by_key(|j| j.end_us).map(|j| j.thread);
        let (mut critical, mut all) = (Totals::default(), Totals::default());
        let mut critical_busy_us = 0;
        let mut threads = std::collections::HashSet::new();
        for j in &jobs {
            all.add(&j.totals);
            threads.insert(j.thread);
            if Some(j.thread) == last {
                critical.add(&j.totals);
                critical_busy_us += j.busy_us;
            }
        }
        let summary = Summary {
            wall_us,
            workers: threads.len(),
            jobs: jobs.len(),
            critical,
            critical_busy_us,
            all,
        };
        tracing::info!(
            target: "victory_gpu::refresh_timing",
            "refresh batch timing wall_us={wall_us} workers={} planned_workers={workers} jobs={} bytes={bytes} \
             critical_busy_us={critical_busy_us} critical_timed_us={} | critical: {} | all: {}",
            summary.workers,
            summary.jobs,
            critical.timed_us(),
            critical.describe(),
            all.describe(),
        );
        summary
    }
}

#[cfg(test)]
mod tests {
    use super::{Kind, batch, force};
    use windows_sys::Win32::System::Memory::{
        MEM_COMMIT, MEM_DECOMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_READWRITE, VirtualAlloc, VirtualFree,
    };

    /// A batch of 4-worker copies over title memory with unmapped 64 KiB
    /// blocks: the copies stay exact, and the timing splits the work into
    /// failed first reads, region queries, retries and zero-fill, with the
    /// critical worker's timed parts inside the batch's wall time. A second
    /// batch over the same spans uses the remembered holes: no failed first
    /// read of a whole span, the same zero-fill.
    #[test]
    fn timing_splits_a_batch_over_holes() {
        const B: usize = 64 << 10;
        const SPAN: usize = 16 * B;
        const SPANS: usize = 8;
        force(true);
        // SAFETY: our own memory, released at the end.
        unsafe {
            let src = VirtualAlloc(std::ptr::null(), SPANS * SPAN, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE).cast::<u8>();
            assert!(!src.is_null());
            for i in 0..SPANS * SPAN {
                src.add(i).write((i / 4096) as u8 | 1);
            }
            let mut expected: Vec<u8> = (0..SPANS * SPAN).map(|i| (i / 4096) as u8 | 1).collect();
            // Two holes in every span: one in the middle, one at the end.
            let mut hole_bytes = 0;
            for s in 0..SPANS {
                for b in [5, 15] {
                    let at = s * SPAN + b * B;
                    assert_ne!(VirtualFree(src.add(at).cast(), B, MEM_DECOMMIT), 0);
                    expected[at..at + B].fill(0);
                    hole_bytes += B;
                }
            }
            let mut dst = vec![0xeeu8; SPANS * SPAN];
            let out = dst.as_mut_ptr() as usize;
            let base = src as usize;
            let jobs: Vec<usize> = (0..SPANS).collect();
            let mut summaries = Vec::new();
            // One worker twice (hole hints are per thread), then four.
            for workers in [1, 1, 4] {
                let timing = batch().expect("timing is on");
                crate::host::upload_jobs::run(&jobs, workers, |&s| {
                    timing.job(|| {
                        crate::host::copy_mapped((out + s * SPAN) as *mut u8, (base + s * SPAN) as *const u8, SPAN, false);
                        SPAN
                    })
                });
                summaries.push(timing.finish(4, SPANS * SPAN));
                assert!(dst == expected, "copy matches title memory, zeros at the holes");
                dst.fill(0xee);
            }
            let (first, second, parallel) = (summaries[0], summaries[1], summaries[2]);
            for s in [&first, &second, &parallel] {
                assert_eq!(s.jobs, SPANS);
                assert!((1..=4).contains(&s.workers));
                assert_eq!(s.all_bytes(Kind::Zero) as usize, hole_bytes, "each hole zero-filled once");
                assert!(s.all_calls(Kind::Query) > 0, "holes are found by region queries");
                let timed: u64 = [Kind::FirstOk, Kind::FirstFailed, Kind::Query, Kind::Retry, Kind::Zero]
                    .iter()
                    .map(|&k| s.critical_us(k))
                    .sum();
                assert!(timed <= s.critical_busy_us + 1, "timed parts lie inside the critical worker's jobs");
                assert!(s.critical_busy_us <= s.wall_us + 1, "the critical worker lies inside the batch");
            }
            assert_eq!(first.all_calls(Kind::FirstFailed) as usize, SPANS, "each span's first whole read fails once");
            assert_eq!(second.all_calls(Kind::FirstFailed), 0, "remembered holes avoid the failing whole read");
            assert!(second.all_calls(Kind::FirstOk) > 0, "readable runs between remembered holes read directly");
            assert!(parallel.critical_busy_us <= parallel.wall_us + 1);
            println!(
                "first pass: wall {} us, failed first reads {} ({} MB), queries {}; second: wall {} us, queries {}",
                first.wall_us,
                first.all_calls(Kind::FirstFailed),
                first.all_bytes(Kind::FirstFailed) / 1_000_000,
                first.all_calls(Kind::Query),
                second.wall_us,
                second.all_calls(Kind::Query)
            );
            assert_ne!(VirtualFree(src.cast(), 0, MEM_RELEASE), 0);
        }
        force(false);
        assert!(batch().is_none(), "off: no batch timing");
    }
}
