//! The only quantization boundary is the host output conversion.
pub(crate) trait Channel: Copy + Default + Send + Sync + 'static {
    const FLOAT: bool;
    fn unit(self) -> f64;
    fn unit32(self) -> f32;
    fn from_unit(value: f64) -> Self;
    fn decode(bytes: &[u8]) -> Vec<Self>;
}
impl Channel for u8 {
    const FLOAT: bool = false;
    fn unit(self) -> f64 { self as f64 / 255.0 }
    fn unit32(self) -> f32 { self as f32 / 255.0 }
    fn from_unit(value: f64) -> Self { (value * 255.0).clamp(0.0,255.0) as u8 }
    fn decode(bytes: &[u8]) -> Vec<Self> { bytes.to_vec() }
}
impl Channel for f32 {
    const FLOAT: bool = true;
    fn unit(self) -> f64 { self as f64 }
    fn unit32(self) -> f32 { self }
    fn from_unit(value: f64) -> Self { if value.is_finite() { value as f32 } else { 0.0 } }
    fn decode(bytes: &[u8]) -> Vec<Self> {
        bytes.chunks_exact(4).map(|b| f32::from_ne_bytes(b.try_into().unwrap())).collect()
    }
}

pub(crate) fn read_float(buf: &[u8], w: usize, h: usize, stride: usize, depth: u16) -> Vec<f32> {
    let bytes = match depth { 16 => 2, 32 => 4, _ => 1 };
    let mut out = vec![0.0; w*h*4];
    for y in 0..h {
        let Some(row) = buf.get(y*stride..y*stride+w*4*bytes) else { continue };
        for (dst, src) in out[y*w*4..(y+1)*w*4].iter_mut().zip(row.chunks_exact(bytes)) {
            *dst = match depth {
                16 => u16::from_ne_bytes(src.try_into().unwrap()) as f32 / 32768.0,
                32 => f32::from_ne_bytes(src.try_into().unwrap()),
                _ => src[0] as f32 / 255.0,
            };
            if !dst.is_finite() { *dst = 0.0; }
        }
    }
    out
}

pub(crate) fn write_float(src: &[f32], buf: &mut [u8], w: usize, h: usize, stride: usize, depth: u16) {
    let bytes = match depth { 16 => 2, 32 => 4, _ => 1 };
    for y in 0..h {
        let Some(row) = buf.get_mut(y*stride..y*stride+w*4*bytes) else { continue };
        for (src, dst) in src[y*w*4..(y+1)*w*4].iter().zip(row.chunks_exact_mut(bytes)) {
            let v = if src.is_finite() { *src } else { 0.0 };
            if depth == 16 { dst.copy_from_slice(&((v*32768.0).round().clamp(0.0,32768.0) as u16).to_ne_bytes()); }
            else if depth == 32 { dst.copy_from_slice(&v.to_ne_bytes()); }
            else { dst[0]=(v*255.0).round().clamp(0.0,255.0) as u8; }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_16bit_levels_roundtrip_with_padded_rows() {
        let (w,h,stride)=(257,129,257*8+24);
        let mut input=vec![0xA5;stride*h];
        for y in 0..h { for x in 0..w*4 {
            let v=((y*w*4+x)%32769) as u16;
            input[y*stride+x*2..y*stride+x*2+2].copy_from_slice(&v.to_ne_bytes());
        }}
        let decoded=read_float(&input,w,h,stride,16);
        let mut output=vec![0xA5;stride*h];
        write_float(&decoded,&mut output,w,h,stride,16);
        assert_eq!(input,output);
    }
}
