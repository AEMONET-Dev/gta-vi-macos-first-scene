//! Views shaders reach through the resource heap itself (decision 0007,
//! "Heap-indexed views"). Grand Theft Auto VI is largely bindless: its
//! shaders load descriptors at indices they take from root constants or
//! memory (`HostShader::heap`), so no draw names the views it reads. Every
//! view the title writes into a shader-visible heap gets its host view when
//! it is written (`descriptor::host`), and each heap keeps here what its
//! slots view that title memory feeds: ranges of host buffer copies, and
//! textures whose contents the title writes. A submission whose lists run
//! such shaders first brings all of them up to date (what changed since,
//! `crate::watch`).

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};
use std::sync::{Arc, Mutex};

use crate::descriptor::View;
use crate::host::HostList;
use crate::object::This;
use crate::shadow::Shadow;

/// What a title heap slot views that title memory feeds.
#[derive(Clone)]
enum Seen {
    /// `len` bytes at `address` of a buffer's host copy; unordered access.
    Buffer(Arc<Shadow>, u64, u64, bool),
    /// A title texture (resource object) whose contents the title writes.
    Texture(usize),
    /// A view left without a host view (textures the title placed itself),
    /// by kind (diagnostics: `KINDS`).
    Left(#[cfg_attr(not(feature = "diagnostics"), allow(dead_code))] usize),
}

/// Kinds of views left without a host view, for the census.
#[cfg(feature = "diagnostics")]
const KINDS: [&str; 4] = ["buffer", "unordered buffer", "placed texture", "other"];

/// A heap's slots that view title memory, and the distinct buffer ranges
/// and textures among them (with how many slots view each).
#[derive(Default)]
pub(crate) struct Viewed {
    inner: Mutex<Inner>,
}

/// One run of nearby buffer views, grouped by host copy and access kind.
type BufferRun = (Arc<Shadow>, u64, u64, bool);

#[derive(Default)]
struct Inner {
    slots: HashMap<u32, Seen>,
    buffers: HashMap<(usize, bool), BufferViews>,
    textures: BTreeMap<usize, u32>,
    /// Shared by all lists until a distinct buffer view is added or removed.
    runs: Option<Arc<[BufferRun]>>,
}

struct BufferViews {
    shadow: Arc<Shadow>,
    /// Half-open address interval and number of slots that refer to it.
    live: BTreeMap<(u64, u64), u32>,
    runs: Option<Vec<(u64, u64)>>,
}

/// Group nearby views for one checked read. A missing tile between two
/// views is inexpensive, but a large unbound region in a heap is not.
fn buffer_runs(live: &BTreeMap<(u64, u64), u32>) -> Vec<(u64, u64)> {
    const MAX_GAP: u64 = 64 << 10;
    let mut runs: Vec<(u64, u64)> = Vec::new();
    for &(from, to) in live.keys() {
        if let Some((_, end)) = runs.last_mut()
            && from <= end.saturating_add(MAX_GAP)
        {
            *end = (*end).max(to);
        } else {
            runs.push((from, to));
        }
    }
    runs
}

impl Inner {
    fn count(&mut self, seen: &Seen, add: bool) {
        let step = |n: &mut u32| if add { *n += 1 } else { *n = n.saturating_sub(1) };
        match seen {
            Seen::Buffer(shadow, address, len, uav) => {
                let key = (Arc::as_ptr(shadow) as usize, *uav);
                let group = self.buffers.entry(key).or_insert_with(|| BufferViews {
                    shadow: shadow.clone(),
                    live: BTreeMap::new(),
                    runs: None,
                });
                let range = (*address, address + len);
                let count = group.live.entry(range).or_default();
                let was = *count;
                step(count);
                let now = *count;
                if (was == 0) != (now == 0) {
                    group.runs = None;
                    self.runs = None;
                }
                if now == 0 {
                    group.live.remove(&range);
                    if group.live.is_empty() {
                        self.buffers.remove(&key);
                    }
                }
            }
            Seen::Texture(r) => {
                let n = self.textures.entry(*r).or_insert(0);
                step(n);
                if *n == 0 {
                    self.textures.remove(r);
                }
            }
            Seen::Left(_) => {}
        }
    }
}

impl Viewed {
    /// Title slot `index` now holds `view` (none: no view), made on the
    /// host (`made`) or left without a host view.
    pub(crate) fn set(&self, index: u32, view: Option<&View>, made: bool) {
        let seen = view.and_then(|v| if made { seen(v) } else { Some(Seen::Left(left_kind(v))) });
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(old) = inner.slots.remove(&index) {
            inner.count(&old, false);
        }
        if let Some(seen) = seen {
            inner.count(&seen, true);
            inner.slots.insert(index, seen);
        }
    }

    /// The buffer ranges views reach, merged per host copy and kind
    /// (shader resource, unordered access): copy, start, bytes.
    fn ranges(&self) -> Arc<[BufferRun]> {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(runs) = &inner.runs {
            return runs.clone();
        }
        let mut all = Vec::new();
        for (&(_, uav), group) in &mut inner.buffers {
            let ranges = group.runs.get_or_insert_with(|| buffer_runs(&group.live));
            all.extend(ranges.iter().map(|&(a, b)| (group.shadow.clone(), a, b - a, uav)));
        }
        let runs: Arc<[BufferRun]> = all.into();
        inner.runs = Some(runs.clone());
        runs
    }

}

/// What a made view reads that title memory feeds.
fn seen(view: &View) -> Option<Seen> {
    let placed = crate::descriptor::host::placed_texture(view);
    let view = placed.as_ref().unwrap_or(view);
    if let View::Shader(resource, _) = view
        && let Some(texture) = crate::resource::host_resource(*resource as This)
    {
        return crate::texture::title_written(texture).then_some(Seen::Texture(*resource));
    }
    // The copy the host view reads.
    let (shadow, address, len) = match *view {
        View::Constant(address, size) => {
            let len = u64::from(size);
            ((len > 0).then(|| crate::shadow::find(address, len)).flatten()?, address, len)
        }
        _ => crate::descriptor::host::view_copy(view)?,
    };
    let uav = matches!(view, View::Unordered(..) | View::Placed(true, ..));
    if len == 0 {
        return None;
    }
    let len = len.min(shadow.left(address));
    Some(Seen::Buffer(shadow, address, len, uav))
}

fn left_kind(view: &View) -> usize {
    match view {
        View::Placed(_, resource, _) if resource[0] != 1 => 2,
        View::Unordered(..) | View::Placed(true, ..) => 1,
        View::Shader(..) | View::Placed(false, ..) | View::Constant(..) => 0,
        _ => 3,
    }
}

/// At submission, before anything else of it: the textures the heaps of its
/// heap-indexing lists view are brought up to date, those whose title
/// memory changed copied into their host textures by a list of ours, which
/// the submission runs first (and leaves them readable by shaders).
///
/// shortcut: a title copy into texture memory recorded in the same
/// submission shows from the next one on.
pub(crate) fn uploads(heaps: &[Arc<Viewed>]) -> Option<Uploads> {
    static LIST: Mutex<Holder> = Mutex::new(Holder(None));
    let mut guard = LIST.lock().unwrap_or_else(|e| e.into_inner());
    match &guard.0 {
        // Closed by the last submission, which has run.
        Some(list) => list.reset(),
        // Made open.
        None => guard.0 = Some(crate::host::host()?.new_list().ok()?),
    }
    let list = guard.0.as_ref()?;
    let mut recorded = false;
    let mut heaps_checked = std::collections::HashSet::new();
    let mut checked = HashMap::new();
    let mut jobs = Vec::new();
    for viewed in heaps {
        if !heaps_checked.insert(Arc::as_ptr(viewed)) {
            continue;
        }
        // Resource destruction waits in forget(). Keep the heap locked until
        // all checks have joined and their GPU copies have been recorded.
        let inner = viewed.inner.lock().unwrap_or_else(|e| e.into_inner());
        for &r in inner.textures.keys() {
            let Some(host) = crate::resource::host_copy(r as This) else {
                continue;
            };
            // Several title objects can share the very same host texture
            // and staging buffer. Keep the identity alive across heaps.
            if let std::collections::hash_map::Entry::Vacant(entry) = checked.entry(Arc::as_ptr(&host))
                && let Some(job) = crate::texture::HeapUpload::new(r as This, &host)
            {
                entry.insert(host);
                jobs.push(job);
            }
        }
        let count = crate::texture::HeapUpload::run(list, &jobs);
        recorded |= count != 0;
        #[cfg(feature = "diagnostics")]
        UPLOADED.fetch_add(count as u64, Relaxed);
        jobs.clear();
    }
    list.close();
    // A list recording nothing is not run (its reset next time is fine).
    recorded.then_some(Uploads(guard))
}

/// The upload list, used under its lock only.
pub(crate) struct Holder(Option<HostList>);
// SAFETY: D3D12 objects are free-threaded; the list is used by one
// submission at a time, under the lock.
unsafe impl Send for Holder {}

/// The upload list of a submission, ours until it has run.
pub(crate) struct Uploads(std::sync::MutexGuard<'static, Holder>);

impl Uploads {
    pub(crate) fn list(&self) -> &HostList {
        self.0.0.as_ref().expect("made before it is handed out")
    }
}

/// Per submission: brings the buffer ranges a heap's views reach up to date
/// (in the submission's batch of refreshes): the pages the title changed
/// since their copies took them.
///
/// shortcut: what shaders write through the heap's unordered access views
/// stays in the host copies (later draws read it there) and does not go
/// back to title memory, so the CPU does not see it, and a page the title
/// also writes is copied over from title memory, losing the GPU's bytes
/// there. Copying back the whole unordered ranges after every submission
/// (about 1 GB of them in the world) cost seconds per frame; the upgrade
/// is shaders marking the pages they write (as `crate::memory` does).
pub(crate) fn refresh(viewed: &Viewed, mut add: impl FnMut(&Arc<Shadow>, usize, usize)) {
    for (shadow, address, len, _) in viewed.ranges().iter() {
        add(shadow, *address as usize, *len as usize);
    }
}

/// Draws and dispatches run with heap-indexing shaders, those of them
/// writing through the heap, textures uploaded for heaps (diagnostics).
static HEAP_DRAWS: [AtomicU64; 2] = [AtomicU64::new(0), AtomicU64::new(0)];
#[cfg(feature = "diagnostics")]
static UPLOADED: AtomicU64 = AtomicU64::new(0);

/// A draw or dispatch runs a shader that indexes the heap (and writes
/// through it).
pub(crate) fn note_draw(writes: bool) {
    HEAP_DRAWS[0].fetch_add(1, Relaxed);
    if writes {
        HEAP_DRAWS[1].fetch_add(1, Relaxed);
    }
}

/// Every view heap's registry.
static HEAPS: Mutex<Vec<std::sync::Weak<Viewed>>> = Mutex::new(Vec::new());

pub(crate) fn register(viewed: &Arc<Viewed>) {
    let mut heaps = HEAPS.lock().unwrap_or_else(|e| e.into_inner());
    heaps.retain(|h| h.strong_count() > 0);
    heaps.push(Arc::downgrade(viewed));
}

/// The title released `resource`: slots viewing it as a texture view
/// nothing to upload any more (its memory may be gone).
pub(crate) fn forget(resource: usize) {
    let heaps = HEAPS.lock().unwrap_or_else(|e| e.into_inner()).clone();
    for viewed in heaps.iter().filter_map(std::sync::Weak::upgrade) {
        let mut inner = viewed.inner.lock().unwrap_or_else(|e| e.into_inner());
        if inner.textures.remove(&resource).is_some() {
            inner.slots.retain(|_, seen| !matches!(seen, Seen::Texture(r) if *r == resource));
        }
    }
}

/// Logs, per frame over `frames`, the heap-indexed draws, and what the
/// view heaps' slots view (diagnostics).
#[cfg(feature = "diagnostics")]
pub(crate) fn log(frames: f64) {
    let [draws, writes] = [0, 1].map(|i| HEAP_DRAWS[i].swap(0, Relaxed) as f64 / frames);
    let uploaded = UPLOADED.swap(0, Relaxed) as f64 / frames;
    let heaps = HEAPS.lock().unwrap_or_else(|e| e.into_inner()).clone();
    for viewed in heaps.iter().filter_map(std::sync::Weak::upgrade) {
        let inner = viewed.inner.lock().unwrap_or_else(|e| e.into_inner());
        let mut left = [0u32; KINDS.len()];
        let (mut buffers, mut textures) = (0u32, 0u32);
        for seen in inner.slots.values() {
            match seen {
                Seen::Buffer(..) => buffers += 1,
                Seen::Texture(_) => textures += 1,
                Seen::Left(k) => left[*k] += 1,
            }
        }
        let ranges: usize = inner.buffers.values().map(|group| group.live.len()).sum();
        let bytes: u64 = inner.buffers.values()
            .flat_map(|group| group.live.keys()).map(|&(a, b)| b - a).sum();
        let copies = inner.buffers.keys().map(|k| k.0).collect::<std::collections::HashSet<_>>().len();
        drop(inner);
        let hulls = viewed.ranges();
        let uav_hulls: u64 = hulls.iter().filter(|r| r.3).map(|r| r.2).sum();
        let all_hulls: u64 = hulls.iter().map(|r| r.2).sum();
        let inner = viewed.inner.lock().unwrap_or_else(|e| e.into_inner());
        let left: Vec<String> = KINDS.iter().zip(left).map(|(k, n)| format!("{k} {n}")).collect();
        let all = crate::shadow::all();
        let all_bytes: u64 = all.iter().map(|s| s.range().1).sum();
        tracing::info!(
            "heap views: {draws:.0} draws/frame index the heap ({writes:.0} writing through it), \
             {uploaded:.0} textures uploaded/frame; slots: {buffers} buffer views ({} ranges, \
             {:.1} MB, {copies} copies), {textures} title-written textures ({} distinct); \
             left null: {}; host copies of title memory: {} ({:.1} MB); hulls {:.1} MB, unordered {:.1} MB",
            ranges,
            bytes as f64 / 1e6,
            inner.textures.len(),
            left.join(", "),
            all.len(),
            all_bytes as f64 / 1e6,
            all_hulls as f64 / 1e6,
            uav_hulls as f64 / 1e6
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::object::{HRESULT, S_OK};
    use windows::Win32::Graphics::Direct3D12::*;
    use windows::core::s;

    /// Reads slots 0 (raw buffer) and 2 (texture) of the heap and writes
    /// slot 1 (raw unordered buffer): through a table over the heap's
    /// start, the same slots a shader indexing the heap reads.
    const SOURCE: &str = r#"
ByteAddressBuffer src : register(t0);
RWByteAddressBuffer dst : register(u0);
Texture2D<float4> tex : register(t1);
[numthreads(16, 1, 1)]
void main(uint3 id : SV_DispatchThreadID) {
    uint texel = (uint)round(tex.Load(int3(id.x % 4, 0, 0)).x * 255.0);
    dst.Store(id.x * 4, src.Load(id.x * 4) + texel);
}
"#;

    fn compile() -> (ID3D12RootSignature, ID3D12PipelineState) {
        use crate::compute::{Param, srv, uav};
        let params = &[Param::Table(&[srv(0, 1), uav(0, 1), srv(1, 1)])];
        crate::compute::pipeline(SOURCE, s!("heap"), 0, params).unwrap()
    }

    /// A title resource from a description in the console's layout (56
    /// bytes: dimension, alignment, width, height, depth and mips, format,
    /// samples, layout, flags).
    fn resource(dimension: u32, width: u64, height: u32, format: u32, layout: u32) -> This {
        let desc: [u32; 14] = [
            dimension, 0, 0, 0, width as u32, (width >> 32) as u32, height, 1 | 1 << 16, format, 1, 0,
            layout, 0, 0,
        ];
        let properties = [1u32, 0, 0, 0, 0];
        let mut out = std::ptr::null();
        let iid = crate::object::guid(
            0x696442be,
            0xa72e,
            0x4059,
            [0xbc, 0x79, 0x5b, 0x5c, 0x98, 0x04, 0x0f, 0xad],
        );
        let made: HRESULT = crate::resource::create_committed(
            std::ptr::null(),
            properties.as_ptr().cast(),
            0,
            desc.as_ptr().cast(),
            0,
            std::ptr::null(),
            &iid,
            &mut out,
        );
        assert_eq!(made, S_OK);
        out
    }

    /// A title resource at `address` (title memory of its own), as the
    /// title places buffers and textures in its heaps.
    fn resource_at(address: usize, dimension: u32, width: u64, height: u32, format: u32, layout: u32) -> This {
        let desc: [u32; 14] = [
            dimension, 0, 0, 0, width as u32, (width >> 32) as u32, height, 1 | 1 << 16, format, 1, 0,
            layout, 0, 0,
        ];
        let mut out = std::ptr::null();
        let iid = crate::object::guid(
            0x696442be,
            0xa72e,
            0x4059,
            [0xbc, 0x79, 0x5b, 0x5c, 0x98, 0x04, 0x0f, 0xad],
        );
        let made = crate::resource::create_at_address(
            std::ptr::null(),
            address as u64,
            desc.as_ptr().cast(),
            0,
            std::ptr::null(),
            &iid,
            &mut out,
        );
        assert_eq!(made, S_OK);
        out
    }

    const LINEAR: u32 = 0x100;

    /// Repeated and replaced slots keep exactly the live buffer runs.
    #[test]
    fn runs_follow_the_live_views() {
        let memory = vec![0u8; 0x40000];
        let at = memory.as_ptr() as u64;
        crate::shadow::register(at as usize, at, 0x40000);
        let Some(copy) = crate::shadow::find(at, 0x40000) else {
            crate::shadow::unregister(at as usize, at);
            return; // no host GPU here
        };
        let viewed = Viewed::default();
        let set = |slot: u32, range: Option<(u64, u64)>| {
            let mut inner = viewed.inner.lock().unwrap();
            if let Some(old) = inner.slots.remove(&slot) {
                inner.count(&old, false);
            }
            if let Some((from, len)) = range {
                let seen = Seen::Buffer(copy.clone(), at + from, len, false);
                inner.count(&seen, true);
                inner.slots.insert(slot, seen);
            }
        };
        let hull = || viewed.ranges().iter().map(|h| (h.1 - at, h.2)).collect::<Vec<_>>();
        set(0, Some((0x100, 0x100)));
        set(1, Some((0x20000, 0x200)));
        set(2, Some((0x100, 0x100)));
        assert_eq!(hull(), [(0x100, 0x100), (0x20000, 0x200)]);
        set(1, None);
        assert_eq!(hull(), [(0x100, 0x100)], "the far range gone");
        set(0, None);
        assert_eq!(hull(), [(0x100, 0x100)], "one slot still views it");
        set(2, None);
        assert!(hull().is_empty());
        crate::shadow::unregister(at as usize, at);
    }

    /// The title writes a buffer and a texture, and views of them and of an
    /// output buffer into its heap; a shader reaching them only through the
    /// heap (no table entry names them) reads what the title wrote at
    /// submission, every round (the title rewrites both each time, as Bink
    /// rewrites its planes every frame). The output buffer is not read back
    /// into title memory (`refresh`'s shortcut): it is read from its host
    /// copy.
    #[test]
    fn heap_views_are_current() {
        let (src, dst) = (resource(1, 256, 1, 0, 1), resource(1, 256, 1, 0, 1));
        let tex = resource(3, 4, 1, 28, LINEAR);
        let write = |base: u32, texel: u8| {
            let (src_bytes, _) = crate::resource::buffer_bytes(src).unwrap();
            let (tex_bytes, tex_desc) = crate::resource::texture_memory(tex).unwrap();
            let size = crate::resource::texture_layout(&tex_desc).unwrap().planes[0].layout.size as usize;
            // SAFETY: the resources' own memory, 256 bytes and the texture's.
            unsafe {
                let words = std::slice::from_raw_parts_mut(src_bytes.cast::<u32>(), 64);
                for (i, w) in words.iter_mut().enumerate() {
                    *w = base + i as u32;
                }
                std::ptr::write_bytes(tex_bytes.cast_mut(), texel, size);
            }
        };
        rounds(src, dst, tex, write);
    }

    /// The same with the buffer and the texture in one view of physical
    /// pages and the title writing them through another (the title fills its
    /// heaps through a pool view): every round must reach the shader.
    #[test]
    fn heap_views_follow_writes_through_another_mapping() {
        let (a, b) = crate::watch::tests::twin_views(2);
        let src = resource_at(a, 1, 256, 1, 0, 1);
        let tex = resource_at(a + PAGE_BYTES, 3, 4, 1, 28, LINEAR);
        let dst = resource(1, 256, 1, 0, 1);
        let (_, tex_desc) = crate::resource::texture_memory(tex).unwrap();
        let size = crate::resource::texture_layout(&tex_desc).unwrap().planes[0].layout.size as usize;
        let write = |base: u32, texel: u8| {
            // SAFETY: the second view of the buffer's and the texture's pages.
            unsafe {
                let words = std::slice::from_raw_parts_mut(b as *mut u32, 64);
                for (i, w) in words.iter_mut().enumerate() {
                    *w = base + i as u32;
                }
                std::ptr::write_bytes((b + PAGE_BYTES) as *mut u8, texel, size);
            }
        };
        rounds(src, dst, tex, write);
    }

    const PAGE_BYTES: usize = crate::watch::PAGE;

    /// Three rounds of: record a dispatch reading slots 0 and 2 and writing
    /// slot 1, let the title `write` (after recording, before submitting),
    /// submit, check the output's host copy.
    fn rounds(src: This, dst: This, tex: This, write: impl Fn(u32, u8)) {
        let Some(host) = crate::host::host() else {
            return; // no host GPU here
        };
        let null = std::ptr::null();
        // A shader-visible view heap of 8 slots, in memory of its own.
        let heap_desc: [u32; 8] = [0, 8, 1, 0, 0, 0, 0, 0];
        let mut heap = null;
        let iid = crate::object::guid(
            0x8efb471d,
            0x616c,
            0x4f49,
            [0x90, 0xf7, 0x12, 0x7b, 0xb7, 0x63, 0xfa, 0x51],
        );
        assert_eq!(crate::descriptor::create_heap(null, heap_desc.as_ptr().cast(), &iid, &mut heap), S_OK);
        let title_heap = crate::descriptor::heap(heap);
        let slot = |i: u64| title_heap.start + 32 * i;
        // Raw views of 64 dwords: format R32_TYPELESS, buffer dimension,
        // the default mapping, then first element, count, stride, flags.
        let srv: [u32; 10] = [39, 1, 0x1688, 0, 0, 0, 64, 0, 1, 0];
        let uav: [u32; 10] = [39, 1, 0, 0, 64, 0, 0, 0, 1, 0];
        crate::descriptor::create_srv(null, src, &srv, slot(0));
        crate::descriptor::create_uav(null, dst, null, &uav, slot(1));
        crate::descriptor::create_srv(null, tex, std::ptr::null(), slot(2));
        let heap_host = title_heap.host().expect("a host view heap");
        let (root, pso) = compile();
        let list = host.new_list().unwrap();
        let (dst_bytes, _) = crate::resource::buffer_bytes(dst).unwrap();
        let out = crate::shadow::find(dst_bytes as u64, 256).expect("the output's host copy");
        for (round, (base, texel)) in [(1000u32, 0x40u8), (5000, 0x10), (9000, 0x22)].into_iter().enumerate() {
            if round > 0 {
                list.reset();
            }
            // SAFETY: an open list, live objects of ours.
            unsafe {
                list.list.SetDescriptorHeaps(&[Some(heap_host.heap.clone())]);
                list.list.SetComputeRootSignature(&root);
                list.list.SetComputeRootDescriptorTable(0, heap_host.gpu(0));
                list.list.SetPipelineState(&pso);
                list.list.Dispatch(4, 1, 1);
            }
            list.defer_heap(&heap_host.viewed);
            list.close();
            // The title writes after recording, before submitting.
            write(base, texel);
            host.execute(&[&list]);
            let words = out.read_host(dst_bytes as u64, 64);
            for (i, &v) in words.iter().enumerate() {
                assert_eq!(v, base + i as u32 + u32::from(texel), "round {round}, dword {i}");
            }
        }
    }
}
