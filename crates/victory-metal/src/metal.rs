//! Direct macOS `Metal.framework` FFI (zero external crates).
//!
//! Provides native Apple Silicon `MTLDevice`, `MTLCommandQueue`, `.metallib` loading,
//! `MTLComputePipelineState`, unified-memory zero-copy `MTLBuffer`
//! (`newBufferWithBytesNoCopy` / `MTLResourceStorageModeShared`), and GPU compute dispatch.

use std::ffi::{CStr, CString, c_char, c_void};

use anyhow::{Result, bail};

#[link(name = "Metal", kind = "framework")]
#[link(name = "Foundation", kind = "framework")]
unsafe extern "C" {
    fn MTLCreateSystemDefaultDevice() -> *mut c_void;
    fn objc_getClass(name: *const c_char) -> *mut c_void;
    fn sel_registerName(name: *const c_char) -> *mut c_void;
    fn objc_msgSend();
    fn dispatch_data_create(
        buffer: *const c_void,
        size: usize,
        queue: *mut c_void,
        destructor: *const c_void,
    ) -> *mut c_void;
    fn dispatch_release(object: *mut c_void);
}

/// `MTLResourceStorageModeShared` = 0 (Apple Silicon CPU+GPU unified memory).
pub const MTL_RESOURCE_STORAGE_MODE_SHARED: usize = 0;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct MtlSize {
    pub width: usize,
    pub height: usize,
    pub depth: usize,
}

fn sel(name: &str) -> *mut c_void {
    let c = CString::new(name).unwrap();
    // SAFETY: `c` is a valid NUL-terminated selector name.
    unsafe { sel_registerName(c.as_ptr()) }
}

unsafe fn retain(obj: *mut c_void) -> *mut c_void {
    if obj.is_null() {
        return obj;
    }
    let f: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { f(obj, sel("retain")) }
}

unsafe fn release(obj: *mut c_void) {
    if !obj.is_null() {
        let f: unsafe extern "C" fn(*mut c_void, *mut c_void) =
            unsafe { std::mem::transmute(objc_msgSend as *const ()) };
        unsafe { f(obj, sel("release")) };
    }
}

unsafe fn nsstring(s: &str) -> *mut c_void {
    let cls = unsafe { objc_getClass(c"NSString".as_ptr()) };
    let c = CString::new(s).unwrap();
    let f: unsafe extern "C" fn(*mut c_void, *mut c_void, *const c_char) -> *mut c_void =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { f(cls, sel("stringWithUTF8String:"), c.as_ptr()) }
}

unsafe fn nsstring_to_rust(ns: *mut c_void) -> String {
    if ns.is_null() {
        return String::new();
    }
    let f: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *const c_char =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    let ptr = unsafe { f(ns, sel("UTF8String")) };
    if ptr.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(ptr) }.to_string_lossy().into_owned()
    }
}

unsafe fn nserror_desc(err: *mut c_void) -> String {
    if err.is_null() {
        return "unknown Metal error".to_string();
    }
    let f: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    let desc = unsafe { f(err, sel("localizedDescription")) };
    unsafe { nsstring_to_rust(desc) }
}

/// Owned handle to a native `MTLDevice`.
pub struct MetalDevice {
    raw: *mut c_void,
}

// SAFETY: `MTLDevice` is thread-safe per Apple Metal documentation.
unsafe impl Send for MetalDevice {}
// SAFETY: as above.
unsafe impl Sync for MetalDevice {}

impl Drop for MetalDevice {
    fn drop(&mut self) {
        // SAFETY: `self.raw` is an owned Objective-C object.
        unsafe { release(self.raw) };
    }
}

/// Owned handle to a native `MTLCommandQueue`.
pub struct MetalQueue {
    raw: *mut c_void,
}

impl Drop for MetalQueue {
    fn drop(&mut self) {
        // SAFETY: `self.raw` is an owned Objective-C object.
        unsafe { release(self.raw) };
    }
}

/// Owned handle to a native `MTLLibrary`.
pub struct MetalLibrary {
    raw: *mut c_void,
}

impl Drop for MetalLibrary {
    fn drop(&mut self) {
        // SAFETY: `self.raw` is an owned Objective-C object.
        unsafe { release(self.raw) };
    }
}

/// Owned handle to a native `MTLComputePipelineState`.
pub struct MetalComputePipeline {
    raw: *mut c_void,
}

impl Drop for MetalComputePipeline {
    fn drop(&mut self) {
        // SAFETY: `self.raw` is an owned Objective-C object.
        unsafe { release(self.raw) };
    }
}

/// Owned handle to a native `MTLBuffer` (shared or zero-copy unified memory).
pub struct MetalBuffer {
    raw: *mut c_void,
    len: usize,
}

impl Drop for MetalBuffer {
    fn drop(&mut self) {
        // SAFETY: `self.raw` is an owned Objective-C object.
        unsafe { release(self.raw) };
    }
}

impl MetalDevice {
    /// Returns the system default Apple Silicon `MTLDevice`.
    pub fn system_default() -> Result<Self> {
        // SAFETY: `MTLCreateSystemDefaultDevice` returns a +1 retained `MTLDevice` or null.
        let raw = unsafe { MTLCreateSystemDefaultDevice() };
        if raw.is_null() {
            bail!("MTLCreateSystemDefaultDevice returned null");
        }
        Ok(Self { raw })
    }

    /// Returns the human-readable GPU name (e.g., `"Apple M5 Max"`).
    pub fn name(&self) -> String {
        // SAFETY: `[self.raw name]` returns an `NSString*`.
        unsafe {
            let f: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
                std::mem::transmute(objc_msgSend as *const ());
            let ns = f(self.raw, sel("name"));
            nsstring_to_rust(ns)
        }
    }

    /// Whether the GPU shares unified memory with the CPU (`hasUnifiedMemory`).
    pub fn has_unified_memory(&self) -> bool {
        // SAFETY: `[self.raw hasUnifiedMemory]` returns `BOOL` (`u8`).
        unsafe {
            let f: unsafe extern "C" fn(*mut c_void, *mut c_void) -> u8 =
                std::mem::transmute(objc_msgSend as *const ());
            f(self.raw, sel("hasUnifiedMemory")) != 0
        }
    }

    /// Creates a native `MTLCommandQueue`.
    pub fn new_command_queue(&self) -> Result<MetalQueue> {
        // SAFETY: `[self.raw newCommandQueue]` returns a +1 retained `MTLCommandQueue`.
        let raw = unsafe {
            let f: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
                std::mem::transmute(objc_msgSend as *const ());
            f(self.raw, sel("newCommandQueue"))
        };
        if raw.is_null() {
            bail!("newCommandQueue returned null");
        }
        Ok(MetalQueue { raw })
    }

    /// Loads a compiled `.metallib` binary into an `MTLLibrary`.
    pub fn new_library_from_metallib(&self, metallib: &[u8]) -> Result<MetalLibrary> {
        // SAFETY: `dispatch_data_create` with default destructor copies `metallib` safely;
        // `newLibraryWithData:error:` returns a +1 retained `MTLLibrary`.
        unsafe {
            let data = dispatch_data_create(
                metallib.as_ptr().cast(),
                metallib.len(),
                std::ptr::null_mut(),
                std::ptr::null(),
            );
            let mut err: *mut c_void = std::ptr::null_mut();
            let f: unsafe extern "C" fn(
                *mut c_void,
                *mut c_void,
                *mut c_void,
                *mut *mut c_void,
            ) -> *mut c_void = std::mem::transmute(objc_msgSend as *const ());
            let lib = f(self.raw, sel("newLibraryWithData:error:"), data, &mut err);
            dispatch_release(data);
            if lib.is_null() {
                bail!("newLibraryWithData failed: {}", nserror_desc(err));
            }
            Ok(MetalLibrary { raw: lib })
        }
    }

    /// Compiles Metal Shading Language (MSL) source directly into an `MTLLibrary`.
    pub fn new_library_from_source(&self, source: &str) -> Result<MetalLibrary> {
        // SAFETY: `newLibraryWithSource:options:error:` returns a +1 retained `MTLLibrary`.
        unsafe {
            let src = nsstring(source);
            let mut err: *mut c_void = std::ptr::null_mut();
            let f: unsafe extern "C" fn(
                *mut c_void,
                *mut c_void,
                *mut c_void,
                *mut c_void,
                *mut *mut c_void,
            ) -> *mut c_void = std::mem::transmute(objc_msgSend as *const ());
            let lib = f(
                self.raw,
                sel("newLibraryWithSource:options:error:"),
                src,
                std::ptr::null_mut(),
                &mut err,
            );
            if lib.is_null() {
                bail!("newLibraryWithSource failed: {}", nserror_desc(err));
            }
            Ok(MetalLibrary { raw: lib })
        }
    }

    /// Creates an `MTLComputePipelineState` from a function inside `library`.
    pub fn new_compute_pipeline(
        &self,
        library: &MetalLibrary,
        function_name: &str,
    ) -> Result<MetalComputePipeline> {
        // SAFETY: `[library newFunctionWithName:]` and `[device newComputePipelineStateWithFunction:error:]`
        // each return +1 retained objects.
        unsafe {
            let name = nsstring(function_name);
            let f_fn: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) -> *mut c_void =
                std::mem::transmute(objc_msgSend as *const ());
            let func = f_fn(library.raw, sel("newFunctionWithName:"), name);
            if func.is_null() {
                bail!("Metal function '{function_name}' not found in library");
            }
            let mut err: *mut c_void = std::ptr::null_mut();
            let f_pso: unsafe extern "C" fn(
                *mut c_void,
                *mut c_void,
                *mut c_void,
                *mut *mut c_void,
            ) -> *mut c_void = std::mem::transmute(objc_msgSend as *const ());
            let pso = f_pso(
                self.raw,
                sel("newComputePipelineStateWithFunction:error:"),
                func,
                &mut err,
            );
            release(func);
            if pso.is_null() {
                bail!(
                    "newComputePipelineStateWithFunction failed: {}",
                    nserror_desc(err)
                );
            }
            Ok(MetalComputePipeline { raw: pso })
        }
    }

    /// Allocates a unified-memory `MTLStorageModeShared` buffer.
    pub fn new_buffer_shared(&self, len: usize) -> Result<MetalBuffer> {
        // SAFETY: `[self.raw newBufferWithLength:options:]` returns a +1 retained `MTLBuffer`.
        let raw = unsafe {
            let f: unsafe extern "C" fn(*mut c_void, *mut c_void, usize, usize) -> *mut c_void =
                std::mem::transmute(objc_msgSend as *const ());
            f(
                self.raw,
                sel("newBufferWithLength:options:"),
                len,
                MTL_RESOURCE_STORAGE_MODE_SHARED,
            )
        };
        if raw.is_null() {
            bail!("newBufferWithLength({len}) returned null");
        }
        Ok(MetalBuffer { raw, len })
    }

    /// Wraps an existing page-aligned host RAM region as a **zero-copy** unified-memory
    /// `MTLBuffer` (`newBufferWithBytesNoCopy:length:options:deallocator:`).
    ///
    /// # Safety
    /// `ptr` must be page-aligned (`16 KiB` on Apple Silicon), `len` must be a non-zero
    /// multiple of page size, and the backing memory must outlive the returned `MetalBuffer`.
    pub unsafe fn new_buffer_no_copy(&self, ptr: *mut u8, len: usize) -> Result<MetalBuffer> {
        let raw = unsafe {
            let f: unsafe extern "C" fn(
                *mut c_void,
                *mut c_void,
                *mut c_void,
                usize,
                usize,
                *mut c_void,
            ) -> *mut c_void = std::mem::transmute(objc_msgSend as *const ());
            f(
                self.raw,
                sel("newBufferWithBytesNoCopy:length:options:deallocator:"),
                ptr.cast(),
                len,
                MTL_RESOURCE_STORAGE_MODE_SHARED,
                std::ptr::null_mut(),
            )
        };
        if raw.is_null() {
            bail!("newBufferWithBytesNoCopy({ptr:?}, {len}) returned null");
        }
        Ok(MetalBuffer { raw, len })
    }
}

impl MetalBuffer {
    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Direct CPU pointer into the shared/zero-copy GPU buffer (`[buffer contents]`).
    pub fn contents(&self) -> *mut u8 {
        // SAFETY: `[self.raw contents]` returns the mapped CPU pointer for shared buffers.
        unsafe {
            let f: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
                std::mem::transmute(objc_msgSend as *const ());
            f(self.raw, sel("contents")).cast::<u8>()
        }
    }
}

impl MetalQueue {
    /// Encodes and synchronously executes a single 1D/3D compute dispatch on the GPU.
    pub fn dispatch_compute_sync(
        &self,
        pso: &MetalComputePipeline,
        buffers: &[&MetalBuffer],
        grid: MtlSize,
        threads: MtlSize,
    ) -> Result<()> {
        // SAFETY: standard Metal command buffer / compute encoder lifecycle.
        unsafe {
            let f0: unsafe extern "C" fn(*mut c_void, *mut c_void) -> *mut c_void =
                std::mem::transmute(objc_msgSend as *const ());
            let cmd = retain(f0(self.raw, sel("commandBuffer")));
            if cmd.is_null() {
                bail!("commandBuffer returned null");
            }
            let enc = retain(f0(cmd, sel("computeCommandEncoder")));
            if enc.is_null() {
                release(cmd);
                bail!("computeCommandEncoder returned null");
            }
            let f_pso: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void) =
                std::mem::transmute(objc_msgSend as *const ());
            f_pso(enc, sel("setComputePipelineState:"), pso.raw);

            let f_buf: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut c_void, usize, usize) =
                std::mem::transmute(objc_msgSend as *const ());
            for (idx, buf) in buffers.iter().enumerate() {
                f_buf(enc, sel("setBuffer:offset:atIndex:"), buf.raw, 0, idx);
            }

            let f_disp: unsafe extern "C" fn(*mut c_void, *mut c_void, MtlSize, MtlSize) =
                std::mem::transmute(objc_msgSend as *const ());
            f_disp(
                enc,
                sel("dispatchThreadgroups:threadsPerThreadgroup:"),
                grid,
                threads,
            );

            let f_void: unsafe extern "C" fn(*mut c_void, *mut c_void) =
                std::mem::transmute(objc_msgSend as *const ());
            f_void(enc, sel("endEncoding"));
            release(enc);

            f_void(cmd, sel("commit"));
            f_void(cmd, sel("waitUntilCompleted"));
            release(cmd);
            Ok(())
        }
    }
}
