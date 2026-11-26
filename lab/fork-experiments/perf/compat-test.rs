#[cfg(test)]
mod mac_no_watch {
    use super::*;
    use windows_sys::Win32::Foundation::{CloseHandle, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::Memory as mem;

    #[test]
    fn alias_and_same_frame_writes_refresh() {
        assert!(!crate::fault_tracking());
        let host=crate::host::host().expect("hardware device");
        assert!(!host.on_warp());
        const N: usize = 2*PAGE+32;
        unsafe {
            let section=mem::CreateFileMappingW(INVALID_HANDLE_VALUE,std::ptr::null(),mem::PAGE_READWRITE,0,N as u32,std::ptr::null());
            assert!(!section.is_null());
            let a=mem::MapViewOfFile(section,mem::FILE_MAP_WRITE,0,0,N);
            let b=mem::MapViewOfFile(section,mem::FILE_MAP_WRITE,0,0,N);
            assert!(!a.Value.is_null() && !b.Value.is_null() && a.Value!=b.Value);
            let addr=a.Value as usize;
            let (buffer,data)=host.shared_buffer(N as u64).unwrap();
            let gpu=buffer.GetGPUVirtualAddress();
            let shadow=Shadow::new(buffer,data,gpu,addr as u64,N as u64);
            let mut oracle=(0..N).map(|i| ((i*61) ^ (i>>7)) as u8).collect::<Vec<_>>();
            std::ptr::copy_nonoverlapping(oracle.as_ptr(),a.Value.cast(),N);
            assert_eq!(shadow.update(addr,N),N);
            assert_eq!(std::slice::from_raw_parts(data,N),oracle);
            let offsets=[0,PAGE-1,PAGE,PAGE+1,N-1];
            for (step,offset) in offsets.into_iter().enumerate() {
                oracle[offset]^=0xff;
                let ptr=if step%2==0 {b.Value}else{a.Value};
                ptr.cast::<u8>().add(offset).write_volatile(oracle[offset]);
                assert_eq!(shadow.update(addr,N),N);
                assert_eq!(std::slice::from_raw_parts(data,N),oracle);
            }
            let held=20..24;
            shadow.pending.lock().unwrap().push((addr+held.start,addr+held.end));
            for i in held.clone() {
                data.add(i).write_volatile(0xa0);
                oracle[i]=0xa0;
                b.Value.cast::<u8>().add(i).write_volatile(0x11);
            }
            oracle[24]=0xf2;
            b.Value.cast::<u8>().add(24).write_volatile(0xf2);
            assert_eq!(shadow.update(addr,N),N-held.len());
            assert_eq!(std::slice::from_raw_parts(data,N),oracle);
            assert!(crate::watch::writes_kind(addr,addr+N,true).is_none());
            assert!(crate::watch::writes_guarded(addr,addr+N).is_none());
            drop(shadow);
            assert_ne!(mem::UnmapViewOfFile(b),0);
            assert_ne!(mem::UnmapViewOfFile(a),0);
            assert_ne!(CloseHandle(section),0);
            println!("FULL_COPY_OK bytes={N} full_reads=6 partial_reads=1 alias_views=2");
        }
    }
}
