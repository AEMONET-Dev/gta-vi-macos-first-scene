    /// Executes the production bound / done path with the five captured dispatch arguments.
    /// A tiny counter shader measures actual groups launched at each site.
    #[test]
    fn mac_captured_indirect() {
        let h = host().expect("a D3D12 host device is required");
        assert!(!h.on_warp(), "hardware adapter required");
        let worker = compile_cs(&h.device, WORKER_CS).expect("worker CS");
        let words: Vec<u32> = include_bytes!("captured-inputs.bin").chunks_exact(4)
            .map(|bytes| u32::from_le_bytes(bytes.try_into().unwrap())).collect();
        let calls = 100usize;
        let (source, src_ptr) = h.shared_buffer(128).unwrap();
        let (_hits, hits_ptr) = h.shared_buffer((calls * 4) as u64).unwrap();
        let hits_gpu = unsafe { _hits.GetGPUVirtualAddress() };
        let _ = clamp().expect("fork's argument clamp must compile");
        let arg = D3D12_INDIRECT_ARGUMENT_DESC {
            Type: D3D12_INDIRECT_ARGUMENT_TYPE_DISPATCH, ..Default::default()
        };
        let desc = D3D12_COMMAND_SIGNATURE_DESC {
            ByteStride: 12, NumArgumentDescs: 1, pArgumentDescs: &arg, NodeMask: 0
        };
        let mut sig: Option<ID3D12CommandSignature> = None;
        let mut query: Option<ID3D12QueryHeap> = None;
        let mut readback: Option<ID3D12Resource> = None;
        let mut timestamp_data: *mut std::ffi::c_void = std::ptr::null_mut();
        unsafe {
            h.device.CreateCommandSignature(&desc, None::<&ID3D12RootSignature>, &mut sig).unwrap();
            std::ptr::copy_nonoverlapping(words.as_ptr(), src_ptr.cast::<u32>(), words.len());
            h.device.CreateQueryHeap(&D3D12_QUERY_HEAP_DESC {
                Type: D3D12_QUERY_HEAP_TYPE_TIMESTAMP, Count: 2, NodeMask: 0
            },&mut query).unwrap();
            let heap = D3D12_HEAP_PROPERTIES {Type: D3D12_HEAP_TYPE_READBACK, ..Default::default()};
            let desc = D3D12_RESOURCE_DESC {
                Dimension: D3D12_RESOURCE_DIMENSION_BUFFER, Width: 16, Height: 1, DepthOrArraySize: 1, MipLevels: 1,
                SampleDesc: DXGI_SAMPLE_DESC {Count:1, Quality:0}, Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
                ..Default::default()
            };
            h.device.CreateCommittedResource(&heap,D3D12_HEAP_FLAG_NONE,&desc,D3D12_RESOURCE_STATE_COPY_DEST,None,&mut readback).unwrap();
            readback.as_ref().unwrap().Map(0,None,Some(&mut timestamp_data)).unwrap();
        }
        let sig = sig.unwrap();
        let query = query.unwrap();
        let readback = readback.unwrap();
        let frequency = h.experiment_time_frequency();
        assert!(frequency > 0);
        for round in 0..10 {
            let start = std::time::Instant::now();
            let list = h.new_list().unwrap();
            unsafe {
                std::ptr::write_bytes(hits_ptr, 0, calls * 4);
                list.list.EndQuery(&query, D3D12_QUERY_TYPE_TIMESTAMP, 0);
            }
            let mut pre_global = 0;
            let mut pre_scoped = 0;
            let mut post_global = 0;
            for i in 0..calls {
                let slot = i % (words.len() / 3);
                // Exercise both ownership branches; the buffers themselves are dedicated.
                let own = i % 4 != 0;
                let waits = clamp_waits(&list, own);
                pre_global += u32::from(waits.before && !waits.scoped);
                pre_scoped += u32::from(waits.before && waits.scoped);
                post_global += u32::from(waits.after);
                let scratch = bound(&list, (&source,(slot * 12) as u64), None, (2,12,1), own).expect("bound");
                unsafe {
                    list.list.SetComputeRootSignature(&worker.root);
                    list.list.SetPipelineState(&worker.pso);
                    list.list.SetComputeRootUnorderedAccessView(0, hits_gpu + (i*4) as u64);
                    list.list.ExecuteIndirect(&sig,1,&scratch,0,None,0);
                }
                list.set_uav_pending(true); // worker has a writable UAV
                done(&list,&scratch);
            }
            unsafe {
                list.list.EndQuery(&query, D3D12_QUERY_TYPE_TIMESTAMP, 1);
                list.list.ResolveQueryData(&query, D3D12_QUERY_TYPE_TIMESTAMP, 0, 2, &readback, 0);
            }
            list.close();
            h.execute(&[&list]);
            let elapsed = start.elapsed().as_secs_f64()*1000.0;
            let stamps = unsafe { std::slice::from_raw_parts(timestamp_data.cast::<u64>(),2) };
            assert!(stamps[1] >= stamps[0]);
            let gpu = (stamps[1]-stamps[0]) as f64 * 1000.0 / frequency as f64;
            for i in 0..calls {
                let offset = i % (words.len() / 3) * 3;
                let expected: u32 = words[offset..offset+3].iter().product();
                assert_eq!(unsafe {hits_ptr.cast::<u32>().add(i).read_volatile()},expected,"dispatch {i}");
            }
            assert_eq!(unsafe {std::slice::from_raw_parts(src_ptr.cast::<u32>(),words.len())},words);
            println!("BENCH round={round} gpu_ms={gpu:.6} wall_ms={elapsed:.6} pre_global={pre_global} pre_scoped={pre_scoped} post_global={post_global} verified={calls}");
        }
    }
