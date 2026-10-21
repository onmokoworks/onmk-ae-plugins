//! Native-depth codecs. Float channels are never quantized or clamped to SDR.
use after_effects as ae;

pub trait Channel: Copy + Default {
    const BYTES: usize;
    fn read(bytes: &[u8]) -> Self;
    fn write(self, bytes: &mut [u8]);
    fn normalized(self) -> f64;
    fn blend(original: Self, scattered: Self, inv: f32, mix: f32) -> Self;
}

impl Channel for u8 {
    const BYTES: usize = 1;
    fn read(b: &[u8]) -> Self { b[0] }
    fn write(self, b: &mut [u8]) { b[0] = self; }
    fn normalized(self) -> f64 { self as f64 / 255.0 }
    fn blend(o: Self, s: Self, inv: f32, mix: f32) -> Self {
        (o as f32 * inv + s as f32 * mix).clamp(0.0, 255.0) as u8
    }
}
impl Channel for u16 {
    const BYTES: usize = 2;
    fn read(b: &[u8]) -> Self { u16::from_ne_bytes([b[0], b[1]]) }
    fn write(self, b: &mut [u8]) { b.copy_from_slice(&self.to_ne_bytes()); }
    fn normalized(self) -> f64 { self as f64 / 32768.0 }
    fn blend(o: Self, s: Self, inv: f32, mix: f32) -> Self {
        (o as f32 * inv + s as f32 * mix).clamp(0.0, 32768.0) as u16
    }
}
impl Channel for f32 {
    const BYTES: usize = 4;
    fn read(b: &[u8]) -> Self { f32::from_ne_bytes(b.try_into().unwrap()) }
    fn write(self, b: &mut [u8]) { b.copy_from_slice(&self.to_ne_bytes()); }
    fn normalized(self) -> f64 { self as f64 }
    fn blend(o: Self, s: Self, inv: f32, mix: f32) -> Self { o * inv + s * mix }
}

pub fn channel_bytes(layer: &ae::Layer) -> Result<usize, ae::Error> {
    match layer.pixel_format()? {
        ae::PixelFormat::Argb32 => Ok(1),
        ae::PixelFormat::Argb64 => Ok(2),
        ae::PixelFormat::Argb128 => Ok(4),
        _ => Err(ae::Error::BadCallbackParameter),
    }
}

fn checked_layout(w:usize,h:usize,stride:usize,bytes:usize)->Result<usize,ae::Error> {
    if w>i32::MAX as usize || h>i32::MAX as usize {return Err(ae::Error::BadCallbackParameter);}
    let row=w.checked_mul(4).and_then(|v|v.checked_mul(bytes)).ok_or(ae::Error::OutOfMemory)?;
    let length=stride.checked_mul(h).ok_or(ae::Error::OutOfMemory)?;
    if w==0 || h==0 || stride<row || length>isize::MAX as usize {return Err(ae::Error::BadCallbackParameter);}
    Ok(length)
}

fn geometry<T: Channel>(layer: &ae::Layer) -> Result<(usize, usize, usize), ae::Error> {
    // Avoid SDK abs() overflow before obtaining the unsigned buffer stride.
    if layer.row_bytes()==i32::MIN as isize {return Err(ae::Error::BadCallbackParameter);}
    let (w, h, stride) = (layer.width(), layer.height(), layer.buffer_stride());
    checked_layout(w,h,stride,T::BYTES)?;
    if channel_bytes(layer)? != T::BYTES
        || unsafe { layer.data_ptr() }.is_null() {
        return Err(ae::Error::BadCallbackParameter);
    }
    Ok((w, h, stride))
}

pub fn read<T: Channel>(layer: &ae::Layer) -> Result<(Vec<T>, usize, usize), ae::Error> {
    let (w, h, stride) = geometry::<T>(layer)?;
    let n = w.checked_mul(h).and_then(|v| v.checked_mul(4)).ok_or(ae::Error::OutOfMemory)?;
    let mut flat = Vec::new();
    flat.try_reserve_exact(n).map_err(|_| ae::Error::OutOfMemory)?;
    let buf = layer.buffer();
    for y in 0..h {
        let row = if layer.row_bytes() < 0 { h - 1 - y } else { y };
        let start = row * stride;
        let bytes = buf.get(start..start + w * 4 * T::BYTES).ok_or(ae::Error::BadCallbackParameter)?;
        flat.extend(bytes.chunks_exact(T::BYTES).map(T::read));
    }
    Ok((flat, w, h))
}

pub fn write<T: Channel>(flat: &[T], layer: &mut ae::Layer, w: usize, h: usize) -> Result<(), ae::Error> {
    let (ow, oh, stride) = geometry::<T>(layer)?;
    let n=w.checked_mul(h).and_then(|v|v.checked_mul(4)).ok_or(ae::Error::OutOfMemory)?;
    if (ow, oh) != (w, h) || flat.len() != n { return Err(ae::Error::BadCallbackParameter); }
    let bottom_up = layer.row_bytes() < 0;
    let buf = layer.buffer_mut();
    for y in 0..h {
        let row = if bottom_up { h - 1 - y } else { y };
        let start = row * stride;
        let bytes = buf.get_mut(start..start + w * 4 * T::BYTES).ok_or(ae::Error::BadCallbackParameter)?;
        for (value, dst) in flat[y*w*4..(y+1)*w*4].iter().zip(bytes.chunks_exact_mut(T::BYTES)) {
            value.write(dst);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn layout_rejects_signed_dimensions_and_oversized_slices() {
        for bytes in [1,2,4] {
            assert_eq!(checked_layout(17,31,17*4*bytes+8,bytes),Ok((17*4*bytes+8)*31));
            assert!(checked_layout(17,31,17*4*bytes-1,bytes).is_err());
        }
        assert!(checked_layout(i32::MAX as usize+1,1,usize::MAX,1).is_err());
        assert!(checked_layout(1,i32::MAX as usize+1,4,1).is_err());
        assert!(checked_layout(1,2,isize::MAX as usize,1).is_err());
        assert!(checked_layout(1,3,usize::MAX,1).is_err());
        assert!(checked_layout(0,1,0,1).is_err());
    }
    #[test]
    fn sixteen_bit_low_bits_survive_codec() {
        for value in 0u16..=32768 {
            let mut bytes = [0; 2]; value.write(&mut bytes);
            assert_eq!(u16::read(&bytes), value);
        }
    }
    #[test]
    fn native16_mix_preserves_low_bits_with_integer_truncation() {
        for original in 0..=32768u32 {
            for scattered in [0, original, original.saturating_sub(1), (original + 1).min(32768), 32768 - original, 32768] {
                for numerator in 0..=4u32 {
                    let mix = numerator as f32 / 4.0;
                    let expected = (original * (4 - numerator) + scattered * numerator) / 4;
                    assert_eq!(u16::blend(original as u16, scattered as u16, 1.0 - mix, mix), expected as u16);
                }
            }
        }
    }
    #[test]
    fn float_hdr_and_negative_survive_codec_and_mix() {
        for value in [-3.5f32, 0.1234567, 4.75] {
            let mut bytes = [0; 4]; value.write(&mut bytes);
            assert_eq!(f32::read(&bytes).to_bits(), value.to_bits());
            assert_eq!(f32::blend(value, value, 0.5, 0.5), value);
        }
    }
}
