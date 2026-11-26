//! Native macOS Apple Silicon Metal backend & zero-copy runtime for VIctory.
//!
//! Architecture stages:
//! 1. **Native DXIL → Metal IR (`.metallib`)** via Apple's `libmetalirconverter.dylib`
//!    (`converter`), paired with native ARM64 `victory-shader` (GFX10 → DXIL).
//! 2. **Direct Metal.framework FFI** (`metal`) with Apple Silicon unified-memory
//!    zero-copy buffers (`newBufferWithBytesNoCopy` / `MTLStorageModeShared`) and
//!    asynchronous `MTLCommandBuffer` dispatch.
//! 3. **Native macOS dirty-page tracking** (`watch`) using `mprotect` + `SIGBUS`/`SIGSEGV`
//!    over 16 KiB Apple Silicon pages, replacing Wine's broken `ReadProcessMemory` full copies.
//! 4. **Shared-memory bridge protocol** (`bridge`) for offloading shader compilation and
//!    GPU command streams from Wine `victory.exe` to a native ARM64 macOS Metal daemon.

#[cfg(target_os = "macos")]
pub mod bridge;
#[cfg(target_os = "macos")]
pub mod converter;
#[cfg(target_os = "macos")]
pub mod metal;
#[cfg(target_os = "macos")]
pub mod watch;
