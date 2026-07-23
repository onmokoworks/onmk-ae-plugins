use crate::params::{Params, SourceChannel};
use crate::FrameBuf;

/// S1 のスカラー場。row-major、各要素は 0..=1。
pub struct Field {
    pub w: usize,
    pub h: usize,
    pub values: Vec<f32>,
    pub peak: f32,
}

impl Field {
    pub fn build(input: FrameBuf<'_>, params: &Params) -> Self {
        let mut values = Vec::with_capacity(input.w * input.h);
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

        let radius = (params.source.spread / 3.0).round() as usize;
        for _ in 0..3 {
            values = blur_horizontal(&values, input.w, input.h, radius);
            values = blur_vertical(&values, input.w, input.h, radius);
        }
        for value in &mut values {
            *value = value.clamp(0.0, 1.0).powf(params.source.field_gamma);
        }
        let peak = values.iter().copied().fold(0.0_f32, f32::max);
        Self {
            w: input.w,
            h: input.h,
            values,
            peak,
        }
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

fn blur_horizontal(source: &[f32], w: usize, h: usize, radius: usize) -> Vec<f32> {
    if radius == 0 {
        return source.to_vec();
    }
    let width = (radius * 2 + 1) as f32;
    let mut output = vec![0.0; source.len()];
    for y in 0..h {
        let row = y * w;
        let mut sum: f32 = source[row..row + (radius + 1).min(w)].iter().sum();
        for x in 0..w {
            output[row + x] = sum / width;
            if x >= radius {
                sum -= source[row + x - radius];
            }
            if x + radius + 1 < w {
                sum += source[row + x + radius + 1];
            }
        }
    }
    output
}

fn blur_vertical(source: &[f32], w: usize, h: usize, radius: usize) -> Vec<f32> {
    if radius == 0 {
        return source.to_vec();
    }
    let width = (radius * 2 + 1) as f32;
    let mut output = vec![0.0; source.len()];
    for x in 0..w {
        let mut sum = (0..(radius + 1).min(h))
            .map(|y| source[y * w + x])
            .sum::<f32>();
        for y in 0..h {
            output[y * w + x] = sum / width;
            if y >= radius {
                sum -= source[(y - radius) * w + x];
            }
            if y + radius + 1 < h {
                sum += source[(y + radius + 1) * w + x];
            }
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

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
        );
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
        let field = Field::build(FrameBuf { w: 2, h: 1, rgba: &input }, &params);
        assert_eq!(field.values[0], 0.0);
        assert!(field.values[1] > 0.0);
    }
}
