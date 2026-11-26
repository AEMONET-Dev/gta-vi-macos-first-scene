//! Diagnostic spatial upscale into the game's packed HDR output. All later
//! tone mapping, compositing and presentation continue on the title's list.
use crate::{
    compute,
    host::{HostList, host, transition},
    object::This,
};
use std::sync::OnceLock;
use windows::Win32::Graphics::Direct3D12::*;
use windows::core::s;

const UPSCALE: &str = r#"
Texture2D<float4> scene : register(t0);
RWByteAddressBuffer pixels : register(u0);
cbuffer Sizes : register(b0) { uint sw; uint sh; uint dw; uint dh; uint pitch; };
[numthreads(8,8,1)]
void main(uint3 id : SV_DispatchThreadID) {
    if (id.x >= dw || id.y >= dh) return;
    float2 coord = (float2(id.xy) + 0.5) * float2(sw, sh) / float2(dw, dh) - 0.5;
    int2 low = int2(floor(coord));
    int2 bound = int2(sw, sh) - 1;
    float2 fracxy = frac(coord);
    float3 a = scene.Load(int3(clamp(low, int2(0,0), bound), 0)).xyz;
    float3 b = scene.Load(int3(clamp(low+int2(1,0), int2(0,0), bound), 0)).xyz;
    float3 c = scene.Load(int3(clamp(low+int2(0,1), int2(0,0), bound), 0)).xyz;
    float3 d = scene.Load(int3(clamp(low+int2(1,1), int2(0,0), bound), 0)).xyz;
    float3 rgb = clamp(lerp(lerp(a,b,fracxy.x), lerp(c,d,fracxy.x),fracxy.y), 0.0, 65408.0);
    float peak = max(rgb.x, max(rgb.y, rgb.z));
    uint e = (uint)clamp(floor(log2(max(peak, exp2(-16.0)))) + 16.0, 0.0, 31.0);
    float scale = exp2(24.0 - (float)e);
    if (floor(peak * scale + 0.5) >= 512.0 && e < 31) { e++; scale *= 0.5; }
    uint3 m = (uint3)min(floor(rgb * scale + 0.5), 511.0);
    pixels.Store(id.y * pitch + id.x * 4, m.x | (m.y << 9) | (m.z << 18) | (e << 27));
}"#;

pub(crate) fn record(list: &HostList, input: This, output: This) -> bool {
    let (Some(host), Some(src), Some(dst)) = (
        host(),
        crate::resource::host_resource(input),
        crate::resource::host_resource(output),
    ) else {
        return false;
    };
    let early = if crate::switch::on("VICTORY_MATERIAL_EARLY_SCENE") {
        let Some(sample) = crate::material_light::sample(src) else { return false; };
        Some(sample)
    } else { None };
    let src = early.as_ref().unwrap_or(src);
    // SAFETY: the command binds both live textures.
    let (a, b) = unsafe { (src.GetDesc(), dst.GetDesc()) };
    if a.Format.0 != 10
        || b.Format.0 != 67
        || a.Dimension != D3D12_RESOURCE_DIMENSION_TEXTURE2D
        || b.Dimension != D3D12_RESOURCE_DIMENSION_TEXTURE2D
        || a.DepthOrArraySize != 1
        || b.DepthOrArraySize != 1
        || a.SampleDesc.Count != 1
        || b.SampleDesc.Count != 1
    {
        return false;
    }
    static PIPE: OnceLock<Option<(ID3D12RootSignature, ID3D12PipelineState)>> = OnceLock::new();
    let Some((root, pso)) = PIPE.get_or_init(|| {
        compute::pipeline(
            UPSCALE,
            s!("spatial HDR upscale"),
            0,
            &[
                compute::Param::Table(&[compute::srv(0, 1)]),
                compute::Param::Uav { register: 0 },
                compute::Param::Constants {
                    register: 0,
                    count: 5,
                },
            ],
        )
        .map_err(|e| tracing::warn!(%e, "could not create spatial upscale"))
        .ok()
    }) else {
        return false;
    };
    let mut fp = D3D12_PLACED_SUBRESOURCE_FOOTPRINT::default();
    let mut bytes = 0;
    // SAFETY: room for the output's one subresource.
    unsafe {
        host.device
            .GetCopyableFootprints(&b, 0, 1, 0, Some(&mut fp), None, None, Some(&mut bytes))
    };
    let Some((packed, mapping)) = host.shared_buffer(bytes) else {
        return false;
    };
    let table: windows::core::Result<ID3D12DescriptorHeap> = unsafe {
        host.device
            .CreateDescriptorHeap(&D3D12_DESCRIPTOR_HEAP_DESC {
                Type: D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV,
                NumDescriptors: 1,
                Flags: D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE,
                ..Default::default()
            })
    };
    let Ok(table) = table else { return false };
    list.keep(&table);
    list.keep(&packed);
    list.keep(src);
    list.keep(dst);
    if early.is_none() {
        crate::list::ensure_state(list, input, 0x40);
    }
    crate::list::ensure_state(list, output, 0x400);
    let dims = [
        a.Width as u32,
        a.Height,
        b.Width as u32,
        b.Height,
        fp.Footprint.RowPitch,
    ];
    // SAFETY: root parameters match the program, the list holds all descriptors
    // and resources. The packed buffer matches the destination's exact footprint.
    unsafe {
        let l = &list.list;
        host.device
            .CreateShaderResourceView(src, None, table.GetCPUDescriptorHandleForHeapStart());
        l.SetDescriptorHeaps(&[Some(table.clone())]);
        l.SetComputeRootSignature(root);
        l.SetPipelineState(pso);
        l.SetComputeRootDescriptorTable(0, table.GetGPUDescriptorHandleForHeapStart());
        l.SetComputeRootUnorderedAccessView(1, packed.GetGPUVirtualAddress());
        l.SetComputeRoot32BitConstants(2, 5, dims.as_ptr().cast(), 0);
        l.Dispatch(dims[2].div_ceil(8), dims[3].div_ceil(8), 1);
        l.ResourceBarrier(&[transition(
            &packed,
            D3D12_RESOURCE_STATE_UNORDERED_ACCESS,
            D3D12_RESOURCE_STATE_COPY_SOURCE,
        )]);
        let from = D3D12_TEXTURE_COPY_LOCATION {
            pResource: std::mem::transmute_copy(&packed),
            Type: D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT,
            Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
                PlacedFootprint: fp,
            },
        };
        let to = D3D12_TEXTURE_COPY_LOCATION {
            pResource: std::mem::transmute_copy(dst),
            Type: D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX,
            Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
                SubresourceIndex: 0,
            },
        };
        l.CopyTextureRegion(&to, 0, 0, 0, &from, None);
    }
    crate::list::ensure_state(list, output, 0x8);
    if crate::capture::listed(crate::queue::presented() + 1) {
        let address = mapping as usize;
        list.defer_after(move || {
            let mut nonzero = 0;
            let mut min = u32::MAX;
            let mut max = 0;
            for y in (0..dims[3] as usize).step_by(8) {
                for x in (0..dims[2] as usize).step_by(8) {
                    // SAFETY: one packed word in the footprint. The private buffer
                    // remains mapped until the completed list is reset.
                    let word = unsafe {
                        ((address + y * dims[4] as usize + x * 4) as *const u32).read_unaligned()
                    };
                    nonzero += usize::from(word != 0);
                    min = min.min(word);
                    max = max.max(word);
                }
            }
            tracing::info!(nonzero, min, max, "spatial packed output samples");
        });
    }
    static FIRST: std::sync::Once = std::sync::Once::new();
    FIRST.call_once(|| tracing::info!(?dims, "spatial upscale recorded into the title HDR output"));
    true
}
