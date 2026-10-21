// SPDX-License-Identifier: Apache-2.0
// onmk implementation (2026); upstream attribution and changes: see package NOTICE.
//! AE effect-world adapters: ARGB (8/16/32 bpc, signed stride) <-> straight
//! RGBA f32. Channel order and precision only; no alpha association here.

use crate::core::{Image, RenderError};
use after_effects as ae;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Depth {
    U8,
    U16,
    F32,
}

impl Depth {
    pub fn channel_bytes(self) -> usize {
        match self {
            Depth::U8 => 1,
            Depth::U16 => 2,
            Depth::F32 => 4,
        }
    }
}

pub fn depth(layer: &ae::Layer) -> Result<Depth, ae::Error> {
    match layer.pixel_format()? {
        ae::PixelFormat::Argb32 => Ok(Depth::U8),
        ae::PixelFormat::Argb64 => Ok(Depth::U16),
        ae::PixelFormat::Argb128 => Ok(Depth::F32),
        _ => Err(ae::Error::BadCallbackParameter),
    }
}

/// Validated stride for a w x h world with signed `row_bytes`.
pub fn layout(
    width: usize,
    height: usize,
    row_bytes: isize,
    depth: Depth,
) -> Result<usize, ae::Error> {
    let bad = ae::Error::BadCallbackParameter;
    let stride = row_bytes.checked_abs().ok_or(bad)? as usize;
    let row = width.checked_mul(4 * depth.channel_bytes()).ok_or(bad)?;
    let extent = stride.checked_mul(height).ok_or(bad)?;
    if width > i32::MAX as usize
        || height > i32::MAX as usize
        || stride > i32::MAX as usize
        || extent > isize::MAX as usize
        || stride < row
        || stride == 0
    {
        return Err(bad);
    }
    Ok(stride)
}

/// Storage row of logical row `y` in `Layer::buffer()`, which starts at the
/// lowest address even for negative row_bytes
/// (vendor after-effects src/pf/layer.rs `buffer`).
#[inline]
pub fn storage_row(y: usize, height: usize, bottom_up: bool) -> usize {
    if bottom_up {
        height - 1 - y
    } else {
        y
    }
}

/// One ARGB pixel -> straight RGBA (AE-normalised).
#[inline]
pub fn decode(b: &[u8], depth: Depth) -> [f32; 4] {
    let ch = |i: usize| -> f32 {
        match depth {
            Depth::U8 => b[i] as f32 / 255.0,
            Depth::U16 => u16::from_ne_bytes([b[2 * i], b[2 * i + 1]]) as f32 / 32768.0,
            Depth::F32 => f32::from_ne_bytes([b[4 * i], b[4 * i + 1], b[4 * i + 2], b[4 * i + 3]]),
        }
    };
    [ch(1), ch(2), ch(3), ch(0)]
}

/// Straight RGBA -> one ARGB pixel. Integer depths round and clamp to the AE
/// range (16-bit white = 32768); float is written unmodified.
#[inline]
pub fn encode(rgba: [f32; 4], depth: Depth, b: &mut [u8]) {
    let argb = [rgba[3], rgba[0], rgba[1], rgba[2]];
    for (i, v) in argb.iter().enumerate() {
        match depth {
            Depth::U8 => {
                let q = if v.is_nan() {
                    0.0
                } else {
                    (v * 255.0).round().clamp(0.0, 255.0)
                };
                b[i] = q as u8;
            }
            Depth::U16 => {
                let q = if v.is_nan() {
                    0.0
                } else {
                    (v * 32768.0).round().clamp(0.0, 32768.0)
                };
                b[2 * i..2 * i + 2].copy_from_slice(&(q as u16).to_ne_bytes());
            }
            Depth::F32 => b[4 * i..4 * i + 4].copy_from_slice(&v.to_ne_bytes()),
        }
    }
}

type WorldLayout = (usize, usize, usize, Depth, bool);
fn checked_world(layer: &ae::Layer) -> Result<Option<WorldLayout>, ae::Error> {
    let (w, h) = (layer.width(), layer.height());
    if w == 0 || h == 0 {
        return Ok(None);
    }
    let d = depth(layer)?;
    // Layer::buffer() asserts non-null data and non-zero rowbytes; check first.
    if unsafe { layer.data_ptr() }.is_null() || layer.row_bytes() == 0 {
        return Err(ae::Error::BadCallbackParameter);
    }
    let stride = layout(w, h, layer.row_bytes(), d)?;
    Ok(Some((w, h, stride, d, layer.row_bytes() < 0)))
}

/// Reads a world positioned at (left, top) in layer coordinates.
pub fn read_image(layer: &ae::Layer, left: i64, top: i64) -> Result<Image, ae::Error> {
    let to_ae = |_: RenderError| ae::Error::BadCallbackParameter;
    let Some((w, h, stride, d, bottom_up)) = checked_world(layer)? else {
        return Image::new(0, 0, left, top, Vec::new()).map_err(to_ae);
    };
    let bpp = 4 * d.channel_bytes();
    let buf = layer.buffer();
    if buf.len() < stride * h {
        return Err(ae::Error::BadCallbackParameter);
    }
    let mut px = Vec::new();
    px.try_reserve_exact(w * h)
        .map_err(|_| ae::Error::OutOfMemory)?;
    for y in 0..h {
        let row = &buf[storage_row(y, h, bottom_up) * stride..][..w * bpp];
        px.extend(row.chunks_exact(bpp).map(|p| decode(p, d)));
    }
    Image::new(w, h, left, top, px).map_err(to_ae)
}

/// Writes w*h straight RGBA pixels into the world (padding untouched).
pub fn write_image(layer: &mut ae::Layer, px: &[[f32; 4]]) -> Result<(), ae::Error> {
    let Some((w, h, stride, d, bottom_up)) = checked_world(layer)? else {
        return Ok(());
    };
    if px.len() != w * h {
        return Err(ae::Error::BadCallbackParameter);
    }
    let bpp = 4 * d.channel_bytes();
    let buf = layer.buffer_mut();
    if buf.len() < stride * h {
        return Err(ae::Error::BadCallbackParameter);
    }
    for y in 0..h {
        let start = storage_row(y, h, bottom_up) * stride;
        let row = &mut buf[start..start + w * bpp];
        for (dst, p) in row.chunks_exact_mut(bpp).zip(&px[y * w..(y + 1) * w]) {
            encode(*p, d, dst);
        }
    }
    Ok(())
}

/// Transparent zero output (no input world available).
pub fn write_transparent(layer: &mut ae::Layer) -> Result<(), ae::Error> {
    let Some((w, h, stride, d, bottom_up)) = checked_world(layer)? else {
        return Ok(());
    };
    let bpp = 4 * d.channel_bytes();
    let buf = layer.buffer_mut();
    if buf.len() < stride * h {
        return Err(ae::Error::BadCallbackParameter);
    }
    for y in 0..h {
        let start = storage_row(y, h, bottom_up) * stride;
        buf[start..start + w * bpp].fill(0);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn argb_order_and_reads_are_independent_of_writes() {
        assert_eq!(
            decode(&[255, 0, 128, 51], Depth::U8),
            [0.0, 128.0 / 255.0, 0.2, 1.0]
        );
        let mut b = [0u8; 8];
        for (i, v) in [16384u16, 32768, 1, 0].iter().enumerate() {
            b[2 * i..2 * i + 2].copy_from_slice(&v.to_ne_bytes());
        }
        assert_eq!(decode(&b, Depth::U16), [1.0, 1.0 / 32768.0, 0.0, 0.5]);
        let mut out = [9u8; 4];
        encode([0.0, 0.5, 1.0, 0.25], Depth::U8, &mut out);
        assert_eq!(out, [64, 0, 128, 255]);
    }

    #[test]
    fn integer_codecs_are_exact_round_trips() {
        for v in 0..=255u8 {
            let mut b = [v, v, v, v];
            let p = decode(&b, Depth::U8);
            encode(p, Depth::U8, &mut b);
            assert_eq!(b, [v; 4]);
        }
        for v in 0..=32768u16 {
            let mut b = [0u8; 8];
            for i in 0..4 {
                b[2 * i..2 * i + 2].copy_from_slice(&v.to_ne_bytes());
            }
            let before = b;
            encode(decode(&b, Depth::U16), Depth::U16, &mut b);
            assert_eq!(b, before, "16-bit value {v}");
        }
        let mut w = [0u8; 8];
        encode([2.0, -1.0, 0.0, 1.5], Depth::U16, &mut w);
        let white = 32768u16.to_ne_bytes();
        assert_eq!(&w[..2], &white[..], "alpha clamps to 32768, not 65535");
        assert_eq!(&w[2..4], &white[..], "16-bit RGB clamps to 32768");
        assert_eq!(&w[4..6], &[0u8, 0][..]);
    }

    #[test]
    fn float_codec_keeps_hdr_negative_and_nan_bits() {
        let vals = [4.75f32, -3.5, f32::from_bits(0x7fc0_0001), 1e-40];
        let mut b = [0u8; 16];
        encode([vals[0], vals[1], vals[2], vals[3]], Depth::F32, &mut b);
        let p = decode(&b, Depth::F32);
        assert_eq!(p.map(f32::to_bits), vals.map(f32::to_bits));
    }

    #[test]
    fn layout_rejects_short_and_overflowing_strides() {
        for d in [Depth::U8, Depth::U16, Depth::F32] {
            let row = 17 * 4 * d.channel_bytes();
            assert_eq!(layout(17, 31, row as isize, d), Ok(row));
            assert_eq!(layout(17, 31, -((row + 12) as isize), d), Ok(row + 12));
            assert!(layout(17, 31, row as isize - 1, d).is_err());
        }
        assert!(layout(1, 1, isize::MIN, Depth::U8).is_err());
        assert!(layout(usize::MAX, 1, 4, Depth::U8).is_err());
        assert!(layout(1, 1, 0, Depth::U8).is_err());
        assert_eq!(
            (0..3).map(|y| storage_row(y, 3, true)).collect::<Vec<_>>(),
            [2, 1, 0]
        );
    }
}
