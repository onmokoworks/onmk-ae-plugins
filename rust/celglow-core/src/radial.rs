use crate::{field::Field, FrameBuf};

pub struct RadialField {
    center: (f32, f32),
    extent: (f32, f32),
    base: f32,
}
impl RadialField {
    pub fn build(input: FrameBuf<'_>, blur: &Field) -> Self {
        let (mut total, mut cx, mut cy) = (0.0, 0.0, 0.0);
        for y in 0..input.h {
            for x in 0..input.w {
                let a = input.rgba[(y * input.w + x) * 4 + 3];
                total += a;
                cx += x as f32 * a;
                cy += y as f32 * a;
            }
        }
        let center = if total > f32::EPSILON {
            (cx / total, cy / total)
        } else {
            (input.w as f32 * 0.5, input.h as f32 * 0.5)
        };
        let (mut ex, mut ey) = (1.0_f32, 1.0_f32);
        // The ring field is sized from a mid-value contour of the blurred
        // source, not from the hard alpha outline. Thus Source Spread is a
        // genuine "how far does the influence reach" control.
        for y in 0..input.h {
            for x in 0..input.w {
                if blur.normalized(blur.values[y * input.w + x]) >= 0.35 {
                    ex = ex.max((x as f32 - center.0).abs());
                    ey = ey.max((y as f32 - center.1).abs());
                }
            }
        }
        Self {
            center,
            extent: (ex, ey),
            base: ex.max(ey),
        }
    }
    pub fn radius(&self, x: f32, y: f32) -> f32 {
        let nx = (x - self.center.0) / self.extent.0;
        let ny = (y - self.center.1) / self.extent.1;
        (nx * nx + ny * ny).sqrt() * self.base
    }
    pub fn start_radius(&self) -> f32 {
        self.base
    }
}
