//! Native shader translator serving a session-specific shared file.
#[cfg(target_os = "macos")]
mod server {
    use anyhow::{Context, Result, bail, ensure};
    use std::fs::{File, OpenOptions};
    use std::os::fd::AsRawFd;
    use std::os::unix::fs::OpenOptionsExt;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::*};
    use std::time::Duration;
    use victory_gpu::shader_cache;
    use victory_metal::bridge::*;
    use victory_metal::converter::{MetalIrConverter, ShaderStage};
    use victory_metal::metal::MetalDevice;

    struct Mapping(*mut u8);
    // SAFETY: the mapped address is stable; workers own separate slots,
    // and access their payload only under the shared state handshake.
    unsafe impl Sync for Mapping {}

    impl Mapping {
        fn create(file: &File) -> Result<Self> {
            unsafe extern "C" {
                fn mmap(
                    addr: *mut u8,
                    len: usize,
                    prot: i32,
                    flags: i32,
                    fd: i32,
                    offset: i64,
                ) -> *mut u8;
            }
            file.set_len(MAPPING_SIZE as u64)?;
            // SAFETY: a fresh file of the full mapping size, read/write, MAP_SHARED.
            let p = unsafe {
                mmap(
                    std::ptr::null_mut(),
                    MAPPING_SIZE,
                    3,
                    1,
                    file.as_raw_fd(),
                    0,
                )
            };
            if p as isize == -1 {
                return Err(std::io::Error::last_os_error().into());
            }
            let mapping = Self(p);
            let h = unsafe { &*p.cast::<BridgeHeader>() };
            h.version.store(BRIDGE_VERSION, Relaxed);
            h.magic.store(BRIDGE_MAGIC, Release);
            Ok(mapping)
        }
    }

    impl Drop for Mapping {
        fn drop(&mut self) {
            unsafe extern "C" {
                fn munmap(addr: *mut u8, len: usize) -> i32;
            }
            // SAFETY: all scoped workers have joined.
            unsafe {
                munmap(self.0, MAPPING_SIZE);
            }
        }
    }

    #[derive(Default)]
    struct Counts {
        hits: AtomicU64,
        misses: AtomicU64,
        refusals: AtomicU64,
        errors: AtomicU64,
    }

    struct Reply {
        first: Vec<u8>,
        second: Vec<u8>,
        hit: bool,
    }

    fn serve(op: u32, option: u32, input: &[u8], gpu_name: &str, counts: &Counts) -> Result<Reply> {
        let mut second = Vec::new();
        let mut hit = false;
        let first = match op {
            x if x == BridgeOpcode::TranslateHostShader as u32 => {
                let (key, container) = input
                    .split_at_checked(32)
                    .context("missing translator fingerprint")?;
                ensure!(
                    option <= 1 && key == shader_cache::fingerprint(option == 1),
                    "translator build or settings mismatch"
                );
                let (shader, cached) = shader_cache::translate_local(container, option == 1);
                hit = cached;
                if cached {
                    counts.hits.fetch_add(1, Relaxed);
                } else {
                    counts.misses.fetch_add(1, Relaxed);
                }
                if shader.is_err() {
                    counts.refusals.fetch_add(1, Relaxed);
                }
                shader_cache::encode(&shader)
            }
            x if x == BridgeOpcode::Ping as u32 => gpu_name.as_bytes().to_vec(),
            x if x == BridgeOpcode::TranslateShader as u32 => {
                let (host, lib) =
                    MetalIrConverter::load_default()?.translate_xbox_container(input)?;
                second = lib;
                host.dxil
            }
            x if x == BridgeOpcode::CompileDxilToMetalLib as u32 => {
                let stage = match option {
                    1 => ShaderStage::Vertex,
                    2 => ShaderStage::Fragment,
                    3 => ShaderStage::Geometry,
                    4 => ShaderStage::Hull,
                    5 => ShaderStage::Domain,
                    6 => ShaderStage::Compute,
                    7 => ShaderStage::Amplification,
                    8 => ShaderStage::Mesh,
                    _ => bail!("invalid Metal shader stage {option}"),
                };
                MetalIrConverter::load_default()?.compile_dxil(input, stage, "main")?
            }
            _ => bail!("unknown bridge operation {op}"),
        };
        ensure!(
            first.len().saturating_add(second.len()) <= CAPACITY,
            "response exceeds slot capacity"
        );
        Ok(Reply { first, second, hit })
    }

    fn worker(map: &Mapping, i: usize, stop: &AtomicBool, counts: &Counts, gpu: &str) {
        // SAFETY: map spans all slots for the lifetime of this worker.
        let (slot, payload) = unsafe { slot(map.0, i) };
        let s = unsafe { &*slot };
        while !stop.load(Relaxed) {
            if s.state
                .compare_exchange(REQUEST, PROCESSING, Acquire, Relaxed)
                .is_err()
            {
                std::thread::sleep(Duration::from_millis(1));
                continue;
            }
            let n = s.length.load(Relaxed) as usize;
            let reply = if n <= CAPACITY {
                // SAFETY: the requesting client published these bytes and is waiting.
                let input = unsafe { std::slice::from_raw_parts(payload, n) };
                serve(
                    s.opcode.load(Relaxed),
                    s.option.load(Relaxed),
                    input,
                    gpu,
                    counts,
                )
            } else {
                Err(anyhow::anyhow!("request exceeds slot capacity"))
            };
            match reply {
                Ok(reply) => {
                    // SAFETY: serve checked total capacity; the input borrow has ended.
                    unsafe {
                        payload.copy_from_nonoverlapping(reply.first.as_ptr(), reply.first.len());
                        payload
                            .add(reply.first.len())
                            .copy_from_nonoverlapping(reply.second.as_ptr(), reply.second.len());
                    }
                    s.length.store(reply.first.len() as u32, Relaxed);
                    s.second_length.store(reply.second.len() as u32, Relaxed);
                    s.cache_hit.store(u32::from(reply.hit), Relaxed);
                    s.status.store(0, Relaxed);
                }
                Err(e) => {
                    counts.errors.fetch_add(1, Relaxed);
                    eprintln!("bridge slot {i}: {e:#}");
                    s.length.store(0, Relaxed);
                    s.second_length.store(0, Relaxed);
                    s.cache_hit.store(0, Relaxed);
                    s.status.store(-1, Relaxed);
                }
            }
            s.state.store(RESPONSE, Release);
        }
    }

    pub fn run() -> Result<()> {
        let mut args = std::env::args_os().skip(1);
        let first = args.next();
        ensure!(
            args.next().is_none(),
            "usage: victory-metal-daemon [--check | SHARED_FILE]"
        );
        let device = MetalDevice::system_default()?;
        let gpu = device.name();
        if first.as_deref() == Some(std::ffi::OsStr::new("--check")) {
            let converter = MetalIrConverter::load_default()?;
            println!(
                "GPU={gpu} unified={} converter={} protocol={BRIDGE_VERSION}",
                device.has_unified_memory(),
                converter.path().display()
            );
            return Ok(());
        }
        let file_path = first
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/tmp/victory-metal-bridge.shm"));
        let cache = std::env::var_os("VICTORY_METAL_CACHE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| Path::new("/tmp").join("victory-metal-cache"));
        let dir = shader_cache::use_shader_cache(cache)
            .context("could not initialize native shader cache")?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&file_path)
            .with_context(|| format!("creating new session {}", file_path.display()))?;
        let map = Mapping::create(&file)?;
        println!(
            "ready arch={} gpu={gpu} slots={SLOTS} cache={}",
            std::env::consts::ARCH,
            dir.display()
        );
        let stop = AtomicBool::new(false);
        let counts = Counts::default();
        std::thread::scope(|scope| {
            for i in 0..SLOTS {
                let (m, s, c, name) = (&map, &stop, &counts, &gpu);
                scope.spawn(move || worker(m, i, s, c, name));
            }
            // Unlinking the session is the launcher's orderly shutdown signal.
            while file_path.exists() {
                std::thread::sleep(Duration::from_millis(100));
            }
            stop.store(true, Relaxed);
        });
        println!(
            "completed cache_hits={} cache_misses={} shader_refusals={} transport_errors={}",
            counts.hits.load(Relaxed),
            counts.misses.load(Relaxed),
            counts.refusals.load(Relaxed),
            counts.errors.load(Relaxed)
        );
        Ok(())
    }
}

#[cfg(target_os = "macos")]
fn main() -> anyhow::Result<()> {
    server::run()
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("victory-metal-daemon requires macOS");
}
