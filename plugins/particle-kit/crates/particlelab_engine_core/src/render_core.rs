#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RenderSurface {
    pub width: usize,
    pub height: usize,
    pub row_bytes: usize,
    pub len_bytes: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RenderSurfaceError {
    RowStrideTooSmall,
    Overflow,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ArgbBlendMode {
    Normal,
    Add,
    Screen,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RenderFrame {
    pub surface: RenderSurface,
    pub origin_x: i32,
    pub origin_y: i32,
    pub time: f32,
    pub dt: f32,
}

impl RenderSurface {
    pub const ARGB8_BYTES_PER_PIXEL: usize = 4;

    pub fn argb8(width: usize, height: usize) -> Result<Self, RenderSurfaceError> {
        let row_bytes = width
            .checked_mul(Self::ARGB8_BYTES_PER_PIXEL)
            .ok_or(RenderSurfaceError::Overflow)?;
        Self::argb8_with_row_bytes(width, height, row_bytes)
    }

    pub fn argb8_with_row_bytes(
        width: usize,
        height: usize,
        row_bytes: usize,
    ) -> Result<Self, RenderSurfaceError> {
        let min_row_bytes = width
            .checked_mul(Self::ARGB8_BYTES_PER_PIXEL)
            .ok_or(RenderSurfaceError::Overflow)?;
        if row_bytes < min_row_bytes {
            return Err(RenderSurfaceError::RowStrideTooSmall);
        }
        let len_bytes = row_bytes
            .checked_mul(height)
            .ok_or(RenderSurfaceError::Overflow)?;
        Ok(Self {
            width,
            height,
            row_bytes,
            len_bytes,
        })
    }

    pub fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0
    }

    pub fn fits_buffer(self, output: &[u8]) -> bool {
        output.len() >= self.len_bytes
    }
}

impl RenderFrame {
    pub fn argb8(
        width: usize,
        height: usize,
        origin_x: i32,
        origin_y: i32,
        time: f32,
        dt: f32,
    ) -> Result<Self, RenderSurfaceError> {
        let row_bytes = width
            .checked_mul(RenderSurface::ARGB8_BYTES_PER_PIXEL)
            .ok_or(RenderSurfaceError::Overflow)?;
        Self::argb8_with_row_bytes(width, height, row_bytes, origin_x, origin_y, time, dt)
    }

    pub fn argb8_with_row_bytes(
        width: usize,
        height: usize,
        row_bytes: usize,
        origin_x: i32,
        origin_y: i32,
        time: f32,
        dt: f32,
    ) -> Result<Self, RenderSurfaceError> {
        Ok(Self {
            surface: RenderSurface::argb8_with_row_bytes(width, height, row_bytes)?,
            origin_x,
            origin_y,
            time,
            dt,
        })
    }
}

pub fn blend_argb8_pixel(
    output: &mut [u8],
    row_bytes: usize,
    x: usize,
    y: usize,
    color: [f32; 4],
    coverage: f32,
    blend_mode: ArgbBlendMode,
) {
    let Some(idx) = y.checked_mul(row_bytes).and_then(|row| {
        x.checked_mul(RenderSurface::ARGB8_BYTES_PER_PIXEL)
            .and_then(|col| row.checked_add(col))
    }) else {
        return;
    };
    if idx + 3 >= output.len() {
        return;
    }

    let src_a = (color[3] * coverage).clamp(0.0, 1.0);
    let src_rgb = [
        color[0].clamp(0.0, 1.0),
        color[1].clamp(0.0, 1.0),
        color[2].clamp(0.0, 1.0),
    ];
    let dst_rgb = [
        output[idx + 1] as f32 / 255.0,
        output[idx + 2] as f32 / 255.0,
        output[idx + 3] as f32 / 255.0,
    ];
    let dst_a = output[idx] as f32 / 255.0;

    let blended = match blend_mode {
        ArgbBlendMode::Normal => [
            dst_rgb[0] * (1.0 - src_a) + src_rgb[0] * src_a,
            dst_rgb[1] * (1.0 - src_a) + src_rgb[1] * src_a,
            dst_rgb[2] * (1.0 - src_a) + src_rgb[2] * src_a,
        ],
        ArgbBlendMode::Add => [
            (dst_rgb[0] + src_rgb[0] * src_a * 1.2).clamp(0.0, 1.0),
            (dst_rgb[1] + src_rgb[1] * src_a * 1.2).clamp(0.0, 1.0),
            (dst_rgb[2] + src_rgb[2] * src_a * 1.2).clamp(0.0, 1.0),
        ],
        ArgbBlendMode::Screen => [
            1.0 - (1.0 - dst_rgb[0]) * (1.0 - src_rgb[0] * src_a * 0.9),
            1.0 - (1.0 - dst_rgb[1]) * (1.0 - src_rgb[1] * src_a * 0.9),
            1.0 - (1.0 - dst_rgb[2]) * (1.0 - src_rgb[2] * src_a * 0.9),
        ],
    };

    let out_a = (dst_a + src_a * (1.0 - dst_a)).clamp(0.0, 1.0);
    output[idx] = (out_a * 255.0).round() as u8;
    output[idx + 1] = (blended[0] * 255.0).round() as u8;
    output[idx + 2] = (blended[1] * 255.0).round() as u8;
    output[idx + 3] = (blended[2] * 255.0).round() as u8;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn argb8_surface_computes_packed_stride_and_len() {
        let surface = RenderSurface::argb8(320, 180).unwrap();
        assert_eq!(
            surface,
            RenderSurface {
                width: 320,
                height: 180,
                row_bytes: 1_280,
                len_bytes: 230_400,
            }
        );
    }

    #[test]
    fn argb8_surface_accepts_larger_row_stride() {
        let surface = RenderSurface::argb8_with_row_bytes(10, 3, 64).unwrap();
        assert_eq!(surface.row_bytes, 64);
        assert_eq!(surface.len_bytes, 192);
    }

    #[test]
    fn argb8_surface_rejects_too_small_row_stride() {
        assert_eq!(
            RenderSurface::argb8_with_row_bytes(10, 3, 39),
            Err(RenderSurfaceError::RowStrideTooSmall)
        );
    }

    #[test]
    fn argb8_surface_reports_overflow() {
        assert_eq!(
            RenderSurface::argb8(usize::MAX / 4 + 1, 1),
            Err(RenderSurfaceError::Overflow)
        );
        assert_eq!(
            RenderSurface::argb8_with_row_bytes(1, usize::MAX / 4 + 1, 4),
            Err(RenderSurfaceError::Overflow)
        );
    }

    #[test]
    fn argb8_surface_checks_output_capacity() {
        let surface = RenderSurface::argb8(4, 4).unwrap();
        assert!(surface.fits_buffer(&vec![0; 64]));
        assert!(!surface.fits_buffer(&vec![0; 63]));
    }

    #[test]
    fn argb8_frame_carries_surface_and_timing_without_host_types() {
        let frame = RenderFrame::argb8(1920, 1080, -12, 34, 1.25, 1.0 / 24.0).unwrap();
        assert_eq!(frame.surface.width, 1920);
        assert_eq!(frame.surface.height, 1080);
        assert_eq!(frame.origin_x, -12);
        assert_eq!(frame.origin_y, 34);
        assert_eq!(frame.time, 1.25);
        assert_eq!(frame.dt, 1.0 / 24.0);
    }

    #[test]
    fn argb8_frame_accepts_host_row_stride() {
        let frame = RenderFrame::argb8_with_row_bytes(10, 4, 64, 1, -2, 0.5, 1.0 / 60.0).unwrap();
        assert_eq!(frame.surface.width, 10);
        assert_eq!(frame.surface.height, 4);
        assert_eq!(frame.surface.row_bytes, 64);
        assert_eq!(frame.surface.len_bytes, 256);
        assert_eq!(frame.origin_x, 1);
        assert_eq!(frame.origin_y, -2);
    }

    #[test]
    fn blend_argb8_pixel_supports_normal_add_and_screen() {
        let mut normal = vec![0, 0, 0, 0];
        blend_argb8_pixel(
            &mut normal,
            4,
            0,
            0,
            [1.0, 0.5, 0.0, 0.5],
            1.0,
            ArgbBlendMode::Normal,
        );
        assert_eq!(normal[0], 128);
        assert!(normal[1] > normal[2]);
        assert_eq!(normal[3], 0);

        let mut add = vec![255, 100, 100, 100];
        blend_argb8_pixel(
            &mut add,
            4,
            0,
            0,
            [1.0, 1.0, 1.0, 1.0],
            1.0,
            ArgbBlendMode::Add,
        );
        assert_eq!(&add[1..4], &[255, 255, 255]);

        let mut screen = vec![255, 0, 0, 0];
        blend_argb8_pixel(
            &mut screen,
            4,
            0,
            0,
            [1.0, 1.0, 1.0, 1.0],
            1.0,
            ArgbBlendMode::Screen,
        );
        assert!(screen[1] > 0 && screen[2] > 0 && screen[3] > 0);
    }
}
