//! Optional ambient contribution for the Act 1 lighting pass. Sample the
//! title's sRGB base colour and its linear HDR lighting at the same pixel.
//! Store in the game's own HDR target before exposure and temporal processing.
use crate::{
    compute,
    host::{HostList, host, transition},
    object::This,
};
use std::sync::{Mutex, Once, OnceLock};
use windows::Win32::Graphics::{Direct3D12::*, Dxgi::Common::*};
use windows::core::s;

const AMBIENT: &str = r#"
Texture2D<float4> base : register(t0);
Texture2D<float4> light : register(t1);
RWTexture2D<float4> result : register(u0);
#ifdef MATERIAL_DEPTH
Texture2D<float> surfaceDepth : register(t2);
cbuffer Dimensions : register(b0) { uint width; uint height; };
float surfaceShade(int2 pixel, float z) {
    // Choose the closer neighbour on each axis so silhouettes do not bend
    // the reconstructed normal towards the background.
    int2 hi = int2(width, height) - 1;
    float left = surfaceDepth.Load(int3(max(pixel - int2(1,0),int2(0,0)),0));
    float right = surfaceDepth.Load(int3(min(pixel + int2(1,0),hi),0));
    float top = surfaceDepth.Load(int3(max(pixel - int2(0,1),int2(0,0)),0));
    float bottom = surfaceDepth.Load(int3(min(pixel + int2(0,1),hi),0));
    float dx = abs(right-z) < abs(z-left) ? right-z : z-left;
    float dy = abs(bottom-z) < abs(z-top) ? bottom-z : z-top;
    float2 slope = clamp(float2(dx,dy) * float(height) / (1.15 * z), -20.0, 20.0);
    float3 normal = normalize(float3(-slope.x, slope.y, 1.0));
    return saturate(dot(normal, float3(-0.48,0.64,0.6)));
}
#endif
[numthreads(8,8,1)]
void main(uint3 id : SV_DispatchThreadID) {
    int3 at = int3(id.xy, 0);
    float4 original = light.Load(at);
    float3 diffuse = base.Load(at).rgb;
#ifdef MATERIAL_RESOLVE
    // Preserve the material colour when the lighting buffer contains large
    // saturated values from a different surface. Use its intensity for
    // shading and admit only compact, bright, multi-channel light sources.
    float3 lit = clamp(original.rgb, 0.0, 64.0);
    float luma = dot(lit, float3(0.2126, 0.7152, 0.0722));
    float fill = 0.6 + 0.55 * (luma / (1.0 + luma));
    float white = min(lit.r, min(lit.g, lit.b));
    float3 highlights = min(lit, 12.0) * (0.12 * saturate((white - 1.0) * 0.5));
    // This compatibility pass combines diffuse colour with an HDR image which
    // can still hold the previous view. Its additive term is not a material
    // reflection. On opaque surfaces it paints camera-fixed light over albedo.
    // Retain the diffuse lighting response and leave uncovered sky lights alone.
#ifdef MATERIAL_DEPTH
    float z = surfaceDepth.Load(at);
    if (z > 0.000001) {
#ifdef MATERIAL_CLEAN_LIGHT
        highlights = float3(0.0, 0.0, 0.0);
#endif
        // Give untextured opaque geometry a neutral diffuse response.
        // Textured surfaces keep their painted colour and sky has no depth.
        float floorValue = 0.007 + 0.065 * surfaceShade(at.xy, z);
        diffuse = max(diffuse, floorValue * saturate(1.0 - 12.0 * max(diffuse.r, max(diffuse.g, diffuse.b))));
    }
#endif
    result[id.xy] = float4(diffuse * fill + highlights, original.a);
#else
    result[id.xy] = float4(max(original.rgb, diffuse * 1.25), original.a);
#endif
}"#;

// A frame may have more than one view. Match the exact HDR resource the
// upscaler reads and never carry a snapshot across a presentation.
struct SceneSample {
    frame: u64,
    scene: ID3D12Resource,
    pixels: ID3D12Resource,
}
static SAMPLES: Mutex<Vec<SceneSample>> = Mutex::new(Vec::new());

pub(crate) fn sample(scene: &ID3D12Resource) -> Option<ID3D12Resource> {
    let frame = crate::queue::presented();
    let samples = SAMPLES.lock().unwrap_or_else(|e| e.into_inner());
    let found = samples
        .iter()
        .rev()
        .find(|s| s.frame == frame && &s.scene == scene)?;
    static FIRST: Once = Once::new();
    FIRST
        .call_once(|| tracing::info!(frame, "spatial upscale uses the current lit material image"));
    Some(found.pixels.clone())
}

pub(crate) fn record(
    list: &HostList,
    base: This,
    scene: This,
    depth: Option<This>,
    groups: (u32, u32, u32),
) -> bool {
    let (Some(host), Some(base_tex), Some(scene_tex)) = (
        host(),
        crate::resource::host_resource(base),
        crate::resource::host_resource(scene),
    ) else {
        return false;
    };
    // SAFETY: both resources come from the bound dispatch descriptors.
    let (base_desc, hdr) = unsafe { (base_tex.GetDesc(), scene_tex.GetDesc()) };
    if base_desc.Format != DXGI_FORMAT_R8G8B8A8_TYPELESS
        || hdr.Format != DXGI_FORMAT_R16G16B16A16_FLOAT
        || base_desc.Width != hdr.Width
        || base_desc.Height != hdr.Height
        || groups.2 != 1
        || u64::from(groups.0) * 8 != hdr.Width
        || u64::from(groups.1) * 8 != u64::from(hdr.Height)
        || [base_desc, hdr].iter().any(|d| {
            d.Dimension != D3D12_RESOURCE_DIMENSION_TEXTURE2D
                || d.SampleDesc.Count != 1
                || d.DepthOrArraySize != 1
                || d.MipLevels != 1
        })
    {
        static SKIP: Once = Once::new();
        SKIP.call_once(|| tracing::info!(?groups, base = ?base_desc, scene = ?hdr, "material lighting skipped for this dispatch layout"));
        return false;
    }
    let use_depth = crate::switch::on("VICTORY_MATERIAL_SURFACES");
    let depth = depth.filter(|_| use_depth).and_then(|r| {
        let texture = crate::resource::host_resource(r)?;
        let desc = unsafe { texture.GetDesc() };
        (desc.Format == DXGI_FORMAT_R32G8X24_TYPELESS
            && desc.Width == hdr.Width
            && desc.Height == hdr.Height
            && desc.SampleDesc.Count == 1)
            .then_some((r, texture))
    });
    static PIPE: OnceLock<Option<(ID3D12RootSignature, ID3D12PipelineState)>> = OnceLock::new();
    let Some((root, pso)) = PIPE.get_or_init(|| {
        let source = if crate::switch::on("VICTORY_MATERIAL_EARLY_SCENE") {
            let surface = if use_depth {
                "#define MATERIAL_DEPTH 1\n"
            } else {
                ""
            };
            let clean = if crate::switch::on("VICTORY_MATERIAL_CLEAN_LIGHT") {
                "#define MATERIAL_CLEAN_LIGHT 1\n"
            } else {
                ""
            };
            format!("#define MATERIAL_RESOLVE 1\n{surface}{clean}{AMBIENT}")
        } else {
            AMBIENT.to_owned()
        };
        compute::pipeline(
            &source,
            s!("material ambient light"),
            0,
            &[
                compute::Param::Table(&[compute::srv(0, 3), compute::uav(0, 1)]),
                compute::Param::Constants {
                    register: 0,
                    count: 2,
                },
            ],
        )
        .map_err(|e| tracing::warn!(%e, "material lighting pipeline failed"))
        .ok()
    }) else {
        return false;
    };
    let mut scratch: Option<ID3D12Resource> = None;
    let heap_desc = D3D12_DESCRIPTOR_HEAP_DESC {
        Type: D3D12_DESCRIPTOR_HEAP_TYPE_CBV_SRV_UAV,
        NumDescriptors: 4,
        Flags: D3D12_DESCRIPTOR_HEAP_FLAG_SHADER_VISIBLE,
        ..Default::default()
    };
    let properties = D3D12_HEAP_PROPERTIES {
        Type: D3D12_HEAP_TYPE_DEFAULT,
        ..Default::default()
    };
    // SAFETY: the temporary output has the destination format and extent.
    let table: windows::core::Result<ID3D12DescriptorHeap> =
        unsafe { host.device.CreateDescriptorHeap(&heap_desc) };
    let Ok(table) = table else { return false };
    let made = unsafe {
        host.device.CreateCommittedResource(
            &properties,
            D3D12_HEAP_FLAG_NONE,
            &D3D12_RESOURCE_DESC {
                Flags: D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS,
                ..hdr
            },
            D3D12_RESOURCE_STATE_UNORDERED_ACCESS,
            None,
            &mut scratch,
        )
    };
    let Some(scratch) = made.ok().and(scratch) else {
        return false;
    };
    list.keep(&scratch);
    list.keep(&table);
    list.keep(base_tex);
    list.keep(scene_tex);
    let base_view = D3D12_SHADER_RESOURCE_VIEW_DESC {
        Format: DXGI_FORMAT_R8G8B8A8_UNORM_SRGB,
        ViewDimension: D3D12_SRV_DIMENSION_TEXTURE2D,
        Shader4ComponentMapping: D3D12_DEFAULT_SHADER_4_COMPONENT_MAPPING,
        Anonymous: D3D12_SHADER_RESOURCE_VIEW_DESC_0 {
            Texture2D: D3D12_TEX2D_SRV {
                MipLevels: 1,
                ..Default::default()
            },
        },
    };
    if let Some((r, texture)) = depth {
        crate::list::ensure_state(list, r, 0x40);
        list.keep(texture);
    }
    let next_frame = crate::queue::presented() + 1;
    if crate::switch::on("VICTORY_MATERIAL_INPUTS") && crate::capture::listed(next_frame) {
        for (label, resource, texture) in [("base", base, base_tex), ("light", scene, scene_tex)] {
            crate::list::ensure_state(list, resource, 0x800);
            crate::capture::material_input(list, texture, next_frame, label);
        }
    }
    crate::list::ensure_state(list, base, 0x40);
    crate::list::ensure_state(list, scene, 0x40);
    // SAFETY: the list retains all resources and the heap. Separate input
    // and output storage also works on hosts without typed HDR UAV loads.
    unsafe {
        let l = &list.list;
        let mut at = table.GetCPUDescriptorHandleForHeapStart();
        let stride = host.device.GetDescriptorHandleIncrementSize(heap_desc.Type) as usize;
        host.device
            .CreateShaderResourceView(base_tex, Some(&base_view), at);
        at.ptr += stride;
        host.device.CreateShaderResourceView(scene_tex, None, at);
        at.ptr += stride;
        let depth_view = D3D12_SHADER_RESOURCE_VIEW_DESC {
            Format: if depth.is_some() {
                DXGI_FORMAT_R32_FLOAT_X8X24_TYPELESS
            } else {
                DXGI_FORMAT_R32_FLOAT
            },
            ..base_view
        };
        host.device
            .CreateShaderResourceView(depth.map(|(_, t)| t), Some(&depth_view), at);
        at.ptr += stride;
        host.device
            .CreateUnorderedAccessView(&scratch, None::<&ID3D12Resource>, None, at);
        l.SetDescriptorHeaps(&[Some(table.clone())]);
        l.SetComputeRootSignature(root);
        l.SetPipelineState(pso);
        l.SetComputeRootDescriptorTable(0, table.GetGPUDescriptorHandleForHeapStart());
        let dimensions = [hdr.Width as u32, hdr.Height];
        l.SetComputeRoot32BitConstants(1, 2, dimensions.as_ptr().cast(), 0);
        l.Dispatch(groups.0, groups.1, 1);
        if depth.is_some() {
            static DEPTH_SEEN: Once = Once::new();
            DEPTH_SEEN
                .call_once(|| tracing::info!("material lighting uses the current scene depth"));
        }
        if crate::switch::on("VICTORY_MATERIAL_EARLY_SCENE") {
            l.ResourceBarrier(&[transition(
                &scratch,
                D3D12_RESOURCE_STATE_UNORDERED_ACCESS,
                D3D12_RESOURCE_STATE_NON_PIXEL_SHADER_RESOURCE,
            )]);
            let frame = crate::queue::presented();
            let mut samples = SAMPLES.lock().unwrap_or_else(|e| e.into_inner());
            samples.retain(|s| s.frame == frame && &s.scene != scene_tex);
            samples.push(SceneSample {
                frame,
                scene: scene_tex.clone(),
                pixels: scratch,
            });
        } else {
            l.ResourceBarrier(&[transition(
                &scratch,
                D3D12_RESOURCE_STATE_UNORDERED_ACCESS,
                D3D12_RESOURCE_STATE_COPY_SOURCE,
            )]);
            crate::list::ensure_state(list, scene, 0x400);
            l.CopyResource(scene_tex, &scratch);
        }
        crate::list::ensure_state(list, scene, 0x8);
    }
    static FIRST: Once = Once::new();
    FIRST.call_once(|| tracing::info!(?groups, "material lighting recorded in the title scene"));
    true
}
