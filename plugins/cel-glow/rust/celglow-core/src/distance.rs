use crate::FrameBuf;

const INF: f32 = 1.0e20;
const DIAGONAL: f32 = std::f32::consts::SQRT_2;

pub struct DistanceField {
    w: usize,
    h: usize,
    values: Vec<f32>,
}

impl DistanceField {
    pub fn build(input: FrameBuf<'_>) -> Self {
        let mut values: Vec<f32> = input
            .rgba
            .chunks_exact(4)
            .map(|p| if p[3] > 1.0 / 255.0 { 0.0 } else { INF })
            .collect();
        for y in 0..input.h {
            for x in 0..input.w {
                let i = y * input.w + x;
                let mut d = values[i];
                if x > 0 {
                    d = d.min(values[i - 1] + 1.0)
                }
                if y > 0 {
                    d = d.min(values[i - input.w] + 1.0)
                }
                if x > 0 && y > 0 {
                    d = d.min(values[i - input.w - 1] + DIAGONAL)
                }
                if x + 1 < input.w && y > 0 {
                    d = d.min(values[i - input.w + 1] + DIAGONAL)
                }
                values[i] = d;
            }
        }
        for y in (0..input.h).rev() {
            for x in (0..input.w).rev() {
                let i = y * input.w + x;
                let mut d = values[i];
                if x + 1 < input.w {
                    d = d.min(values[i + 1] + 1.0)
                }
                if y + 1 < input.h {
                    d = d.min(values[i + input.w] + 1.0)
                }
                if x + 1 < input.w && y + 1 < input.h {
                    d = d.min(values[i + input.w + 1] + DIAGONAL)
                }
                if x > 0 && y + 1 < input.h {
                    d = d.min(values[i + input.w - 1] + DIAGONAL)
                }
                values[i] = d;
            }
        }
        Self {
            w: input.w,
            h: input.h,
            values,
        }
    }
    pub fn sample(&self, x: f32, y: f32) -> f32 {
        let xi = x.round() as isize;
        let yi = y.round() as isize;
        if xi < 0 || yi < 0 || xi >= self.w as isize || yi >= self.h as isize {
            INF
        } else {
            self.values[yi as usize * self.w + xi as usize]
        }
    }

    pub fn blurred(&self, radius: usize) -> Self {
        let mut horizontal = vec![0.0; self.values.len()];
        for y in 0..self.h {
            for x in 0..self.w {
                let lo = x.saturating_sub(radius);
                let hi = (x + radius).min(self.w - 1);
                horizontal[y * self.w + x] = (lo..=hi)
                    .map(|xx| self.values[y * self.w + xx])
                    .sum::<f32>()
                    / (hi - lo + 1) as f32;
            }
        }
        let mut values = vec![0.0; self.values.len()];
        for y in 0..self.h {
            for x in 0..self.w {
                let lo = y.saturating_sub(radius);
                let hi = (y + radius).min(self.h - 1);
                values[y * self.w + x] =
                    (lo..=hi).map(|yy| horizontal[yy * self.w + x]).sum::<f32>()
                        / (hi - lo + 1) as f32;
            }
        }
        // The blur may carry positive distance back across the silhouette.
        // Restore the original source interior so glow remains external.
        for (index, original) in self.values.iter().enumerate() {
            if *original == 0.0 {
                values[index] = 0.0;
            }
        }
        Self {
            w: self.w,
            h: self.h,
            values,
        }
    }
}
