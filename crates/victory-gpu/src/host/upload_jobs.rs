//! Reuse CPU upload workers and join them before GPU submission.
use std::any::Any;
use std::panic::{AssertUnwindSafe, catch_unwind, resume_unwind};
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};
use std::sync::{Mutex, mpsc};

struct Round<'a> {
    drain: &'a (dyn Fn() + Sync),
    panics: Mutex<Vec<Box<dyn Any + Send>>>,
}

struct Task {
    round: *const (),
    _running: mpsc::Sender<()>,
}

// SAFETY: the pointed-to Round is Sync. Each task holds its own sender until
// its worker returns. Joined::drop waits for every sender before the caller
// may access or drop the borrowed round, including on unwind.
unsafe impl Send for Task {}

impl Task {
    fn execute(self) {
        // SAFETY: this task still owns its sender. Round outlives this call.
        let round = unsafe { &*self.round.cast::<Round<'_>>() };
        if let Err(payload) = catch_unwind(AssertUnwindSafe(round.drain)) {
            round
                .panics
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(payload);
        }
    }
}

struct Joined<'a> {
    round: &'a Round<'a>,
    start: Option<mpsc::Sender<()>>,
    complete: mpsc::Receiver<()>,
}

impl Joined<'_> {
    fn task(&self) -> Task {
        Task {
            round: std::ptr::from_ref(self.round).cast(),
            _running: self.start.as_ref().expect("open round").clone(),
        }
    }
}

impl Drop for Joined<'_> {
    fn drop(&mut self) {
        // No values are sent. Disconnection means all workers have finished.
        drop(self.start.take());
        let _ = self.complete.recv();
    }
}

/// A zero-capacity channel per worker: only idle workers can accept a task.
/// Concurrent (or nested) callers never queue behind their own workers.
static WORKERS: Workers = Mutex::new(Vec::new());

type Workers = Mutex<Vec<mpsc::SyncSender<Task>>>;

fn dispatch(pool: &Workers, joined: &Joined<'_>, count: usize) {
    let mut workers = pool.lock().unwrap_or_else(|e| e.into_inner());
    let mut sent = 0;
    for worker in workers.iter() {
        if worker.try_send(joined.task()).is_ok() {
            sent += 1;
            if sent == count {
                return;
            }
        }
    }
    while workers.len() < count {
        let (send, receive) = mpsc::sync_channel::<Task>(0);
        let first = joined.task();
        let made = std::thread::Builder::new()
            .name("graphics upload".into())
            .spawn(move || {
                first.execute();
                for task in receive {
                    task.execute();
                }
            });
        if made.is_err() {
            break; // the submitter drains any remaining work
        }
        workers.push(send);
    }
}

pub(crate) fn run<T: Sync>(
    jobs: &[T],
    workers: usize,
    check: impl Fn(&T) -> usize + Sync,
) -> usize {
    run_on(&WORKERS, jobs, workers, check)
}

fn run_on<T: Sync>(
    pool: &Workers,
    jobs: &[T],
    workers: usize,
    check: impl Fn(&T) -> usize + Sync,
) -> usize {
    let workers = workers.min(jobs.len());
    if workers <= 1 {
        return jobs.iter().map(check).sum();
    }
    let next = AtomicUsize::new(0);
    let bytes = AtomicUsize::new(0);
    let drain = || {
        let mut read = 0;
        while let Some(job) = jobs.get(next.fetch_add(1, Relaxed)) {
            read += check(job);
        }
        bytes.fetch_add(read, Relaxed);
    };
    let round = Round {
        drain: &drain,
        panics: Mutex::default(),
    };
    let (start, complete) = mpsc::channel();
    let joined = Joined {
        round: &round,
        start: Some(start),
        complete,
    };
    dispatch(pool, &joined, workers - 1);
    drain();
    drop(joined);
    let mut panics = round.panics.into_inner().unwrap_or_else(|e| e.into_inner());
    if let Some(payload) = panics.pop() {
        resume_unwind(payload);
    }
    bytes.into_inner()
}
#[cfg(test)]
mod tests {
    use super::{Workers, run, run_on};
    #[test]
    fn exactly_once_and_sum_when_workers_exceed_jobs() {
        use std::sync::atomic::{AtomicUsize, Ordering::SeqCst};
        for n in [0, 1, 7, 1024] {
            let entries: Vec<_> = (0..n).map(|_| AtomicUsize::new(0)).collect();
            let size = run(&entries, 4, |counter| {
                assert_eq!(counter.fetch_add(1, SeqCst), 0);
                4
            });
            assert_eq!(size, 4 * n);
            assert!(entries.iter().all(|c| c.load(SeqCst) == 1));
        }
    }
    #[test]
    fn a_panicking_worker_is_joined_and_the_pool_remains_usable() {
        use std::{
            sync::atomic::{AtomicBool, Ordering::SeqCst},
            thread,
            time::{Duration, Instant},
        };
        let pool = Workers::default();
        let caller = thread::current().id();
        let start = Instant::now();
        let worker = AtomicBool::new(false);
        let error = std::panic::catch_unwind(|| {
            run_on(&pool, &[0; 32], 2, |_| {
                if thread::current().id() != caller {
                    worker.store(true, SeqCst);
                    panic!("upload worker panic");
                }
                while !worker.load(SeqCst) && start.elapsed() < Duration::from_secs(2) {
                    thread::yield_now();
                }
                1
            })
        })
        .unwrap_err();
        assert_eq!(error.downcast_ref::<&str>(), Some(&"upload worker panic"));
        assert_eq!(run_on(&pool, &[1, 2, 3], 4, |n| *n), 6);
    }

    #[test]
    fn caller_panic_waits_for_active_workers() {
        use std::{
            sync::atomic::{AtomicBool, Ordering::SeqCst},
            thread,
            time::{Duration, Instant},
        };
        let pool = Workers::default();
        let caller = thread::current().id();
        let start = Instant::now();
        let active = AtomicBool::new(false);
        let done = AtomicBool::new(false);
        let result = std::panic::catch_unwind(|| {
            run_on(&pool, &[0; 16], 2, |_| {
                if thread::current().id() == caller {
                    while !active.load(SeqCst) && start.elapsed() < Duration::from_secs(2) {
                        thread::yield_now();
                    }
                    panic!("submitter panic");
                }
                active.store(true, SeqCst);
                thread::sleep(Duration::from_millis(2));
                done.store(true, SeqCst);
                1
            })
        });
        assert!(result.is_err() && active.load(SeqCst) && done.load(SeqCst));
    }
}
