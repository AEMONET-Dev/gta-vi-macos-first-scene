//! Wine opens this file once. All concurrent shader workers then communicate
//! through mapped pages; an operational failure falls back to local translation.
use std::fs::OpenOptions;
use std::io;
use std::os::windows::io::AsRawHandle;
use std::path::Path;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering::*};
use std::time::{Duration, Instant};
use victory_pipeline::shader_bridge::*;
use victory_shader::emit::HostShader;
use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::System::Memory::*;

struct Mapping(usize);

impl Mapping {
    fn open(path: &Path) -> anyhow::Result<Self> {
        let file = OpenOptions::new().read(true).write(true).open(path)?;
        anyhow::ensure!(
            file.metadata()?.len() == MAPPING_SIZE as u64,
            "wrong bridge file size"
        );
        // SAFETY: file is open for read/write. The view holds the section
        // alive after closing its handle and the file.
        let view = unsafe {
            let section = CreateFileMappingW(
                file.as_raw_handle(),
                std::ptr::null(),
                PAGE_READWRITE,
                0,
                0,
                std::ptr::null(),
            );
            if section.is_null() {
                return Err(io::Error::last_os_error().into());
            }
            let view = MapViewOfFile(section, FILE_MAP_WRITE, 0, 0, MAPPING_SIZE);
            let error = io::Error::last_os_error();
            CloseHandle(section);
            if view.Value.is_null() {
                return Err(error.into());
            }
            view
        };
        let mapping = Self(view.Value as usize);
        // SAFETY: mapping starts with the shared atomic header.
        let h = unsafe { &*view.Value.cast::<BridgeHeader>() };
        anyhow::ensure!(
            h.magic.load(Acquire) == BRIDGE_MAGIC && h.version.load(Relaxed) == BRIDGE_VERSION,
            "daemon unavailable or incompatible"
        );
        Ok(mapping)
    }

    fn request(
        &self,
        bytes: &[u8],
        locks: bool,
    ) -> anyhow::Result<(Result<HostShader, String>, bool)> {
        let deadline = Instant::now() + Duration::from_secs(30);
        let wait = || -> anyhow::Result<()> {
            anyhow::ensure!(
                Instant::now() < deadline,
                "shader bridge response timed out"
            );
            std::thread::sleep(Duration::from_micros(300));
            Ok(())
        };
        let (s, payload) = loop {
            let free = (0..SLOTS).find_map(|i| {
                // SAFETY: Mapping owns the full view for the duration of the request.
                let (ptr, data) = unsafe { slot(self.0 as *mut u8, i) };
                let s = unsafe { &*ptr };
                s.state
                    .compare_exchange(EMPTY, WRITING, AcqRel, Relaxed)
                    .ok()
                    .map(|_| (s, data))
            });
            if let Some(free) = free {
                break free;
            }
            wait()?;
        };
        // SAFETY: we won WRITING; caller checked capacity, and neither source aliases the view.
        unsafe {
            payload.copy_from_nonoverlapping(super::shader_cache::fingerprint(locks).as_ptr(), 32);
            payload
                .add(32)
                .copy_from_nonoverlapping(bytes.as_ptr(), bytes.len());
        }
        s.opcode
            .store(BridgeOpcode::TranslateHostShader as u32, Relaxed);
        s.option.store(u32::from(locks), Relaxed);
        s.length.store((32 + bytes.len()) as u32, Relaxed);
        s.state.store(REQUEST, Release);
        while s.state.load(Acquire) != RESPONSE {
            wait()?;
        }
        let n = s.length.load(Relaxed) as usize;
        let status = s.status.load(Relaxed);
        let hit = s.cache_hit.load(Relaxed) != 0;
        let reply = if status == 0 && n <= CAPACITY {
            // SAFETY: RESPONSE transfers these initialized bytes until we release the slot.
            super::shader_cache::decode(unsafe { std::slice::from_raw_parts(payload, n) })
        } else {
            None
        };
        s.state.store(EMPTY, Release);
        anyhow::ensure!(status == 0, "native shader bridge status {status}");
        Ok((
            reply.ok_or_else(|| anyhow::anyhow!("invalid native shader cache entry"))?,
            hit,
        ))
    }
}

impl Drop for Mapping {
    fn drop(&mut self) {
        // SAFETY: this exact view was returned by MapViewOfFile.
        unsafe {
            UnmapViewOfFile(MEMORY_MAPPED_VIEW_ADDRESS {
                Value: self.0 as *mut _,
            });
        }
    }
}

pub(crate) fn translate(bytes: &[u8], locks: bool) -> Option<(Result<HostShader, String>, bool)> {
    static CLIENT: OnceLock<Option<Mapping>> = OnceLock::new();
    static FAILED: AtomicBool = AtomicBool::new(false);
    let mapping = CLIENT
        .get_or_init(|| {
            let path = std::env::var_os("VICTORY_SHADER_BRIDGE")?;
            match Mapping::open(Path::new(&path)) {
                Ok(mapping) => {
                    tracing::info!(?path, "native shader bridge connected");
                    Some(mapping)
                }
                Err(e) => {
                    tracing::warn!(%e, "native shader bridge unavailable; using local translator");
                    None
                }
            }
        })
        .as_ref()?;
    if FAILED.load(Relaxed) {
        return None;
    }
    if bytes.len() > CAPACITY - 32 {
        tracing::warn!(
            bytes = bytes.len(),
            "shader too large for bridge; translating locally"
        );
        return None;
    }
    match mapping.request(bytes, locks) {
        Ok(reply) => Some(reply),
        Err(e) => {
            if !FAILED.swap(true, Relaxed) {
                tracing::warn!(%e, "native shader bridge failed; using local translator");
            }
            None
        }
    }
}
