#[cfg(test)]
mod mac_full_texture {
    use super::*;
    use victory_formats::texture::{Dimension, Swizzle, Texture, TextureDesc};
    #[test]
    fn updated_then_zeroed_texture_in_one_frame() {
        use std::sync::atomic::Ordering::Relaxed;
        assert!(!crate::fault_tracking());
        let h=crate::host::host().expect("hardware D3D12 device");
        assert!(!h.on_warp());
        let (width,height)=(32,8);
        let desc=TextureDesc {dimension:Dimension::Tex2d,width,height,depth_or_array:1,mips:1,
            format:28,samples:1,swizzle:Swizzle::SW_4KB_S};
        let layout=Texture::new(&desc).unwrap().planes[0].layout.clone();
        let size=layout.size as usize;
        let mut title=vec![0u8;size];
        let bytes=256*height as usize;
        let (buffer,data)=h.upload_buffer(bytes as u64).unwrap();
        STAGED[0].fetch_add(1,Relaxed);
        STAGED[1].fetch_add(bytes as u64,Relaxed);
        let fp=D3D12_PLACED_SUBRESOURCE_FOOTPRINT {
            Offset:0, Footprint:D3D12_SUBRESOURCE_FOOTPRINT {Width:width,Height:height,Depth:1,RowPitch:256,..Default::default()}
        };
        let stage=Staging {
            buffer,#[cfg(feature="diagnostics")] total:bytes as u64,data,footprints:vec![fp],rows:vec![height],layout:layout.clone(),mips:1,
            first:0,size,bytes:bytes as u64,memory:title.as_ptr(),last:Mutex::default(),dirty:Default::default(),
            uploaded:Default::default(),is_view:Default::default(),used:Default::default()
        };
        unsafe {data.write_bytes(0x7c,bytes);}
        for pass in 0..3 {
            for (i,x) in title.iter_mut().enumerate() {
                *x=if pass==2 {0} else {((i*31+pass)^(i>>6)) as u8};
            }
            let mut expected=vec![0x7c;bytes];
            layout.detile(&title,0,0,&mut expected,256,bytes).unwrap();
            assert_eq!(stage.refresh_now(),size);
            assert_eq!(unsafe{std::slice::from_raw_parts(data,bytes)},expected);
        }
        println!("FULL_COPY_OK texture_bytes={size} staged_bytes={bytes} passes=3 including_zero=true");
    }
}
