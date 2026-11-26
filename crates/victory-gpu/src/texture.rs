//! Texture contents the title writes itself (decision 0007, "Texture
//! memory"): streamed pages, layout-library copies, movie frames, glyph
//! atlases. Such a texture lives in console memory in its tiled layout;
//! before a submission samples its host copy, the memory is detiled into a
//! staging buffer when its content changed, and the staging buffer is
//! copied into the host texture.
//!
//! "Changed" means a page of the memory changed since the last detiling
//! ([`crate::watch`], which follows writes through every mapping of the
//! same physical pages) in the runtime's memory; elsewhere, the whole memory
//! compared with the bytes last detiled at every submission that samples it.
//!
//! shortcut: a change detiles the whole texture, not the pages written.
//! Textures read through descriptor tables are brought up to date, and for
//! shaders that index the heap themselves every texture with a resource
//! object the heap views (`crate::bindless`); textures the title placed in
//! memory itself are not made for the heap (see `descriptor::host`). Plane
//! 0 is uploaded (every mip and array slice), depth and stencil planes are
//! not.

mod heap_reads;

use std::sync::{Arc, Mutex};

use windows::Win32::Graphics::Direct3D12::*;

use crate::host::{HostList, host};
use crate::object::This;
use windows::core::Interface;

macro_rules! once {
    ($($arg:tt)*) => {{
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| tracing::warn!($($arg)*));
    }};
}

/// A texture's upload state: the staging buffer laid out as the host
/// texture's subresources, and what was last detiled.
pub(crate) struct Staging {
    buffer: ID3D12Resource,
    /// Its bytes.
    size: u64,
    data: *mut u8,
    footprints: Vec<D3D12_PLACED_SUBRESOURCE_FOOTPRINT>,
    rows: Vec<u32>,
    layout: victory_formats::texture::Layout,
    /// Mips per array slice.
    mips: u32,
    first: u32,
    prefix: usize,
    memory: *const u8,
    /// The memory's pages changed since detiled (none: not the runtime's
    /// memory, compared instead).
    tracked: Option<crate::watch::Tracked>,
    last: Mutex<Last>,
}

/// What a texture's staging buffer holds: the memory last detiled (memory
/// not tracked, compared to tell a change), or room to read tracked memory
/// into.
#[derive(Default)]
struct Last {
    read: Vec<u8>,
}

// SAFETY: the staging mapping is written only at submission, under
// `last`'s lock; the title memory is only read.
unsafe impl Send for Staging {}
// SAFETY: as above.
unsafe impl Sync for Staging {}

/// A host copy or resolve writes `resource`: its host texture's contents
/// are the host's, not title memory's, which stays as it was.
pub(crate) fn host_written(resource: This) {
    if let Some(copy) = crate::resource::host_copy(resource) {
        copy.host_written.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

fn is_host_written(resource: This) -> bool {
    crate::resource::host_copy(resource)
        .is_some_and(|c| c.host_written.load(std::sync::atomic::Ordering::Relaxed))
}

/// Whether `resource`'s host copy takes its contents from title memory:
/// a texture the host GPU does not render to or write.
pub(crate) fn title_written(texture: &ID3D12Resource) -> bool {
    const TARGET_DEPTH_UNORDERED: i32 = 0x7;
    // SAFETY: a live resource.
    unsafe { texture.GetDesc() }.Flags.0 & TARGET_DEPTH_UNORDERED == 0
}

/// Copies [`alias_copy`] recorded since the last frame-rate line.
pub(crate) static ALIAS_COPIES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Records, once per list, the copy of `from`'s host texture into `to`'s:
/// two title textures at one memory that cannot share a host texture
/// (`resource::alias_source`: block-compressed against plain formats of
/// the same bytes, or a cast the host refused), `to` (a placed view's)
/// reading what the GPU wrote through `from`. Through a buffer, each
/// subresource with its own format, as many as have the same bytes per
/// row and rows in both; `to` no longer takes its contents from title
/// memory.
pub(crate) fn alias_copy(list: &HostList, from: This, to: This) {
    const COPY_DEST: u32 = 0x400;
    const COPY_SOURCE: u32 = 0x800;
    let (Some(a), Some(b), Some(host)) = (
        crate::resource::host_resource(from),
        crate::resource::host_resource(to),
        host(),
    ) else {
        return;
    };
    // An odd key: textures' own first uses are keyed by the host texture.
    if !list.first_use(to as usize | 1) {
        return;
    }
    // SAFETY: live resources.
    let (da, db) = unsafe { (a.GetDesc(), b.GetDesc()) };
    let count = |d: &D3D12_RESOURCE_DESC| {
        if d.Dimension == D3D12_RESOURCE_DIMENSION_TEXTURE3D {
            u32::from(d.MipLevels)
        } else {
            u32::from(d.MipLevels) * u32::from(d.DepthOrArraySize).max(1)
        }
    };
    let footprints = |d: &D3D12_RESOURCE_DESC, n: u32| {
        let mut f = vec![D3D12_PLACED_SUBRESOURCE_FOOTPRINT::default(); n as usize];
        let mut rows = vec![0u32; n as usize];
        let mut total = 0;
        // SAFETY: outputs sized for `n` subresources.
        unsafe {
            host.device
                .GetCopyableFootprints(d, 0, n, 0, Some(f.as_mut_ptr()), Some(rows.as_mut_ptr()), None, Some(&mut total));
        }
        (f, rows, total)
    };
    let (fa, ra, total) = footprints(&da, count(&da));
    let (fb, rb, _) = footprints(&db, count(&db));
    // Subresources in order while both have the same bytes there (mips of
    // a block-compressed texture past its last whole block do not).
    let n = (0..fa.len().min(fb.len()))
        .take_while(|&i| {
            let (x, y) = (fa[i], fb[i]);
            x.Offset == y.Offset && x.Footprint.RowPitch == y.Footprint.RowPitch && ra[i] == rb[i] && x.Footprint.Depth == y.Footprint.Depth
        })
        .count() as u32;
    if n == 0 {
        once!(?da, ?db, "an aliased texture not copied: its layouts differ on the host");
        return;
    }
    let Some(buffer) = host.default_buffer(total) else { return };
    list.keep(&buffer);
    crate::texture::host_written(to);
    crate::list::ensure_state(list, from, COPY_SOURCE);
    crate::list::ensure_state(list, to, COPY_DEST);
    let place = |r: &ID3D12Resource, f: D3D12_PLACED_SUBRESOURCE_FOOTPRINT| D3D12_TEXTURE_COPY_LOCATION {
        // SAFETY: a borrowed pointer the copy does not release.
        pResource: unsafe { std::mem::transmute_copy(r) },
        Type: D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT,
        Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 { PlacedFootprint: f },
    };
    let sub = |r: &ID3D12Resource, i: u32| D3D12_TEXTURE_COPY_LOCATION {
        // SAFETY: as above.
        pResource: unsafe { std::mem::transmute_copy(r) },
        Type: D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX,
        Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 { SubresourceIndex: i },
    };
    let l = &list.list;
    // SAFETY: an open list; live resources in the states named.
    unsafe {
        for i in 0..n {
            l.CopyTextureRegion(&place(&buffer, fa[i as usize]), 0, 0, 0, &sub(a, i), None);
        }
        l.ResourceBarrier(&[crate::host::transition(
            &buffer,
            D3D12_RESOURCE_STATE_COPY_DEST,
            D3D12_RESOURCE_STATE_COPY_SOURCE,
        )]);
        for i in 0..n {
            l.CopyTextureRegion(&sub(b, i), 0, 0, 0, &place(&buffer, fb[i as usize]), None);
        }
    }
    ALIAS_COPIES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    static LOGGED: Mutex<Vec<(usize, usize)>> = Mutex::new(Vec::new());
    let mut logged = LOGGED.lock().unwrap_or_else(|e| e.into_inner());
    if logged.len() < 64 && !logged.contains(&(from as usize, to as usize)) {
        logged.push((from as usize, to as usize));
        tracing::info!(from = ?from, to = ?to, a = da.Format.0, b = db.Format.0, w = da.Width, h = da.Height, subresources = n, "aliased texture copied");
    }
}

/// Records, once per list, the copy of `resource`'s staging buffer into
/// its host copy, and the detiling that refreshes the staging buffer at
/// submission if the memory changed. Leaves the texture in the copy
/// destination state.
pub(crate) fn upload(list: &HostList, resource: This, texture: &ID3D12Resource) {
    // Copies and bound tables read the texture whole.
    if let Some(copy) = crate::resource::host_copy(resource) {
        note_reads(&copy, 0);
    }
    // shortcut: a texture the host copied into keeps the host's contents
    // even if the title later writes its memory too.
    // Once per list for each host texture, however many title textures of
    // its memory the list samples.
    if is_host_written(resource) || !list.first_use(texture.as_raw() as usize) {
        return;
    }
    let Some(staging) = staging(resource, texture) else {
        return;
    };
    // The staging buffer lives as long as `staging`, which the deferred
    // refresh below holds until the list is reset.
    record(list, resource, texture, &staging);
    // Once per submission however many of its lists sample the texture.
    list.defer_texture(staging);
}

/// One distinct host texture sampled through a submission's bindless heap.
pub(crate) struct HeapUpload {
    resource: usize,
    texture: ID3D12Resource,
    staging: Arc<Staging>,
    changed: std::sync::atomic::AtomicBool,
}

impl HeapUpload {
    pub(crate) fn new(resource: This, host: &crate::resource::HostTexture) -> Option<Self> {
        if host.host_written.load(std::sync::atomic::Ordering::Relaxed) {
            return None;
        }
        Some(Self {
            resource: resource as usize,
            texture: host.texture.clone(),
            staging: staging(resource, &host.texture)?,
            changed: false.into(),
        })
    }

    /// Check and detile independent staging buffers concurrently. Only the
    /// submitting thread records copies and barriers, in original heap order.
    pub(crate) fn run(list: &HostList, jobs: &[Self]) -> usize {
        use std::sync::atomic::Ordering::Relaxed;
        if jobs.is_empty() {
            return 0;
        }
        let started = std::time::Instant::now();
        let check = |index: usize, snapshot: Option<&[u8]>| {
            let job = &jobs[index];
            let (size, changed) = match snapshot {
                Some(bytes) => job.staging.refresh_snapshot(bytes),
                None => job.staging.refresh_now(),
            };
            job.changed.store(changed, Relaxed);
            size
        };
        let bytes = if crate::watch::fault_tracking() {
            // Page protection changes and reads must stay ordered.
            (0..jobs.len()).map(|i| check(i, None)).sum()
        } else {
            let mut ranges = Vec::with_capacity(jobs.len());
            let mut total = 0usize;
            for (job, upload) in jobs.iter().enumerate() {
                let start = upload.staging.memory as usize;
                let size = upload.staging.prefix;
                // An invalid title range cannot produce a complete snapshot.
                if size != 0 && let Some(end) = start.checked_add(size) {
                    ranges.push(heap_reads::TextureRange { job, start, end });
                    total = total.saturating_add(size);
                }
            }
            let groups = heap_reads::groups(&mut ranges);
            let workers = texture_workers().min((total / (8 << 20)).max(1));
            crate::host::upload_jobs::run(&groups, workers, |g| heap_reads::check(g, &ranges, check))
        };
        let preparing = &crate::host::PREPARING;
        preparing[2].fetch_add(started.elapsed().as_micros() as u64, Relaxed);
        preparing[3].fetch_add(bytes as u64, Relaxed);
        let mut recorded = 0;
        for job in jobs.iter().filter(|job| job.changed.load(Relaxed)) {
            let r = job.resource as This;
            record(list, r, &job.texture, &job.staging);
            list.keep(&job.staging.buffer);
            const SHADER_RESOURCE: u32 = 0x40 | 0x80;
            crate::list::ensure_state(list, r, SHADER_RESOURCE);
            recorded += 1;
        }
        recorded
    }
}

fn texture_workers() -> usize {
    static WORKERS: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *WORKERS.get_or_init(|| {
        let available = std::thread::available_parallelism().map_or(1, usize::from);
        std::env::var("VICTORY_TEXTURE_WORKERS").ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(4).clamp(1, available.min(8))
    })
}

/// Records the copy of every subresource of `staging` into `texture`,
/// moving it to the copy destination state first.
fn record(list: &HostList, resource: This, texture: &ID3D12Resource, staging: &Staging) {
    const COPY_DEST: u32 = 0x400;
    crate::list::ensure_state(list, resource, COPY_DEST);
    for (i, footprint) in staging.footprints.iter().enumerate() {
        if (i as u32) % staging.mips < staging.first {
            continue;
        }
        #[cfg(feature = "diagnostics")]
        {
            let f = footprint.Footprint;
            // SAFETY: live resources.
            let (gpu, d) = unsafe { (staging.buffer.GetGPUVirtualAddress(), texture.GetDesc()) };
            let len = u64::from(f.RowPitch) * u64::from(staging.rows[i]) * u64::from(f.Depth);
            list.note_read(
                || {
                    format!(
                        "texture upload {resource:?} ({}x{}x{} f{} mips {}) sub {i} of {} from staging {gpu:#x}+{:#x}: offset {:#x} {}x{}x{} pitch {}",
                        d.Width,
                        d.Height,
                        d.DepthOrArraySize,
                        d.Format.0,
                        d.MipLevels,
                        staging.footprints.len(),
                        staging.size,
                        footprint.Offset,
                        f.Width,
                        f.Height,
                        f.Depth,
                        f.RowPitch
                    )
                },
                (gpu + footprint.Offset, len),
            );
        }
        let to = D3D12_TEXTURE_COPY_LOCATION {
            // SAFETY: a borrowed pointer the location never releases.
            pResource: unsafe { std::mem::transmute_copy(texture) },
            Type: D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX,
            Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
                SubresourceIndex: i as u32,
            },
        };
        let from = D3D12_TEXTURE_COPY_LOCATION {
            // SAFETY: as above.
            pResource: unsafe { std::mem::transmute_copy(&staging.buffer) },
            Type: D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT,
            Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
                PlacedFootprint: *footprint,
            },
        };
        // SAFETY: an open list; both locations name live resources.
        unsafe { list.list.CopyTextureRegion(&to, 0, 0, 0, &from, None) };
    }
}

impl Staging {
    /// Detiles the texture memory into the staging buffer if it changed
    /// since the last time (and is mapped at all): whether it did.
    pub(crate) fn refresh(&self) -> bool {
        use std::sync::atomic::Ordering::Relaxed;
        let started = std::time::Instant::now();
        let (read, changed) = self.refresh_now();
        let preparing = &crate::host::PREPARING;
        preparing[2].fetch_add(started.elapsed().as_micros() as u64, Relaxed);
        preparing[3].fetch_add(read as u64, Relaxed);
        changed
    }

    /// Returns the bytes of title memory read, and whether it detiled.
    fn refresh_now(&self) -> (usize, bool) {
        // Only the levels read: past them may lie another texture.
        let size = self.prefix;
        let start = self.memory as usize;
        let mut last = self.last.lock().unwrap_or_else(|e| e.into_inner());
        let last = &mut *last;
        if let Some(t) = &self.tracked {
            if t.take(start, start + size).is_empty() {
                return (0, false);
            }
            // Read through the copy, which stands unmapped pages: memory of
            // big pools is too slow to ask page by page. The rest of the
            // layout stays zero (unread levels).
            last.read.resize(self.layout.size as usize, 0);
            crate::host::copy_mapped(last.read.as_mut_ptr(), self.memory, size, false);
            self.detile(&last.read);
            #[cfg(feature = "diagnostics")]
            crate::copystat::add(crate::copystat::C::TexWatched, size as u64);
            return (size, true);
        }
        let Some(changed) = read_changed(&mut last.read, self.memory, size) else {
            if self.layout.slices == 1 && self.layout.desc.block == 4
                && self.layout.desc.dimension == victory_formats::texture::Dimension::Tex2d
                && self.first + 1 < self.mips
            {
                let available = readable_prefix(self.memory, size);
                if let Some((first, bytes)) = self.resident_levels(available) {
                    if let Some(changed) = read_changed(&mut last.read, self.memory, bytes) {
                        if changed {
                            self.detile_prefix(&last.read, first);
                            #[cfg(feature = "diagnostics")]
                            if crate::switch::on("VICTORY_MATERIAL_INPUTS") {
                                static RECOVERED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
                                let n = RECOVERED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                                if n < 12 || n.is_power_of_two() {
                                    tracing::info!(?self.memory, first, bytes, size, n, "uploaded resident levels after whole-texture read failed");
                                }
                            }
                        }
                        return (bytes, changed);
                    }
                }
            }
            if crate::switch::on("VICTORY_MATERIAL_INPUTS") {
                static COUNT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
                let n = COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if n < 10 || n.is_power_of_two() {
                    let readable = (1..self.mips).find(|&k| committed(self.memory, readable_bytes(&self.layout, k) as usize));
                    tracing::info!(?self.memory, size, ?self.layout.desc, self.first, ?readable, n, "material upload skipped unreadable layout");
                }
            }
            #[cfg(feature = "diagnostics")]
            tracing::debug!(
                memory = ?self.memory,
                size,
                desc = ?self.layout.desc,
                "a sampled texture's memory is not committed"
            );
            return (0, false);
        };
        self.finish_untracked(&last.read, changed)
    }

    /// The heap reader already checked every page of this texture. Only
    /// changed images need a private copy for the next submission.
    fn refresh_snapshot(&self, tiled: &[u8]) -> (usize, bool) {
        let mut last = self.last.lock().unwrap_or_else(|e| e.into_inner());
        let changed = save_changed(&mut last.read, tiled);
        self.finish_untracked(&last.read, changed)
    }

    fn finish_untracked(&self, tiled: &[u8], changed: bool) -> (usize, bool) {
        let size = tiled.len();
        #[cfg(feature = "diagnostics")]
        crate::copystat::add(crate::copystat::C::TexHashed, size as u64);
        if !changed {
            return (size, false);
        }
        self.detile_prefix(tiled, self.first);
        #[cfg(feature = "diagnostics")]
        crate::copystat::add(crate::copystat::C::TexChanged, size as u64);
        (size, true)
    }

    fn resident_levels(&self, available: usize) -> Option<(u32, usize)> {
        (self.first + 1..self.mips).find_map(|k| {
            let bytes = readable_bytes(&self.layout, k) as usize;
            (bytes > 0 && bytes <= available).then_some((k, bytes))
        })
    }

    fn detile_prefix(&self, tiled: &[u8], first: u32) {
        if tiled.len() == self.layout.size as usize {
            self.detile_from(tiled, first);
        } else {
            // Layout::detile validates the whole allocation. Only resident
            // subresources are written; previously uploaded finer mips survive.
            let mut full = vec![0; self.layout.size as usize];
            full[..tiled.len()].copy_from_slice(tiled);
            self.detile_from(&full, first);
        }
    }

    /// Detiles `tiled` (the texture's memory) into the staging buffer.
    fn detile(&self, tiled: &[u8]) {
        self.detile_from(tiled, self.first);
    }

    fn detile_from(&self, tiled: &[u8], first: u32) {
        #[cfg(feature = "diagnostics")]
        if tiled.iter().all(|&b| b == 0) {
            tracing::debug!(
                memory = ?self.memory,
                desc = ?self.layout.desc,
                "a sampled texture's memory is all zero (not streamed yet, or a failed BCPack read)"
            );
        }
        for (i, (footprint, &rows)) in self.footprints.iter().zip(&self.rows).enumerate() {
            let (mip, slice) = (i as u32 % self.mips, i as u32 / self.mips);
            if mip < first {
                continue;
            }
            let f = footprint.Footprint;
            let pitch = f.RowPitch as usize;
            let len = subresource_len(footprint.Offset, pitch, rows, f.Depth, self.size);
            // SAFETY: `subresource_len` clamps `len` to `self.size - footprint.Offset`,
            // keeping the slice strictly inside the mapped staging buffer even when
            // GetCopyableFootprints does not pad the final row of the last subresource.
            let linear = unsafe {
                std::slice::from_raw_parts_mut(self.data.add(footprint.Offset as usize), len)
            };
            if let Err(e) =
                self.layout
                    .detile(tiled, mip, slice, linear, pitch, pitch * rows as usize)
            {
                tracing::warn!(?e, mip, slice, "texture not detiled");
                return;
            }
        }
    }
}

/// Exact equality for untracked texture memory. Comparing whole cache lines
/// lets LLVM vectorize the common equal case under Wine/Rosetta; small
/// textures and the final partial line use the regular slice comparison.
fn same_texels(previous: &[u8], current: &[u8]) -> bool {
    if previous.len() != current.len() || current.len() < 512 {
        return previous == current;
    }
    let (previous_lines, previous_tail) = previous.as_chunks::<64>();
    let (current_lines, current_tail) = current.as_chunks::<64>();
    for (a, b) in previous_lines.iter().zip(current_lines) {
        let (a, _) = a.as_chunks::<16>();
        let (b, _) = b.as_chunks::<16>();
        let differences = a.iter().zip(b).fold(0, |bits, (x, y)| {
            bits | (u128::from_ne_bytes(*x) ^ u128::from_ne_bytes(*y))
        });
        if differences != 0 {
            return false;
        }
    }
    previous_tail == current_tail
}

/// Only changed images acquire storage, sized for their own texels.
fn save_changed(last: &mut Vec<u8>, bytes: &[u8]) -> bool {
    if same_texels(last, bytes) {
        return false;
    }
    last.clear();
    last.extend_from_slice(bytes);
    true
}

/// Take a checked snapshot of the title mapping. A failed read never
/// replaces the last complete image. Worker capacity stays with the worker.
fn read_changed(last: &mut Vec<u8>, memory: *const u8, size: usize) -> Option<bool> {
    use std::cell::RefCell;
    thread_local! { static SCRATCH: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) }; }
    (memory as usize).checked_add(size)?;
    SCRATCH.with_borrow_mut(|scratch| {
        if size > heap_reads::LIMIT {
            return read_large(last, memory, size, scratch);
        }
        if scratch.len() < size {
            scratch.resize(size, 0);
        }
        let bytes = &mut scratch[..size];
        crate::host::read_into(memory, bytes).then(|| save_changed(last, bytes))
    })
}

/// Stable large images only use a bounded comparison buffer. Allocate a new
/// complete image on a real difference, after checking the full title range.
/// The following kernel reads still check each piece if its mapping changes.
fn read_large(last: &mut Vec<u8>, memory: *const u8, size: usize, scratch: &mut Vec<u8>) -> Option<bool> {
    const CHUNK: usize = 1 << 20;
    if scratch.len() < CHUNK {
        scratch.resize(CHUNK, 0);
    }
    let mut replacement: Option<Vec<u8>> = None;
    for start in (0..size).step_by(CHUNK) {
        let len = (size - start).min(CHUNK);
        let bytes = &mut scratch[..len];
        if !crate::host::read_into(memory.wrapping_add(start), bytes) {
            return None;
        }
        if replacement.is_none() && (last.len() != size || !same_texels(&last[start..start+len], bytes)) {
            if !committed(memory, size) {
                return None;
            }
            let mut image = Vec::new();
            image.try_reserve_exact(size).ok()?;
            // Earlier chunks matched their last complete versions.
            image.extend_from_slice(&last[..start]);
            replacement = Some(image);
        }
        if let Some(image) = replacement.as_mut() {
            image.extend_from_slice(bytes);
        }
    }
    if let Some(image) = replacement {
        *last = image;
        Some(true)
    } else {
        Some(false)
    }
}

/// Bytes available in the staging buffer for a subresource at `offset`.
/// D3D12's `GetCopyableFootprints` omits trailing row-pitch padding on the
/// final row of the final subresource, so `pitch * rows * depth` can exceed
/// `total - offset`.
fn subresource_len(offset: u64, pitch: usize, rows: u32, depth: u32, total: u64) -> usize {
    let padded = pitch
        .saturating_mul(rows as usize)
        .saturating_mul(depth as usize);
    let remaining = usize::try_from(total.saturating_sub(offset)).unwrap_or(usize::MAX);
    padded.min(remaining)
}

/// Whether `[at, at + len)` is committed, readable memory (streamed textures
/// may be unmapped until their pages arrive).
pub(crate) fn committed(at: *const u8, len: usize) -> bool {
    len == 0 || readable_prefix(at, len) == len
}

fn readable_prefix(at: *const u8, len: usize) -> usize {
    use windows_sys::Win32::System::Memory::{
        MEM_COMMIT, MEMORY_BASIC_INFORMATION, PAGE_GUARD, PAGE_NOACCESS, VirtualQuery,
    };
    let mut p = at as usize;
    let Some(end) = p.checked_add(len) else {
        return 0;
    };
    while p < end {
        // SAFETY: plain data filled in by the query.
        let mut info: MEMORY_BASIC_INFORMATION = unsafe { std::mem::zeroed() };
        // SAFETY: a valid out pointer of the right size.
        let n = unsafe { VirtualQuery(p as *const _, &mut info, size_of_val(&info)) };
        let unreadable = info.Protect == 0
            || info.Protect & (PAGE_NOACCESS | PAGE_GUARD) != 0;
        if n == 0 || info.State != MEM_COMMIT || unreadable {
            break;
        }
        let next = (info.BaseAddress as usize).saturating_add(info.RegionSize);
        if next <= p {
            break;
        }
        p = next.min(end);
    }
    p - at as usize
}

/// The first mip level a shader view (its words as the title writes them)
/// can read: its most detailed level, or a later one where its minimum
/// level-of-detail clamp is past it. 0 for a view of the resource as a
/// whole and for buffers.
pub(crate) fn first_level(view: Option<&[u32; 10]>) -> u32 {
    let Some(w) = view else {
        return 0;
    };
    // `D3D12_SHADER_RESOURCE_VIEW_DESC`: format, dimension, mapping, a pad,
    // then the dimension's fields, the clamp last.
    let clamp = match w[1] {
        2 | 8 | 9 => w[6], // 1D, 3D, cube
        4 => w[7],         // 2D
        3 | 10 => w[8],    // 1D array, cube array
        5 => w[9],         // 2D array
        _ => return 0,
    };
    let clamp = f32::from_bits(clamp);
    let clamp = if clamp.is_finite() && clamp > 0.0 {
        clamp as u32
    } else {
        0
    };
    w[4].max(clamp)
}

/// A shader view of title texture `resource` reads its levels from `first`
/// on.
pub(crate) fn note_view(resource: This, first: u32) {
    if let Some(copy) = crate::resource::host_copy(resource) {
        note_reads(&copy, first);
    }
}

/// Something reads `copy` from level `first` on: a staging made from a
/// later level is made again.
fn note_reads(copy: &crate::resource::HostTexture, first: u32) {
    use std::sync::atomic::Ordering::Relaxed;
    if copy.read_from.load(Relaxed) <= first || copy.read_from.fetch_min(first, Relaxed) <= first {
        return;
    }
    let mut held = copy.staging.lock().unwrap_or_else(|e| e.into_inner());
    if held.as_ref().is_some_and(|s| s.first > first) {
        *held = None;
    }
}

/// The first level to upload of a texture of `mips` levels read from
/// `read_from` on: only single 2D textures start past level 0 (an array's
/// slices each hold the whole chain, so its unread levels are not one
/// range).
fn first_uploaded(layout: &victory_formats::texture::Layout, read_from: u32, mips: u32) -> u32 {
    use victory_formats::texture::Dimension;
    if read_from == u32::MAX || layout.slices != 1 || layout.desc.dimension != Dimension::Tex2d {
        return 0;
    }
    read_from.min(mips.saturating_sub(1))
}

/// The bytes at the start of a single texture's memory that hold its levels
/// from `first` on: the levels are stored smallest first (the mip tail,
/// then each larger level), so they end where level `first - 1` begins,
/// or where the tail ends when `first` lies in the tail.
fn readable_bytes(layout: &victory_formats::texture::Layout, first: u32) -> u64 {
    if first == 0 || layout.slices != 1 {
        return layout.size;
    }
    let k = first.min(layout.first_tail_mip) as usize;
    match k.checked_sub(1).and_then(|k| layout.mips.get(k)) {
        Some(next) if next.offset > 0 => next.offset,
        _ => layout.size,
    }
}

/// The staging state of a title texture's host texture, made on first use
/// (one for all title textures of its memory).
fn staging(resource: This, texture: &ID3D12Resource) -> Option<Arc<Staging>> {
    let copy = crate::resource::host_copy(resource)?;
    let mut held = copy.staging.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(s) = held.as_ref() {
        return Some(s.clone());
    }
    let (memory, desc) = crate::resource::texture_memory(resource)?;
    let Some(layout) = crate::resource::texture_layout(&desc).map(|t| t.planes[0].layout.clone())
    else {
        once!(
            ?desc,
            "texture contents without a console layout not uploaded"
        );
        return None;
    };
    if layout.desc.swizzle.xor() {
        once!(?desc, "texture contents in an XOR mode not uploaded");
        return None;
    }
    let host = host()?;
    // SAFETY: a live resource.
    let d = unsafe { texture.GetDesc() };
    let count = if d.Dimension == D3D12_RESOURCE_DIMENSION_TEXTURE3D {
        u32::from(d.MipLevels)
    } else {
        u32::from(d.MipLevels) * u32::from(d.DepthOrArraySize)
    };
    let mut footprints = vec![D3D12_PLACED_SUBRESOURCE_FOOTPRINT::default(); count as usize];
    let mut rows = vec![0u32; count as usize];
    let mut total = 0;
    // SAFETY: outputs for `count` subresources.
    unsafe {
        host.device.GetCopyableFootprints(
            &d,
            0,
            count,
            0,
            Some(footprints.as_mut_ptr()),
            Some(rows.as_mut_ptr()),
            None,
            Some(&mut total),
        );
    }
    let (buffer, data) = host.upload_buffer(total)?;
    crate::host::name(&buffer, || {
        format!(
            "staging of {resource:?} {}x{}x{} f{}",
            d.Width, d.Height, d.DepthOrArraySize, d.Format.0
        )
    });
    tracing::debug!(?desc, total, "texture contents uploaded from title memory");
    let mips = u32::from(d.MipLevels);
    let first = first_uploaded(
        &layout,
        copy.read_from.load(std::sync::atomic::Ordering::Relaxed),
        mips,
    );
    // Watched by 64 KiB pages: a write in the last page the levels read
    // share with what follows them still uploads them again (only them).
    let prefix = readable_bytes(&layout, first) as usize;
    if first > 0 {
        if crate::switch::on("VICTORY_MATERIAL_INPUTS") && committed(memory, prefix)
            && !committed(memory, layout.size as usize)
        {
            static SAVED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            let n = SAVED.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if n < 12 || n.is_power_of_two() {
                tracing::info!(?memory, format=d.Format.0, first, prefix, whole=layout.size, n, "material texture has readable mip levels in an incomplete mapping");
            }
        }
        once!(
            ?desc,
            first,
            read = prefix,
            whole = layout.size,
            "textures only viewed from a smaller level are uploaded from it on"
        );
    }
    #[cfg(feature = "diagnostics")]
    tracing::debug!(
        memory = ?memory,
        mips,
        first,
        read = prefix,
        whole = layout.size,
        neighbours = crate::resource::placed_textures_in(memory as u64 + prefix as u64, memory as u64 + layout.size),
        "texture staged"
    );
    let s = Arc::new(Staging {
        buffer,
        size: total,
        data,
        footprints,
        rows,
        layout,
        mips,
        first,
        prefix,
        memory,
        tracked: crate::watch::track(memory as usize, prefix, true),
        last: Mutex::default(),
    });
    *held = Some(s.clone());
    Some(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout(w: u32, h: u32, mips: u32) -> victory_formats::texture::Layout {
        use victory_formats::texture::{Desc, Dimension, Layout, Swizzle};
        Layout::new(&Desc {
            dimension: Dimension::Tex2d,
            width: w,
            height: h,
            depth: 1,
            mips,
            element_bytes: 16,
            block: 4,
            samples: 1,
            swizzle: Swizzle::SW_64KB_S_X,
        })
        .unwrap()
    }

    /// A texture's levels are stored smallest first: the levels from any
    /// level on take the start of its memory, smaller as the level rises,
    /// and level 0 on is the whole of it.
    #[test]
    fn the_readable_levels_are_a_prefix_of_the_memory() {
        let l = layout(2048, 2048, 12);
        assert!(
            l.first_tail_mip > 0 && l.first_tail_mip < 12,
            "{}",
            l.first_tail_mip
        );
        assert_eq!(readable_bytes(&l, 0), l.size);
        let prefixes: Vec<u64> = (0..12).map(|first| readable_bytes(&l, first)).collect();
        for pair in prefixes.windows(2) {
            assert!(
                pair[1] <= pair[0],
                "smaller from a later level: {prefixes:?}"
            );
        }
        // Each level on lies inside its prefix, and none before it does.
        for first in 1..l.first_tail_mip {
            let end = readable_bytes(&l, first);
            let m = &l.mips[first as usize];
            assert!(m.offset < end, "level {first} in its prefix");
            assert!(
                l.mips[first as usize - 1].offset >= end,
                "level {} outside",
                first - 1
            );
        }
        // From a level in the tail on: the tail block alone, the same for all.
        let tail = readable_bytes(&l, l.first_tail_mip);
        assert_eq!(readable_bytes(&l, 11), tail);
        assert!(tail < l.size / 64, "a tail of {tail} bytes");
        // Level 1 on is about a quarter of the memory.
        let one = readable_bytes(&l, 1);
        assert!(one * 3 < l.size && one * 5 > l.size, "{one} of {}", l.size);
    }

    /// Views say their first level by their most detailed level or their
    /// level-of-detail clamp, per dimension; arrays and unknown views keep
    /// every level.
    #[test]
    fn views_name_their_first_level() {
        let view = |dimension: u32, most_detailed: u32, clamp_at: usize, clamp: f32| {
            let mut w = [0u32; 10];
            (w[1], w[4]) = (dimension, most_detailed);
            w[clamp_at] = clamp.to_bits();
            w
        };
        assert_eq!(first_level(None), 0);
        assert_eq!(
            first_level(Some(&view(4, 3, 7, 0.0))),
            3,
            "2D, most detailed level"
        );
        assert_eq!(
            first_level(Some(&view(4, 0, 7, 2.5))),
            2,
            "2D, clamp 2.5 reads level 2"
        );
        assert_eq!(
            first_level(Some(&view(4, 4, 7, 2.0))),
            4,
            "the later of the two"
        );
        assert_eq!(first_level(Some(&view(9, 0, 6, 5.0))), 5, "cube");
        assert_eq!(first_level(Some(&view(5, 0, 9, 1.0))), 1, "2D array");
        assert_eq!(first_level(Some(&view(4, 0, 7, f32::NAN))), 0);
        assert_eq!(first_level(Some(&view(1, 9, 7, 0.0))), 0, "a buffer");
        // Arrays are uploaded whole whatever their views read.
        let mut array = layout(256, 256, 9);
        assert_eq!(first_uploaded(&array, 3, 9), 3);
        array.slices = 2;
        assert_eq!(first_uploaded(&array, 3, 9), 0);
        assert_eq!(
            first_uploaded(&layout(256, 256, 9), u32::MAX, 9),
            0,
            "no view yet"
        );
        assert_eq!(
            first_uploaded(&layout(256, 256, 9), 20, 9),
            8,
            "at most the last level"
        );
    }

    /// A texture placed in title memory and viewed only from level 3 on
    /// (the title keeps a streamed texture's small levels and places the
    /// next texture where its larger ones would be): it is uploaded and
    /// watched from level 3 on only, so a write past those levels (the next
    /// texture's, past the 64 KiB watch page they share) does not upload it
    /// again, a write inside them does, and a view of every level later
    /// makes it whole again.
    #[test]
    fn a_texture_viewed_from_a_later_level_is_staged_from_it_only() {
        let p = crate::watch::tests::pool(8);
        let words = crate::resource::texture_words(256, 256, 28, 9);
        let Some(r) = crate::resource::texture_at(p as u64, &words) else {
            crate::watch::tests::free(p);
            return; // no host GPU here
        };
        note_view(r, 3);
        let copy = crate::resource::host_copy(r).expect("a host copy");
        let s = staging(r, &copy.texture).expect("its staging");
        assert_eq!(s.first, 3);
        assert_eq!(s.prefix as u64, readable_bytes(&s.layout, 3));
        assert!(
            s.prefix * 16 < s.layout.size as usize,
            "{} of {}",
            s.prefix,
            s.layout.size
        );
        assert!(s.refresh(), "uploaded the first time");
        assert!(!s.refresh(), "nothing written since");
        let title = |at: usize| (p + at) as *mut u8;
        let past = s.prefix.next_multiple_of(crate::watch::PAGE) + 16;
        assert!(past < s.layout.size as usize);
        // SAFETY: a byte of our pool, inside the layout.
        unsafe { title(past).write_volatile(0x5a) };
        assert!(
            !s.refresh(),
            "a write past the levels read is not this texture's"
        );
        // SAFETY: as above.
        unsafe { title(8).write_volatile(0xa5) };
        assert!(s.refresh(), "a write inside them is");
        // A view of every level: made again, whole.
        note_view(r, 0);
        assert!(
            copy.staging.lock().unwrap().is_none(),
            "the staging from level 3 is let go"
        );
        let whole = staging(r, &copy.texture).expect("its staging again");
        assert_eq!((whole.first, whole.prefix), (0, whole.layout.size as usize));
        crate::watch::tests::free(p);
    }

    #[test]
    fn streamed_bc_mips_survive_an_absent_top_level_and_reload() {
        use windows_sys::Win32::System::Memory::{
            MEM_COMMIT, MEM_DECOMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_READWRITE, VirtualAlloc, VirtualFree,
        };
        const REGION: usize = 4 << 20;
        // SAFETY: this fixture exclusively owns the mapping. Only the checked
        // texture reader touches it between decommit and recommit.
        unsafe {
            let p = VirtualAlloc(std::ptr::null(), REGION, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE).cast::<u8>();
            assert!(!p.is_null());
            let words = crate::resource::texture_words(1024, 1024, 98, 10);
            let r = crate::resource::texture_at(p as u64, &words).expect("GPU for the upload test");
            let copy = crate::resource::host_copy(r).unwrap();
            let s = staging(r, &copy.texture).unwrap();
            let full = s.layout.size as usize;
            let prefix = readable_bytes(&s.layout, 1) as usize;
            assert!(s.tracked.is_none() && prefix < full && full <= REGION);
            assert!(prefix.is_multiple_of(4096));
            let byte = |mip:usize| s.data.add(s.footprints[mip].Offset as usize).read_volatile();
            p.write_bytes(13,full);
            assert_eq!(s.refresh_now(), (full,true));
            assert_eq!((byte(0),byte(1)), (13,13));
            let tail = p.add(prefix).cast();
            assert_ne!(VirtualFree(tail, full-prefix, MEM_DECOMMIT), 0);
            p.write_bytes(24,prefix);
            assert_eq!(s.refresh_now(), (prefix,true));
            assert_eq!((byte(0),byte(1)), (13,24));
            assert_eq!(s.refresh_now(), (prefix,false));
            assert_eq!(VirtualAlloc(tail, full-prefix, MEM_COMMIT, PAGE_READWRITE), tail);
            p.add(prefix).write_bytes(37,full-prefix);
            assert_eq!(s.refresh_now(), (full,true));
            assert_eq!((byte(0),byte(1)), (37,24));
            assert_ne!(VirtualFree(p.cast(),0,MEM_RELEASE),0);
        }
    }

    #[test]
    fn subresource_len_stays_inside_staging_total() {
        // A 4x4 RGBA8 texture has RowPitch=256, rows=4, last_row=16 -> total=784.
        // Padded size (1024) must be clamped to 784 so from_raw_parts_mut stays in bounds.
        assert_eq!(subresource_len(0, 256, 4, 1, 784), 784);
        assert_eq!(subresource_len(512, 256, 4, 1, 784), 272);
        assert_eq!(subresource_len(0, 256, 4, 1, 2048), 1024);
    }

    #[test]
    fn parallel_texture_checks_see_every_submission_in_heap_order() {
        use std::sync::atomic::{AtomicBool, Ordering::Relaxed};
        let mut jobs: Vec<_> = (0..41).map(|i| {
            (vec![i as u8; 513 + i], Mutex::new(Vec::new()), AtomicBool::new(false))
        }).collect();
        for round in 0..3 {
            if round == 2 {
                jobs[4].0[512] ^= 1;
                jobs[40].0[0] ^= 1;
            }
            let total = crate::host::upload_jobs::run(&jobs, 4, |(memory, previous, changed)| {
                let update = read_changed(&mut previous.lock().unwrap(), memory.as_ptr(), memory.len());
                changed.store(update.unwrap(), Relaxed);
                memory.len()
            });
            assert_eq!(total, jobs.iter().map(|j| j.0.len()).sum::<usize>());
            let changed: Vec<_> = jobs.iter().enumerate().filter(|(_, j)| j.2.load(Relaxed))
                .map(|(i, _)| i).collect();
            let expected = match round { 0 => (0..41).collect(), 1 => Vec::new(), _ => vec![4, 40] };
            assert_eq!(changed, expected);
        }
    }

    #[test]
    fn texture_comparison_checks_every_byte_including_the_tail() {
        let previous = vec![0xa5; 1090];
        let mut current = previous.clone();
        for len in [0, 1, 511, 512, 513, 1088, 1089] {
            let reference = &previous[1..1 + len];
            assert!(same_texels(reference, &current[..len]));
            assert!(!same_texels(reference, &current[..len + 1]));
            for offset in 0..len {
                current[offset] ^= 1;
                assert!(!same_texels(reference, &current[..len]), "{len} at {offset}");
                current[offset] ^= 1;
            }
        }
    }

    #[test]
    fn untracked_memory_reads_as_changed_once_per_change() {
        let mut last = Vec::new();
        let mut memory = vec![1u8; 4096];
        assert!(read_changed(&mut last, memory.as_ptr(), memory.len()).unwrap(), "never detiled");
        assert!(!read_changed(&mut last, memory.as_ptr(), memory.len()).unwrap());
        memory[4095] = 2;
        assert!(read_changed(&mut last, memory.as_ptr(), memory.len()).unwrap());
        assert_eq!(last, memory);
        assert!(!read_changed(&mut last, memory.as_ptr(), memory.len()).unwrap());
    }

    #[test]
    fn unreadable_texture_pages_preserve_the_last_complete_snapshot() {
        use windows_sys::Win32::System::Memory::{
            MEM_COMMIT, MEM_DECOMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_READWRITE,
            VirtualAlloc, VirtualFree,
        };
        const PAGE: usize = 4096;
        let mut last = Vec::new();
        // SAFETY: both pages belong to this test. Only the checked reader
        // touches the second page while it is decommitted.
        unsafe {
            let base = VirtualAlloc(std::ptr::null(), 2 * PAGE, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE).cast::<u8>();
            assert!(!base.is_null());
            base.write_bytes(7, 2 * PAGE);
            assert_eq!(read_changed(&mut last, base, 2 * PAGE), Some(true));
            let tail = base.add(PAGE);
            assert_ne!(VirtualFree(tail.cast(), PAGE, MEM_DECOMMIT), 0);
            base.write_bytes(9, PAGE);
            assert_eq!(read_changed(&mut last, base, 2 * PAGE), None);
            assert_eq!(last, vec![7; 2 * PAGE]);
            assert_eq!(VirtualAlloc(tail.cast(), PAGE, MEM_COMMIT, PAGE_READWRITE), tail.cast());
            tail.write_bytes(2, PAGE);
            assert_eq!(read_changed(&mut last, base, 2 * PAGE), Some(true));
            assert!(last[..PAGE].iter().all(|&b| b == 9));
            assert!(last[PAGE..].iter().all(|&b| b == 2));
            assert_eq!(read_changed(&mut last, base, 2 * PAGE), Some(false));
            assert_ne!(VirtualFree(base.cast(), 0, MEM_RELEASE), 0);
        }
    }

    #[test]
    fn small_texture_snapshots_never_inherit_a_large_workers_capacity() {
        let large = vec![5u8; 3 << 20];
        let small = [7u8; 128];
        let mut first = Vec::new();
        let mut second = Vec::new();
        assert_eq!(read_changed(&mut first, large.as_ptr(), large.len()), Some(true));
        assert_eq!(read_changed(&mut first, large.as_ptr(), large.len()), Some(false));
        assert_eq!(read_changed(&mut second, small.as_ptr(), small.len()), Some(true));
        assert!(second.capacity() < 1024);
        assert_eq!(second, small);
    }

    #[test]
    fn small_snapshots_do_not_retain_a_large_workers_buffer() {
        let big = vec![5; 8 << 20];
        let small = [3; 4096];
        let mut previous = big.clone();
        assert_eq!(read_changed(&mut previous, big.as_ptr(), big.len()), Some(false));
        let mut snapshot = Vec::new();
        assert_eq!(read_changed(&mut snapshot, small.as_ptr(), small.len()), Some(true));
        assert_eq!(snapshot, small);
        assert!(snapshot.capacity() < 2 * small.len());
    }

    #[test]
    fn large_snapshot_checks_the_tail_and_preserves_previous_bytes_on_failure() {
        use windows_sys::Win32::System::Memory::{
            MEM_COMMIT, MEM_DECOMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_READWRITE, VirtualAlloc, VirtualFree,
        };
        const SIZE: usize = (5 << 20) + 4096;
        let mut last = Vec::new();
        let mut scratch = Vec::new();
        // SAFETY: the test owns the pages, and only writes to committed ones.
        unsafe {
            let at = VirtualAlloc(std::ptr::null(), SIZE, MEM_COMMIT|MEM_RESERVE, PAGE_READWRITE).cast::<u8>();
            assert!(!at.is_null());
            at.write_bytes(1, SIZE);
            assert_eq!(read_large(&mut last, at, SIZE, &mut scratch), Some(true));
            assert_eq!(read_large(&mut last, at, SIZE, &mut scratch), Some(false));
            assert!(scratch.capacity() <= 1 << 20);
            at.add(SIZE-1).write(92);
            assert_eq!(read_large(&mut last, at, SIZE, &mut scratch), Some(true));
            assert_eq!(last[SIZE-1], 92);
            at.write(17);
            assert_ne!(VirtualFree(at.add(SIZE-4096).cast(),4096,MEM_DECOMMIT),0);
            assert_eq!(read_large(&mut last, at, SIZE, &mut scratch), None);
            assert_eq!((last[0],last[SIZE-1]), (1,92));
            assert_ne!(VirtualFree(at.cast(),0,MEM_RELEASE),0);
        }
    }

    #[test]
    fn committed_rejects_overflow_and_noaccess_pages() {
        use windows_sys::Win32::System::Memory::{
            MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, PAGE_NOACCESS, PAGE_READWRITE, VirtualAlloc,
            VirtualFree,
        };
        assert!(!committed(usize::MAX as *const u8, 16));
        // SAFETY: reserving and committing one private page for the test, freed below.
        let noaccess =
            unsafe { VirtualAlloc(std::ptr::null(), 4096, MEM_RESERVE | MEM_COMMIT, PAGE_NOACCESS) };
        assert!(!noaccess.is_null());
        assert!(!committed(noaccess.cast::<u8>(), 4096));
        // SAFETY: freeing the allocation above.
        unsafe { VirtualFree(noaccess, 0, MEM_RELEASE) };

        // Negative control: a readable committed page still passes.
        // SAFETY: reserving and committing one private page for the test, freed below.
        let rw =
            unsafe { VirtualAlloc(std::ptr::null(), 4096, MEM_RESERVE | MEM_COMMIT, PAGE_READWRITE) };
        assert!(!rw.is_null());
        assert!(committed(rw.cast::<u8>(), 4096));
        // SAFETY: freeing the allocation above.
        unsafe { VirtualFree(rw, 0, MEM_RELEASE) };
    }
}
