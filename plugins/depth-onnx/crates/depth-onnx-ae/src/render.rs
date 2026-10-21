use after_effects as ae;

fn cpu_depth(format: ae::PixelFormat) -> Option<u8> {
    match format {
        ae::PixelFormat::Argb32 => Some(8),
        ae::PixelFormat::Argb64 => Some(16),
        ae::PixelFormat::Argb128 => Some(32),
        _ => None,
    }
}

fn valid_depth_map(depth: &[f32], size: i32) -> bool {
    size > 0 && (size as usize).checked_mul(size as usize) == Some(depth.len())
        && depth.iter().all(|v|v.is_finite())
}

fn validate_layout(w: usize, h: usize, rowbytes: isize, address: usize, depth: u8) -> Result<(), ae::Error> {
    let channel = match depth { 8 => 1, 16 => 2, 32 => 4, _ => return Err(ae::Error::BadCallbackParameter) };
    let row = w.checked_mul(4 * channel).ok_or(ae::Error::BadCallbackParameter)?;
    let stride = rowbytes.checked_abs().ok_or(ae::Error::BadCallbackParameter)? as usize;
    if w == 0 || h == 0 || w > i32::MAX as usize || h > i32::MAX as usize
        || address == 0 || address % channel != 0 || stride % channel != 0 || stride < row {
        return Err(ae::Error::BadCallbackParameter);
    }
    let offset = (h-1).checked_mul(stride).ok_or(ae::Error::BadCallbackParameter)?;
    let span = offset.checked_add(row).filter(|&v|v <= isize::MAX as usize).ok_or(ae::Error::BadCallbackParameter)?;
    let valid_address = if rowbytes < 0 {address.checked_sub(offset).is_some() && address.checked_add(row).is_some()}
        else {address.checked_add(span).is_some()};
    if !valid_address {return Err(ae::Error::BadCallbackParameter);}
    Ok(())
}

pub fn world_to_float_rgba(input: &ae::Layer) -> Result<Vec<f32>, ae::Error> {
    let depth = cpu_depth(input.pixel_format()?).ok_or(ae::Error::BadCallbackParameter)?;
    read_cpu_world(input, depth)
}

fn read_cpu_world(input: &ae::Layer, depth: u8) -> Result<Vec<f32>, ae::Error> {
    validate_layout(input.width(), input.height(), input.row_bytes(), unsafe {input.data_ptr()} as usize, depth)?;
    let width = input.width() as i32;
    let height = input.height() as i32;
    let w = width as usize;
    let h = height as usize;
    let count=w.checked_mul(h).and_then(|n|n.checked_mul(4)).ok_or(ae::Error::BadCallbackParameter)?;
    let mut rgba=Vec::new();
    rgba.try_reserve_exact(count).map_err(|_|ae::Error::OutOfMemory)?;
    rgba.resize(count,0.0f32);

    match depth {
        8 => {
            const INV: f32 = 1.0 / 255.0;
            for y in 0..h {
                for x in 0..w {
                    let p = input.as_pixel8(x, y);
                    let i = (y * w + x) * 4;
                    rgba[i] = p.alpha as f32 * INV;
                    rgba[i + 1] = p.red as f32 * INV;
                    rgba[i + 2] = p.green as f32 * INV;
                    rgba[i + 3] = p.blue as f32 * INV;
                }
            }
        }
        16 => {
            const INV: f32 = 1.0 / 32768.0;
            for y in 0..h {
                for x in 0..w {
                    let p = input.as_pixel16(x, y);
                    let i = (y * w + x) * 4;
                    rgba[i] = p.alpha as f32 * INV;
                    rgba[i + 1] = p.red as f32 * INV;
                    rgba[i + 2] = p.green as f32 * INV;
                    rgba[i + 3] = p.blue as f32 * INV;
                }
            }
        }
        32 => {
            for y in 0..h {
                for x in 0..w {
                    let p = input.as_pixel32(x, y);
                    let i = (y * w + x) * 4;
                    rgba[i] = p.alpha;
                    rgba[i + 1] = p.red;
                    rgba[i + 2] = p.green;
                    rgba[i + 3] = p.blue;
                }
            }
        }
        _ => return Err(ae::Error::BadCallbackParameter),
    }

    Ok(rgba)
}

pub fn write_depth_to_world(
    output: &mut ae::Layer,
    depth: &[f32],
    size: i32,
    invert: bool,
) -> Result<(), ae::Error> {
    let output_depth = cpu_depth(output.pixel_format()?).ok_or(ae::Error::BadCallbackParameter)?;
    write_cpu_world(output, depth, size, invert, output_depth)
}

fn write_cpu_world(output: &mut ae::Layer, depth: &[f32], size: i32, invert: bool, output_depth: u8) -> Result<(), ae::Error> {
    if !valid_depth_map(depth,size) { return Err(ae::Error::BadCallbackParameter); }
    validate_layout(output.width(), output.height(), output.row_bytes(), unsafe {output.data_ptr()} as usize, output_depth)?;
    let width = output.width() as i32;
    let height = output.height() as i32;
    let w = width as usize;
    let h = height as usize;

    match output_depth {
        8 => {
            for y in 0..h {
                for x in 0..w {
                    let d = sample(depth, size, width, height, x as i32, y as i32, invert);
                    let g = (d * 255.0) as u8;
                    let out = output.as_pixel8_mut(x, y);
                    out.alpha = 255;
                    out.red = g;
                    out.green = g;
                    out.blue = g;
                }
            }
        }
        16 => {
            for y in 0..h {
                for x in 0..w {
                    let d = sample(depth, size, width, height, x as i32, y as i32, invert);
                    let g = (d * 32768.0) as u16;
                    let out = output.as_pixel16_mut(x, y);
                    out.alpha = 32768;
                    out.red = g;
                    out.green = g;
                    out.blue = g;
                }
            }
        }
        32 => {
            for y in 0..h {
                for x in 0..w {
                    let d = sample(depth, size, width, height, x as i32, y as i32, invert);
                    let out = output.as_pixel32_mut(x, y);
                    out.alpha = 1.0;
                    out.red = d;
                    out.green = d;
                    out.blue = d;
                }
            }
        }
        _ => return Err(ae::Error::BadCallbackParameter),
    }

    Ok(())
}

fn sample(depth: &[f32], size: i32, width: i32, height: i32, x: i32, y: i32, invert: bool) -> f32 {
    let u = if width > 1 {
        x as f32 / (width - 1) as f32
    } else {
        0.0
    };
    let v = if height > 1 {
        y as f32 / (height - 1) as f32
    } else {
        0.0
    };
    let mut d = sample_bilinear(depth, size, u, v);
    if invert {
        d = 1.0 - d;
    }
    d.clamp(0.0, 1.0)
}

fn sample_bilinear(depth: &[f32], size: i32, u: f32, v: f32) -> f32 {
    let size = size.max(1);
    let fx = u.clamp(0.0, 1.0) * (size - 1) as f32;
    let fy = v.clamp(0.0, 1.0) * (size - 1) as f32;
    let x0 = fx.floor() as i32;
    let y0 = fy.floor() as i32;
    let x1 = (x0 + 1).min(size - 1);
    let y1 = (y0 + 1).min(size - 1);
    let wx = fx - x0 as f32;
    let wy = fy - y0 as f32;

    let v00 = depth[(y0 * size + x0) as usize];
    let v01 = depth[(y0 * size + x1) as usize];
    let v10 = depth[(y1 * size + x0) as usize];
    let v11 = depth[(y1 * size + x1) as usize];
    let v0 = v00 * (1.0 - wx) + v01 * wx;
    let v1 = v10 * (1.0 - wx) + v11 * wx;
    v0 * (1.0 - wy) + v1 * wy
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn padded_and_negative_worlds_preserve_pixels_and_guards() {
        for (depth,bytes) in [(8,1usize),(16,2),(32,4)] {
            for (w,h) in [(1usize,7usize),(7,1),(7,5)] {
                for reverse in [false,true] {
                    let stride=w*4*bytes+16;
                    let size=32+stride*h+32;
                    let mut storage=vec![0xa5a5a5a5u32;size/4];
                    let raw_bytes=unsafe {std::slice::from_raw_parts_mut(storage.as_mut_ptr() as *mut u8,size)};
                    let mut expected=Vec::new();
                    for y in 0..h {for x in 0..w {for channel in 0..4 {
                        let i=(y*w+x)*4+channel;
                        let physical=if reverse {h-1-y}else{y};
                        let offset=32+physical*stride+(x*4+channel)*bytes;
                        let (encoded,value)=match depth {
                            8=>{let v=(i%256) as u8;(vec![v],v as f32*(1.0/255.0))},
                            16=>{let v=16385+(i%100) as u16;(v.to_ne_bytes().to_vec(),v as f32/32768.0)},
                            _=>{let v=-0.25+(i%31) as f32/8.0;(v.to_ne_bytes().to_vec(),v)},
                        };
                        raw_bytes[offset..offset+bytes].copy_from_slice(&encoded);
                        expected.push(value);
                    }}}
                    let mut raw:ae::sys::PF_LayerDef=unsafe {std::mem::zeroed()};
                    raw.width=w as _;raw.height=h as _;
                    raw.rowbytes=if reverse {-(stride as i32)}else{stride as i32};
                    raw.data=unsafe {raw_bytes.as_mut_ptr().add(32+if reverse {(h-1)*stride}else{0})} as _;
                    let mut layer=ae::Layer::from_raw(&mut raw,std::ptr::null::<ae::sys::PF_InData>(),None);
                    assert_eq!(read_cpu_world(&layer,depth).unwrap(),expected);
                    for invert in [false,true] {
                        write_cpu_world(&mut layer,&[0.375],1,invert,depth).unwrap();
                        let value=if invert {0.625}else{0.375};
                        let gray=match depth {8=>(value*255.0) as u8 as f32*(1.0/255.0),16=>value,_=>value};
                        let decoded=read_cpu_world(&layer,depth).unwrap();
                        for pixel in decoded.chunks_exact(4) {assert_eq!(pixel,&[1.0,gray,gray,gray]);}
                        assert!(raw_bytes[..32].iter().all(|&b|b==0xa5));
                        assert!(raw_bytes[32+stride*h..].iter().all(|&b|b==0xa5));
                        for row in 0..h {assert!(raw_bytes[32+row*stride+w*4*bytes..32+(row+1)*stride].iter().all(|&b|b==0xa5));}
                    }
                }
            }
        }
    }
    #[test]
    fn native_world_layout_rejects_invalid_addresses_and_strides() {
        for (depth, bytes) in [(8,1),(16,2),(32,4)] {
            for (w,h) in [(1,31),(17,1),(17,31)] {
                let stride = w*4*bytes+4*bytes;
                assert!(validate_layout(w,h,stride as isize,0x100000,depth).is_ok());
                assert!(validate_layout(w,h,-(stride as isize),0x100000,depth).is_ok());
                assert!(validate_layout(w,h,(w*4*bytes-1) as isize,0x100000,depth).is_err());
                assert!(validate_layout(w,h,stride as isize,0,depth).is_err());
                if bytes>1 {
                    assert!(validate_layout(w,h,stride as isize,0x100001,depth).is_err());
                    assert!(validate_layout(w,h,(stride+1) as isize,0x100000,depth).is_err());
                }
            }
        }
        for (w,h,stride,address,depth) in [(0,1,4,4096,8),(1,0,4,4096,8),
            (usize::MAX,1,4,4096,8),(1,2,isize::MIN,4096,8),
            (1,2,isize::MAX,4096,8),(1,2,-16,8,8),(1,1,4,usize::MAX,8),(1,1,4,4096,64)] {
            assert!(validate_layout(w,h,stride,address,depth).is_err());
        }
    }
    #[test]
    fn dispatch_uses_explicit_host_pixel_format() {
        assert_eq!(cpu_depth(ae::PixelFormat::Argb32),Some(8));
        assert_eq!(cpu_depth(ae::PixelFormat::Argb64),Some(16));
        assert_eq!(cpu_depth(ae::PixelFormat::Argb128),Some(32));
    }
    #[test]
    fn invalid_inference_shapes_and_nonfinite_values_are_rejected() {
        assert!(!valid_depth_map(&[],0));
        assert!(!valid_depth_map(&[0.0],-1));
        assert!(!valid_depth_map(&[0.0;3],2));
        assert!(!valid_depth_map(&[f32::NAN],1));
        assert!(!valid_depth_map(&[f32::INFINITY],1));
        assert!(valid_depth_map(&[0.0,0.25,0.75,1.0],2));
    }
    #[test]
    fn depth_sampling_endpoints_inversion_and_single_pixel() {
        let depth=[0.0,0.25,0.75,1.0];
        assert_eq!(sample(&depth,2,3,3,1,1,false),0.5);
        assert_eq!(sample(&depth,2,3,3,2,2,false),1.0);
        assert_eq!(sample(&depth,2,3,3,2,2,true),0.0);
        assert_eq!(sample(&[0.375],1,1,1,0,0,false),0.375);
    }
}
