    /// Exercise the console-side command list callback for adjacent barriers,
    /// including the first barrier after reset and an explicitly marked writer.
    #[test]
    #[cfg(windows)]
    fn mac_adjacent_barrier_groups() {
        use std::sync::atomic::Ordering::Relaxed;
        let hardware = crate::host::host().expect("hardware D3D12 host");
        assert!(!hardware.on_warp());
        let null = std::ptr::null();
        let mut allocator = null;
        let mut guest = null;
        assert_eq!(create_allocator(null, 0, &ALLOCATOR_IIDS[0], &mut allocator), S_OK);
        assert_eq!(create_list(null, 0, 0, allocator, null, &LIST_IIDS[1], &mut guest), S_OK);
        let list = host_list(guest).unwrap();
        assert!(list.uav_pending());
        let counters = &crate::host::BARRIER_STATS;
        let (sent, skipped)=(counters[0].load(Relaxed),counters[1].load(Relaxed));
        for epoch in 0..7 {
            if epoch != 0 {
                list.set_uav_pending(true); // simulate a potentially writing draw/dispatch
            }
            for _ in 0..3 {
                barrier_groups(guest, 1, std::ptr::null());
            }
        }
        assert_eq!(close(guest),S_OK);
        hardware.execute(&[list]);
        let emitted = counters[0].load(Relaxed)-sent;
        let coalesced = counters[1].load(Relaxed)-skipped;
        if crate::host::barrier_merge() {
            assert_eq!((emitted,coalesced),(7,14));
        } else {
            assert_eq!((emitted,coalesced),(0,0)); // counters only instrument the merge path
        }
        println!("GROUPS submitted=21 emitted={} coalesced={coalesced} hardware=true", if crate::host::barrier_merge(){emitted}else{21});
        list_release(guest);
        allocator_release(allocator);
    }
