//! Native macOS dirty-page tracker (`mprotect` + `SIGBUS`/`SIGSEGV` handler).
//!
//! Under Wine + Rosetta 2, Windows `VirtualProtect` page-fault tracking fails, forcing
//! VIctory to re-copy 15–99 GB/frame whole. On native macOS, `mprotect(PROT_READ)` +
//! `sigaction` tracks exact 16 KiB Apple Silicon dirty pages with near-zero overhead.

use std::ffi::{c_int, c_void};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Mutex, Once};

use anyhow::{Result, bail};

const PROT_READ: c_int = 0x1;
const PROT_WRITE: c_int = 0x2;
const MAP_PRIVATE: c_int = 0x0002;
const MAP_ANON: c_int = 0x1000;
const SIGBUS: c_int = 10;
const SIGSEGV: c_int = 11;
const SA_SIGINFO: c_int = 0x0040;
const _SC_PAGESIZE: c_int = 29;

#[repr(C)]
struct SigAction {
    sa_sigaction: usize,
    sa_mask: u32,
    sa_flags: c_int,
}

#[repr(C)]
struct SigInfo {
    si_signo: c_int,
    si_errno: c_int,
    si_code: c_int,
    si_pid: i32,
    si_uid: u32,
    si_status: c_int,
    si_addr: *mut c_void,
}

unsafe extern "C" {
    fn sysconf(name: c_int) -> isize;
    fn mmap(
        addr: *mut c_void,
        len: usize,
        prot: c_int,
        flags: c_int,
        fd: c_int,
        offset: i64,
    ) -> *mut c_void;
    fn munmap(addr: *mut c_void, len: usize) -> c_int;
    fn mprotect(addr: *mut c_void, len: usize, prot: c_int) -> c_int;
    fn sigaction(sig: c_int, act: *const SigAction, oact: *mut SigAction) -> c_int;
}

/// Returns the macOS hardware page size (16,384 bytes on Apple Silicon).
pub fn page_size() -> usize {
    // SAFETY: `sysconf(_SC_PAGESIZE)` is a standard read-only POSIX query.
    let ps = unsafe { sysconf(_SC_PAGESIZE) };
    if ps > 0 { ps as usize } else { 16384 }
}

// Single active watched region registry (signal-safe atomics for zero-lock fault handling).
static WATCH_BASE: AtomicUsize = AtomicUsize::new(0);
static WATCH_LEN: AtomicUsize = AtomicUsize::new(0);
static WATCH_PAGE: AtomicUsize = AtomicUsize::new(16384);
static WATCH_BITMAP_PTR: AtomicUsize = AtomicUsize::new(0);
static WATCH_FAULTS: AtomicU64 = AtomicU64::new(0);
static INSTALL_ONCE: Once = Once::new();
static WATCH_LOCK: Mutex<()> = Mutex::new(());

unsafe extern "C" fn fault_handler(sig: c_int, info: *mut SigInfo, _ctx: *mut c_void) {
    if info.is_null() {
        std::process::abort();
    }
    // SAFETY: `info` is provided by the Darwin kernel for `SA_SIGINFO` handlers.
    let addr = unsafe { (*info).si_addr as usize };
    let base = WATCH_BASE.load(Ordering::Acquire);
    let len = WATCH_LEN.load(Ordering::Acquire);
    if base != 0 && addr >= base && addr < base + len {
        let ps = WATCH_PAGE.load(Ordering::Relaxed);
        let page = (addr - base) / ps;
        let page_addr = base + page * ps;
        let bmp = WATCH_BITMAP_PTR.load(Ordering::Acquire) as *const AtomicU64;
        if !bmp.is_null() {
            // SAFETY: `page / 64` is within the allocated bitmap for `[base, base + len)`.
            unsafe {
                (*bmp.add(page / 64)).fetch_or(1u64 << (page % 64), Ordering::Release);
            }
        }
        WATCH_FAULTS.fetch_add(1, Ordering::Relaxed);
        // SAFETY: `page_addr` is page-aligned inside our `mmap` region; `mprotect` is
        // async-signal-safe on Darwin.
        let rc = unsafe { mprotect(page_addr as *mut c_void, ps, PROT_READ | PROT_WRITE) };
        if rc == 0 {
            return;
        }
    }
    let _ = sig;
    std::process::abort();
}

fn install_handlers() {
    INSTALL_ONCE.call_once(|| {
        let act = SigAction {
            sa_sigaction: fault_handler as *const () as usize,
            sa_mask: 0,
            sa_flags: SA_SIGINFO,
        };
        // SAFETY: installs our `SA_SIGINFO` handler for `SIGBUS` and `SIGSEGV`.
        unsafe {
            sigaction(SIGBUS, &act, std::ptr::null_mut());
            sigaction(SIGSEGV, &act, std::ptr::null_mut());
        }
    });
}

/// A page-aligned `mmap` region with native macOS write-watch dirty-page tracking.
///
/// The backing memory is also page-aligned so it can be passed directly to
/// [`crate::metal::MetalDevice::new_buffer_no_copy`] for zero-copy GPU access!
pub struct WatchedRegion {
    base: *mut u8,
    len: usize,
    page_size: usize,
    bitmap: Box<[AtomicU64]>,
    _guard: std::sync::MutexGuard<'static, ()>,
}

impl WatchedRegion {
    /// Allocates `pages * page_size()` bytes of page-aligned RAM and enables write-watch.
    pub fn new(pages: usize) -> Result<Self> {
        let guard = WATCH_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        install_handlers();
        let ps = page_size();
        let len = pages
            .checked_mul(ps)
            .ok_or_else(|| anyhow::anyhow!("region size overflow"))?;
        // SAFETY: anonymous private `mmap` allocation.
        let ptr = unsafe {
            mmap(
                std::ptr::null_mut(),
                len,
                PROT_READ | PROT_WRITE,
                MAP_PRIVATE | MAP_ANON,
                -1,
                0,
            )
        };
        if ptr.is_null() || (ptr as isize) == -1 {
            bail!("mmap({len}) failed");
        }
        let words = pages.div_ceil(64);
        let bitmap: Box<[AtomicU64]> = (0..words)
            .map(|_| AtomicU64::new(0))
            .collect::<Vec<_>>()
            .into_boxed_slice();
        WATCH_PAGE.store(ps, Ordering::Relaxed);
        WATCH_BITMAP_PTR.store(bitmap.as_ptr() as usize, Ordering::Release);
        WATCH_BASE.store(ptr as usize, Ordering::Release);
        WATCH_LEN.store(len, Ordering::Release);
        Ok(Self {
            base: ptr.cast::<u8>(),
            len,
            page_size: ps,
            bitmap,
            _guard: guard,
        })
    }

    pub fn as_mut_ptr(&self) -> *mut u8 {
        self.base
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn page_size(&self) -> usize {
        self.page_size
    }

    /// Marks the entire region read-only (`PROT_READ`) and clears dirty bits.
    pub fn arm(&self) -> Result<()> {
        for w in &self.bitmap {
            w.store(0, Ordering::Release);
        }
        // SAFETY: `self.base..self.base+self.len` is our page-aligned `mmap` region.
        let rc = unsafe { mprotect(self.base.cast(), self.len, PROT_READ) };
        if rc != 0 {
            bail!("mprotect(PROT_READ) failed with {rc}");
        }
        Ok(())
    }

    /// Returns the list of dirty page indices written since the last [`Self::arm`].
    pub fn take_dirty_pages(&self) -> Vec<usize> {
        let mut dirty = Vec::new();
        for (wi, word) in self.bitmap.iter().enumerate() {
            let mut bits = word.swap(0, Ordering::AcqRel);
            while bits != 0 {
                let bit = bits.trailing_zeros() as usize;
                dirty.push(wi * 64 + bit);
                bits &= bits - 1;
            }
        }
        dirty
    }

    pub fn total_faults() -> u64 {
        WATCH_FAULTS.load(Ordering::Relaxed)
    }
}

impl Drop for WatchedRegion {
    fn drop(&mut self) {
        WATCH_BASE.store(0, Ordering::Release);
        WATCH_LEN.store(0, Ordering::Release);
        WATCH_BITMAP_PTR.store(0, Ordering::Release);
        // SAFETY: `self.base` was returned by `mmap(len)`.
        unsafe {
            let _ = mprotect(self.base.cast(), self.len, PROT_READ | PROT_WRITE);
            let _ = munmap(self.base.cast(), self.len);
        }
    }
}
