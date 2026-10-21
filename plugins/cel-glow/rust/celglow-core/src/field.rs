use crate::params::{Params, SourceChannel};
use crate::{FrameBuf, RenderError};

/// S1 のスカラー場。row-major、各要素は 0..=1。
pub struct Field {
    pub w: usize,
    pub h: usize,
    pub values: Vec<f32>,
    pub peak: f32,
}

impl Field {
    pub fn build(input: FrameBuf<'_>, params: &Params) -> Result<Self, RenderError> {
        let count = input.w.checked_mul(input.h).ok_or(RenderError::FieldDimensions)?;
        let expected = count.checked_mul(4).ok_or(RenderError::FieldDimensions)?;
        if input.w > isize::MAX as usize || input.h > isize::MAX as usize {
            return Err(RenderError::FieldDimensions);
        }
        if input.rgba.len() != expected {
            return Err(RenderError::InputLength {actual:input.rgba.len(),expected});
        }
        let spread = params.source.spread;
        if !spread.is_finite() || spread < 0.0 { return Err(RenderError::FieldRadius); }
        let radius = (spread / 3.0).round() as usize;
        radius.checked_mul(2).and_then(|n| n.checked_add(1)).ok_or(RenderError::FieldRadius)?;
        let mut values = Vec::new();
        values.try_reserve_exact(count).map_err(|_| RenderError::FieldAllocation)?;
        for pixel in input.rgba.chunks_exact(4) {
            let [r, g, b, a] = [pixel[0], pixel[1], pixel[2], pixel[3]];
            let luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;
            let strength = match params.source.channel {
                SourceChannel::Alpha => a,
                SourceChannel::Luma => luma,
                SourceChannel::LumaXAlpha => luma * a,
            };
            let strength = (strength * (params.source.gain / 100.0))
                .clamp(0.0, 1.0)
                .powf(params.source.gamma);
            let threshold = (params.source.threshold / 100.0).clamp(0.0, 1.0);
            let softness = (params.source.threshold_softness / 100.0).max(0.0);
            let gate = if threshold <= f32::EPSILON {
                1.0
            } else if softness <= f32::EPSILON {
                if strength >= threshold { 1.0 } else { 0.0 }
            } else {
                let t = ((strength - threshold) / softness).clamp(0.0, 1.0);
                t * t * (3.0 - 2.0 * t)
            };
            values.push(strength * gate);
        }

        for _ in 0..3 {
            values = blur_horizontal(&values, input.w, input.h, radius)?;
            values = blur_vertical(&values, input.w, input.h, radius)?;
        }
        for value in &mut values {
            *value = value.clamp(0.0, 1.0).powf(params.source.field_gamma);
        }
        let peak = values.iter().copied().fold(0.0_f32, f32::max);
        Ok(Self {
            w: input.w,
            h: input.h,
            values,
            peak,
        })
    }

    /// Bilinear sample with clamp-to-zero outside the supplied input rect.
    pub fn sample(&self, x: f32, y: f32) -> f32 {
        let x0 = x.floor() as isize;
        let y0 = y.floor() as isize;
        let tx = x - x0 as f32;
        let ty = y - y0 as f32;
        let get = |x: isize, y: isize| -> f32 {
            if x < 0 || y < 0 || x >= self.w as isize || y >= self.h as isize {
                0.0
            } else {
                self.values[y as usize * self.w + x as usize]
            }
        };
        let top = get(x0, y0) + (get(x0 + 1, y0) - get(x0, y0)) * tx;
        let bottom = get(x0, y0 + 1) + (get(x0 + 1, y0 + 1) - get(x0, y0 + 1)) * tx;
        top + (bottom - top) * ty
    }

    pub fn gradient_magnitude(&self, x: f32, y: f32) -> f32 {
        let dx = (self.sample(x + 1.0, y) - self.sample(x - 1.0, y)) * 0.5;
        let dy = (self.sample(x, y + 1.0) - self.sample(x, y - 1.0)) * 0.5;
        dx.hypot(dy)
    }

    /// Curves の入力白点を現在のぼかし場の最大値へ合わせる。
    pub fn normalized(&self, value: f32) -> f32 {
        if self.peak <= f32::EPSILON {
            0.0
        } else {
            (value / self.peak).clamp(0.0, 1.0)
        }
    }
}

pub(crate) fn zero_plane(count: usize) -> Result<Vec<f32>, RenderError> {
    let mut values = Vec::new();
    values.try_reserve_exact(count).map_err(|_| RenderError::FieldAllocation)?;
    values.resize(count,0.0);
    Ok(values)
}

fn blur_horizontal(source: &[f32], w: usize, h: usize, radius: usize) -> Result<Vec<f32>,RenderError> {
    let mut output = zero_plane(source.len())?;
    if radius == 0 {
        output.copy_from_slice(source);
        return Ok(output);
    }
    let width = (radius * 2 + 1) as f32;
    for y in 0..h {
        let row = y * w;
        let mut sum: f32 = source[row..row + (radius + 1).min(w)].iter().sum();
        for x in 0..w {
            output[row + x] = sum / width;
            if x >= radius {
                sum -= source[row + x - radius];
            }
            let next = x.saturating_add(radius).saturating_add(1);
            if next < w {
                sum += source[row + next];
            }
        }
    }
    Ok(output)
}

fn blur_vertical(source: &[f32], w: usize, h: usize, radius: usize) -> Result<Vec<f32>,RenderError> {
    let mut output = zero_plane(source.len())?;
    if radius == 0 {
        output.copy_from_slice(source);
        return Ok(output);
    }
    let width = (radius * 2 + 1) as f32;
    for x in 0..w {
        let mut sum = (0..(radius + 1).min(h))
            .map(|y| source[y * w + x])
            .sum::<f32>();
        for y in 0..h {
            output[y * w + x] = sum / width;
            if y >= radius {
                sum -= source[(y - radius) * w + x];
            }
            let next = y.saturating_add(radius).saturating_add(1);
            if next < h {
                sum += source[next * w + x];
            }
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_rejects_bad_geometry_radius_and_capacity() {
        let mut params = Params::default();
        assert!(matches!(Field::build(FrameBuf {w:usize::MAX,h:2,rgba:&[]},&params),Err(RenderError::FieldDimensions)));
        assert!(matches!(Field::build(FrameBuf {w:1,h:1,rgba:&[0.0;3]},&params),Err(RenderError::InputLength {..})));
        assert!(matches!(zero_plane(usize::MAX),Err(RenderError::FieldAllocation)));
        for spread in [-1.0,f32::NAN,f32::INFINITY,f32::MAX] {
            params.source.spread = spread;
            let input = FrameBuf {w:1,h:1,rgba:&[0.0;4]};
            assert!(matches!(Field::build(input,&params),Err(RenderError::FieldRadius)));
            let mut output = [19.0;4];
            assert_eq!(crate::render(input,(0,0),(0,0),(1,1),&params,crate::DebugView::Result,&mut output),Err(RenderError::FieldRadius));
            assert_eq!(output,[19.0;4]);
        }
        params.source.spread = 3.0;
        assert!(Field::build(FrameBuf {w:0,h:7,rgba:&[]},&params).unwrap().values.is_empty());
    }

    #[test]
    fn fallible_blur_matches_direct_zero_padded_average() {
        for (w,h) in [(1,1),(1,7),(9,1),(7,9)] {
            let source:Vec<f32>=(0..w*h).map(|i|((i*17)%31) as f32/31.0).collect();
            for radius in [0,1,3,100] {
                let horizontal=blur_horizontal(&source,w,h,radius).unwrap();
                let vertical=blur_vertical(&source,w,h,radius).unwrap();
                for y in 0..h {for x in 0..w {
                    let hx:f32=(x.saturating_sub(radius)..(x+radius+1).min(w)).map(|xx|source[y*w+xx]).sum::<f32>()/(2*radius+1) as f32;
                    let vy:f32=(y.saturating_sub(radius)..(y+radius+1).min(h)).map(|yy|source[yy*w+x]).sum::<f32>()/(2*radius+1) as f32;
                    assert!((horizontal[y*w+x]-hx).abs()<1e-6);
                    assert!((vertical[y*w+x]-vy).abs()<1e-6);
                }}
            }
        }
    }

    #[test]
    fn blur_is_symmetric_and_zero_padded() {
        let input = [0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0];
        let mut params = Params::default();
        params.source.channel = SourceChannel::Alpha;
        params.source.spread = 3.0;
        let field = Field::build(
            FrameBuf {
                w: 3,
                h: 1,
                rgba: &input,
            },
            &params,
        ).unwrap();
        assert!((field.values[0] - field.values[2]).abs() < 1e-6);
        assert!(field.values[1] > field.values[0]);
    }

    #[test]
    fn threshold_gates_source_before_blur() {
        let input = [0.0, 0.0, 0.0, 0.25, 0.0, 0.0, 0.0, 0.75];
        let mut params = Params::default();
        params.source.channel = SourceChannel::Alpha;
        params.source.spread = 0.0;
        params.source.threshold = 50.0;
        let field = Field::build(FrameBuf { w: 2, h: 1, rgba: &input }, &params).unwrap();
        assert_eq!(field.values[0], 0.0);
        assert!(field.values[1] > 0.0);
    }
}
