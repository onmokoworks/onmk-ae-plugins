//! Native-depth codecs. Float channels are never quantized or clamped to SDR.
use after_effects as ae;

pub trait Channel: Copy + Default {
    const BYTES: usize;
    fn read(bytes: &[u8]) -> Self;
    fn write(self, bytes: &mut [u8]);
    fn normalized(self) -> f64;
}

impl Channel for u8 {
    const BYTES: usize = 1;
    fn read(b: &[u8]) -> Self { b[0] }
    fn write(self, b: &mut [u8]) { b[0] = self; }
    fn normalized(self) -> f64 { self as f64 / 255.0 }
}
impl Channel for u16 {
    const BYTES: usize = 2;
    fn read(b: &[u8]) -> Self { u16::from_ne_bytes([b[0], b[1]]) }
    fn write(self, b: &mut [u8]) { b.copy_from_slice(&self.to_ne_bytes()); }
    fn normalized(self) -> f64 { self as f64 / 32768.0 }
}
impl Channel for f32 {
    const BYTES: usize = 4;
    fn read(b: &[u8]) -> Self { f32::from_ne_bytes(b.try_into().unwrap()) }
    fn write(self, b: &mut [u8]) { b.copy_from_slice(&self.to_ne_bytes()); }
    fn normalized(self) -> f64 { self as f64 }
}

pub fn channel_bytes(layer: &ae::Layer) -> Result<usize, ae::Error> {
    match layer.pixel_format()? {
        ae::PixelFormat::Argb32 => Ok(1),
        ae::PixelFormat::Argb64 => Ok(2),
        ae::PixelFormat::Argb128 => Ok(4),
        _ => Err(ae::Error::BadCallbackParameter),
    }
}

fn geometry<T: Channel>(layer: &ae::Layer) -> Result<(usize, usize, usize), ae::Error> {
    let (w, h, stride) = (layer.width(), layer.height(), layer.buffer_stride());
    let row = w.checked_mul(4 * T::BYTES).ok_or(ae::Error::OutOfMemory)?;
    stride.checked_mul(h).ok_or(ae::Error::OutOfMemory)?;
    if channel_bytes(layer)? != T::BYTES || w == 0 || h == 0 || stride < row
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
    if (ow, oh) != (w, h) || flat.len() != w * h * 4 { return Err(ae::Error::BadCallbackParameter); }
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
    fn sixteen_bit_low_bits_survive_codec() {
        for value in 0u16..=32768 {
            let mut bytes = [0; 2]; value.write(&mut bytes);
            assert_eq!(u16::read(&bytes), value);
        }
    }
    #[test]
    fn float_hdr_and_negative_survive_codec() {
        for value in [-3.5f32, 0.1234567, 4.75] {
            let mut bytes = [0; 4]; value.write(&mut bytes);
            assert_eq!(f32::read(&bytes).to_bits(), value.to_bits());
        }
    }
}
