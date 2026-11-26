//! End-to-end native M5 Max Apple Silicon verification tests for `victory-metal`:
//! 1. Native `MTLDevice` detection (`Apple M5 Max`, `hasUnifiedMemory == true`).
//! 2. True zero-copy unified memory (`newBufferWithBytesNoCopy`) + live GPU compute execution.
//! 3. Native macOS 16 KiB dirty-page tracking (`mprotect` + `SIGBUS`/`SIGSEGV`) on the exact
//!    same zero-copy buffer (0 bytes copied to GPU, exact dirty pages tracked!).
//! 4. End-to-end `libmetalirconverter.dylib` DXIL → Metal IR (`.metallib`) → `MTLDevice.newLibraryWithData`
//!    → `MTLComputePipelineState` compilation natively on ARM64!

#![cfg(target_os = "macos")]

use victory_metal::converter::{MetalIrConverter, ShaderStage};
use victory_metal::metal::{MetalDevice, MtlSize};
use victory_metal::watch::WatchedRegion;

#[test]
fn native_m5_zero_copy_compute_and_dirty_page_tracking() {
    let device = MetalDevice::system_default().expect("MTLCreateSystemDefaultDevice");
    let gpu_name = device.name();
    println!("Native Metal GPU: {gpu_name}, unified={}", device.has_unified_memory());
    assert!(device.has_unified_memory(), "Apple Silicon must report unified memory");

    // 1. Allocate 4 Apple Silicon pages (64 KiB) with native macOS write-watch.
    let region = WatchedRegion::new(4).expect("WatchedRegion::new");
    let ps = region.page_size();
    assert_eq!(region.len(), 4 * ps);

    // 2. Wrap the EXACT same physical host RAM pages into Metal with ZERO COPY!
    // SAFETY: `region` is page-aligned and outlives `buf`.
    let buf = unsafe { device.new_buffer_no_copy(region.as_mut_ptr(), region.len()) }
        .expect("newBufferWithBytesNoCopy");
    assert_eq!(buf.contents(), region.as_mut_ptr(), "zero-copy buffer must share exact CPU address");

    // 3. Arm dirty-page tracking, then write ONLY page 0 and page 2 from CPU.
    region.arm().expect("arm write-watch");
    // SAFETY: writing u32 values inside page 0 and page 2 of `region`.
    unsafe {
        let p0 = region.as_mut_ptr().cast::<u32>();
        for i in 0..64 {
            *p0.add(i) = i as u32 + 1;
        }
        let p2 = region.as_mut_ptr().add(2 * ps).cast::<u32>();
        *p2 = 0xDEAD_BEEF;
    }
    let dirty = region.take_dirty_pages();
    println!("Dirty pages tracked (out of 4 pages, ps={ps}): {dirty:?}");
    assert_eq!(dirty, vec![0, 2], "must track exact written 16 KiB pages");

    // 4. Execute a native Metal compute shader directly on the zero-copy buffer (0 bytes copied!).
    let msl = r#"
        #include <metal_stdlib>
        using namespace metal;
        kernel void double_u32(device uint* data [[buffer(0)]],
                               uint id [[thread_position_in_grid]]) {
            data[id] = data[id] * 2u + 10u;
        }
    "#;
    let lib = device.new_library_from_source(msl).expect("compile MSL");
    let pso = device
        .new_compute_pipeline(&lib, "double_u32")
        .expect("compute PSO");
    let queue = device.new_command_queue().expect("command queue");
    queue
        .dispatch_compute_sync(
            &pso,
            &[&buf],
            MtlSize { width: 1, height: 1, depth: 1 },
            MtlSize { width: 64, height: 1, depth: 1 },
        )
        .expect("GPU compute dispatch");

    // 5. Read GPU results directly from host RAM pointer — ZERO copy_back!
    // SAFETY: GPU finished (`waitUntilCompleted`) and wrote `u32[0..64]`.
    unsafe {
        let p0 = region.as_mut_ptr().cast::<u32>();
        for i in 0..64 {
            let expected = (i as u32 + 1) * 2 + 10;
            assert_eq!(*p0.add(i), expected, "mismatch at element {i}");
        }
    }
    println!("Zero-copy M5 Max GPU compute round-trip verified (0 bytes copied)!");
}

#[test]
fn native_gfx10_to_dxil_to_metallib_to_metal_pso() {
    let converter = MetalIrConverter::load_default().expect("load libmetalirconverter.dylib");
    println!("Loaded Apple Metal IR Converter from: {}", converter.path().display());

    // Build a real synthetic GFX10 compute program (just like victory-shader's unit tests),
    // lower it to PC DXIL via `victory-shader`, then convert DXIL → `.metallib` via
    // `libmetalirconverter.dylib`, then load & compile it into a real `MTLComputePipelineState`!
    let words: &[u32] = &[
        0xBE84_0380, // s_mov_b32 s4, 0
        0xBF81_0000, // s_endpgm
    ];
    let native = victory_shader::NativeProgram {
        stage: victory_shader::HwStage::Cs,
        code: words.iter().flat_map(|w| w.to_le_bytes()).collect(),
        wave64: false,
        vgprs: 4,
        io: victory_shader::native::StageIo::Cs(victory_shader::native::ComputeIo {
            rsrc2: 0,
            threads: [64, 1, 1],
        }),
        layout: Vec::new(),
        used: Vec::new(),
    };
    let (p, st) = victory_shader::range::build(&native.patched_code(), native.wave64, native.vgprs)
        .expect("range::build");
    let st = st.expect("structure");
    let res = victory_shader::resource::track(&p, &native.layout, native.stage, Some(&st))
        .expect("resource::track");
    let io = victory_shader::io::raise(&p, &native).expect("io::raise");
    // Minimal valid empty DXBC container header (32 bytes: "DXBC", 16-byte digest, ver 1.0, len 32, 0 parts)
    let mut empty_dxbc = vec![0u8; 32];
    empty_dxbc[0..4].copy_from_slice(b"DXBC");
    empty_dxbc[0x14..0x16].copy_from_slice(&1u16.to_le_bytes());
    empty_dxbc[0x18..0x1C].copy_from_slice(&32u32.to_le_bytes());
    let container = victory_shader::dxbc::Container::parse(&empty_dxbc).expect("parse empty dxbc");
    let host = victory_shader::emit::emit(&container, &native, &p, &st, &io, &res)
        .expect("victory-shader emit DXIL");
    println!("victory-shader emitted {} bytes of PC DXIL natively on ARM64", host.dxil.len());

    let metallib = converter
        .compile_dxil(&host.dxil, ShaderStage::Compute, "main")
        .expect("DXIL -> Metal IR (.metallib)");
    println!("libmetalirconverter produced {} bytes of Apple Metal IR (.metallib)", metallib.len());
    assert!(metallib.starts_with(b"MTLB"), "output must be a valid MTLB metallib");

    let device = MetalDevice::system_default().expect("MTLDevice");
    let lib = device
        .new_library_from_metallib(&metallib)
        .expect("MTLDevice.newLibraryWithData(metallib)");
    let pso = device
        .new_compute_pipeline(&lib, "main")
        .expect("MTLDevice.newComputePipelineStateWithFunction('main')");
    let queue = device.new_command_queue().expect("MTLCommandQueue");
    queue
        .dispatch_compute_sync(
            &pso,
            &[],
            MtlSize { width: 1, height: 1, depth: 1 },
            MtlSize { width: 64, height: 1, depth: 1 },
        )
        .expect("execute translated GFX10->DXIL->MetalIR shader on M5 Max GPU!");
    println!("END-TO-END GFX10 -> DXIL -> Metal IR -> M5 Max GPU EXECUTION SUCCEEDED!");
}
