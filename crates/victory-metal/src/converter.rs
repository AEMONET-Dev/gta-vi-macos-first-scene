//! Native macOS DXIL → Metal IR (`.metallib`) compiler via Apple's
//! `libmetalirconverter.dylib` (Metal Shader Converter C API).

use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};

const RTLD_NOW: c_int = 0x2;
const RTLD_LOCAL: c_int = 0x4;

unsafe extern "C" {
    fn dlopen(path: *const c_char, mode: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    fn dlclose(handle: *mut c_void) -> c_int;
    fn dlerror() -> *const c_char;
}

/// Apple Metal Shader Converter shader stage enum (`IRShaderStage`).
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShaderStage {
    Vertex = 1,
    Fragment = 2,
    Geometry = 3,
    Hull = 4,
    Domain = 5,
    Compute = 6,
    Amplification = 7,
    Mesh = 8,
}

/// `IRBytecodeOwnership` (0 = None, 1 = Copy).
const IR_BYTECODE_OWNERSHIP_NONE: u32 = 0;
/// `IROperatingSystem` (0 = macOS).
const IR_OS_MACOS: u32 = 0;
/// `IRGPUFamily` (5001 = Metal3).
const IR_GPU_FAMILY_METAL3: u32 = 5001;

type FnCompilerCreate = unsafe extern "C" fn() -> *mut c_void;
type FnCompilerDestroy = unsafe extern "C" fn(*mut c_void);
type FnCompilerSetMinTarget = unsafe extern "C" fn(*mut c_void, u32, *const c_char);
type FnCompilerSetMinGpu = unsafe extern "C" fn(*mut c_void, u32);
type FnCompilerSetEntryPoint = unsafe extern "C" fn(*mut c_void, *const c_char);
type FnCompilerCompileAndLink =
    unsafe extern "C" fn(*mut c_void, *const c_char, *const c_void, *mut *mut c_void) -> *mut c_void;
type FnObjectCreateFromDxil = unsafe extern "C" fn(*const u8, usize, u32) -> *mut c_void;
type FnObjectDestroy = unsafe extern "C" fn(*mut c_void);
type FnObjectGetStage = unsafe extern "C" fn(*const c_void) -> u32;
type FnObjectGetMetalLib = unsafe extern "C" fn(*const c_void, u32, *mut c_void) -> bool;
type FnMetalLibCreate = unsafe extern "C" fn() -> *mut c_void;
type FnMetalLibDestroy = unsafe extern "C" fn(*mut c_void);
type FnMetalLibGetSize = unsafe extern "C" fn(*const c_void) -> usize;
type FnMetalLibGetBytes = unsafe extern "C" fn(*const c_void, *mut u8) -> usize;
type FnErrorGetCode = unsafe extern "C" fn(*const c_void) -> u32;
type FnErrorGetPayload = unsafe extern "C" fn(*const c_void) -> *const c_void;
type FnErrorDestroy = unsafe extern "C" fn(*mut c_void);

/// Dynamically loaded `libmetalirconverter.dylib` handle and entry points.
pub struct MetalIrConverter {
    lib: *mut c_void,
    path: PathBuf,
    compiler_create: FnCompilerCreate,
    compiler_destroy: FnCompilerDestroy,
    compiler_set_min_target: FnCompilerSetMinTarget,
    compiler_set_min_gpu: FnCompilerSetMinGpu,
    compiler_set_entry_point: FnCompilerSetEntryPoint,
    compiler_compile_and_link: FnCompilerCompileAndLink,
    object_create_from_dxil: FnObjectCreateFromDxil,
    object_destroy: FnObjectDestroy,
    object_get_stage: FnObjectGetStage,
    object_get_metallib: FnObjectGetMetalLib,
    metallib_create: FnMetalLibCreate,
    metallib_destroy: FnMetalLibDestroy,
    metallib_get_size: FnMetalLibGetSize,
    metallib_get_bytes: FnMetalLibGetBytes,
    error_get_code: FnErrorGetCode,
    error_get_payload: FnErrorGetPayload,
    error_destroy: FnErrorDestroy,
}

// SAFETY: Apple's `libmetalirconverter` creates independent `IRCompiler` objects
// per compilation call; the dylib function pointers are immutable process globals.
unsafe impl Send for MetalIrConverter {}
// SAFETY: as above.
unsafe impl Sync for MetalIrConverter {}

impl Drop for MetalIrConverter {
    fn drop(&mut self) {
        if !self.lib.is_null() {
            // SAFETY: `self.lib` was returned by `dlopen`.
            unsafe { dlclose(self.lib) };
        }
    }
}

impl MetalIrConverter {
    /// Searches standard macOS locations (and `VICTORY_METAL_IR_CONVERTER`) for
    /// `libmetalirconverter.dylib` and loads it.
    pub fn load_default() -> Result<Self> {
        let mut candidates: Vec<PathBuf> = Vec::new();
        if let Ok(env_path) = std::env::var("VICTORY_METAL_IR_CONVERTER") {
            candidates.push(PathBuf::from(env_path));
        }
        candidates.extend([
            PathBuf::from("/usr/local/lib/libmetalirconverter.dylib"),
            PathBuf::from("/opt/homebrew/lib/libmetalirconverter.dylib"),
            PathBuf::from(
                "/Applications/Parallels Desktop.app/Contents/Frameworks/libmetalirconverter.dylib",
            ),
        ]);
        for path in &candidates {
            if path.exists() {
                return Self::load(path);
            }
        }
        bail!(
            "libmetalirconverter.dylib not found in {:?}",
            candidates
        )
    }

    /// Loads `libmetalirconverter.dylib` from an explicit path.
    pub fn load(path: &Path) -> Result<Self> {
        let cpath = CString::new(path.to_string_lossy().as_bytes())
            .context("invalid dylib path")?;
        // SAFETY: `cpath` is a valid NUL-terminated path.
        let lib = unsafe { dlopen(cpath.as_ptr(), RTLD_NOW | RTLD_LOCAL) };
        if lib.is_null() {
            // SAFETY: `dlerror` returns a static or thread-local C string.
            let msg = unsafe {
                let p = dlerror();
                if p.is_null() {
                    "unknown dlopen error".to_string()
                } else {
                    CStr::from_ptr(p).to_string_lossy().into_owned()
                }
            };
            bail!("dlopen({}) failed: {msg}", path.display());
        }
        let load_sym = |name: &str| -> Result<*mut c_void> {
            let cname = CString::new(name).unwrap();
            // SAFETY: `lib` is a live dlopen handle and `cname` is NUL-terminated.
            let p = unsafe { dlsym(lib, cname.as_ptr()) };
            if p.is_null() {
                bail!("symbol {name} not found in {}", path.display());
            }
            Ok(p)
        };
        // SAFETY: each symbol signature matches Apple's Metal Shader Converter C header.
        unsafe {
            Ok(Self {
                lib,
                path: path.to_path_buf(),
                compiler_create: std::mem::transmute(load_sym("IRCompilerCreate")?),
                compiler_destroy: std::mem::transmute(load_sym("IRCompilerDestroy")?),
                compiler_set_min_target: std::mem::transmute(load_sym(
                    "IRCompilerSetMinimumDeploymentTarget",
                )?),
                compiler_set_min_gpu: std::mem::transmute(load_sym(
                    "IRCompilerSetMinimumGPUFamily",
                )?),
                compiler_set_entry_point: std::mem::transmute(load_sym(
                    "IRCompilerSetEntryPointName",
                )?),
                compiler_compile_and_link: std::mem::transmute(load_sym(
                    "IRCompilerAllocCompileAndLink",
                )?),
                object_create_from_dxil: std::mem::transmute(load_sym("IRObjectCreateFromDXIL")?),
                object_destroy: std::mem::transmute(load_sym("IRObjectDestroy")?),
                object_get_stage: std::mem::transmute(load_sym("IRObjectGetMetalIRShaderStage")?),
                object_get_metallib: std::mem::transmute(load_sym("IRObjectGetMetalLibBinary")?),
                metallib_create: std::mem::transmute(load_sym("IRMetalLibBinaryCreate")?),
                metallib_destroy: std::mem::transmute(load_sym("IRMetalLibBinaryDestroy")?),
                metallib_get_size: std::mem::transmute(load_sym("IRMetalLibGetBytecodeSize")?),
                metallib_get_bytes: std::mem::transmute(load_sym("IRMetalLibGetBytecode")?),
                error_get_code: std::mem::transmute(load_sym("IRErrorGetCode")?),
                error_get_payload: std::mem::transmute(load_sym("IRErrorGetPayload")?),
                error_destroy: std::mem::transmute(load_sym("IRErrorDestroy")?),
            })
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Converts a DXBC/DXIL container (`dxil`) directly into an Apple `.metallib` binary.
    pub fn compile_dxil(
        &self,
        dxil: &[u8],
        stage: ShaderStage,
        entry_point: &str,
    ) -> Result<Vec<u8>> {
        let entry = CString::new(entry_point).context("invalid entry point name")?;
        let target_ver = CString::new("15.0").unwrap();
        // SAFETY: all pointers and lifetimes follow Apple's IRCompiler C API contract.
        unsafe {
            let compiler = (self.compiler_create)();
            if compiler.is_null() {
                bail!("IRCompilerCreate returned null");
            }
            (self.compiler_set_min_target)(compiler, IR_OS_MACOS, target_ver.as_ptr());
            (self.compiler_set_min_gpu)(compiler, IR_GPU_FAMILY_METAL3);
            (self.compiler_set_entry_point)(compiler, entry.as_ptr());

            let dxil_obj = (self.object_create_from_dxil)(
                dxil.as_ptr(),
                dxil.len(),
                IR_BYTECODE_OWNERSHIP_NONE,
            );
            if dxil_obj.is_null() {
                (self.compiler_destroy)(compiler);
                bail!("IRObjectCreateFromDXIL returned null (invalid DXIL container)");
            }

            let mut err: *mut c_void = std::ptr::null_mut();
            let out_obj =
                (self.compiler_compile_and_link)(compiler, entry.as_ptr(), dxil_obj, &mut err);
            (self.object_destroy)(dxil_obj);
            (self.compiler_destroy)(compiler);

            if out_obj.is_null() {
                let detail = if !err.is_null() {
                    let code = (self.error_get_code)(err);
                    let payload = (self.error_get_payload)(err);
                    let text = if !payload.is_null() {
                        CStr::from_ptr(payload.cast::<c_char>())
                            .to_string_lossy()
                            .into_owned()
                    } else {
                        String::new()
                    };
                    (self.error_destroy)(err);
                    format!("code={code} {text}")
                } else {
                    "unknown compiler error".to_string()
                };
                return Err(anyhow!("IRCompilerAllocCompileAndLink failed: {detail}"));
            }
            if !err.is_null() {
                (self.error_destroy)(err);
            }

            let detected_stage = (self.object_get_stage)(out_obj);
            let actual_stage = if detected_stage != 0 {
                detected_stage
            } else {
                stage as u32
            };
            let metallib = (self.metallib_create)();
            let ok = (self.object_get_metallib)(out_obj, actual_stage, metallib);
            (self.object_destroy)(out_obj);
            if !ok {
                (self.metallib_destroy)(metallib);
                bail!("IRObjectGetMetalLibBinary returned false for stage {stage:?}");
            }

            let size = (self.metallib_get_size)(metallib);
            let mut bytes = vec![0u8; size];
            let written = (self.metallib_get_bytes)(metallib, bytes.as_mut_ptr());
            (self.metallib_destroy)(metallib);
            bytes.truncate(written);
            Ok(bytes)
        }
    }

    /// End-to-end native ARM64 translation: Xbox GFX10 container → DXIL (`victory-shader`)
    /// → Apple Metal IR `.metallib` (`libmetalirconverter.dylib`).
    pub fn translate_xbox_container(&self, container: &[u8]) -> Result<(victory_shader::emit::HostShader, Vec<u8>)> {
        let host = victory_shader::emit::translate(container)
            .map_err(|e| anyhow!("victory-shader translation failed: {e}"))?;
        let stage = match host.stage {
            victory_shader::HwStage::Vs => ShaderStage::Vertex,
            victory_shader::HwStage::Ps => ShaderStage::Fragment,
            victory_shader::HwStage::Cs => ShaderStage::Compute,
            victory_shader::HwStage::Ms => ShaderStage::Mesh,
            other => bail!("unsupported stage for Metal IR conversion: {other:?}"),
        };
        let metallib = self.compile_dxil(&host.dxil, stage, "main")?;
        Ok((host, metallib))
    }
}
