use glam::Vec3;
use crate::renderer::{blend_pixel, smooth_falloff, project_point_3d, BlendMode, CameraProjection};

pub struct PlexusRenderConfig {
    pub width: usize,
    pub height: usize,
    pub row_stride: usize,
    pub origin_x: f32,
    pub origin_y: f32,
    pub point_size: f32,
    pub point_color: [f32; 4],
    pub blend_mode: BlendMode,
    pub camera_projection: Option<CameraProjection>,
}

impl PlexusRenderConfig {
    fn row_bytes(&self) -> usize {
        if self.row_stride > 0 { self.row_stride } else { self.width * 4 }
    }
}

pub fn render_plexus_points_8bit(
    points: &[Vec3],
    config: &PlexusRenderConfig,
    output: &mut [u8],
) {
    for &pos in points {
        draw_point_dot(pos, config, output);
    }
}

fn draw_point_dot(pos: Vec3, config: &PlexusRenderConfig, output: &mut [u8]) {
    let (local_x, local_y, perspective_scale) = if let Some(ref projection) = config.camera_projection {
        match project_point_3d(pos, projection) {
            Some((sx, sy, depth)) => {
                let lx = sx - config.origin_x;
                let ly = sy - config.origin_y;
                let scale = (projection.image_plane_dist as f32 / depth.max(1.0)).clamp(0.05, 10.0);
                (lx, ly, scale)
            }
            None => return,
        }
    } else {
        let raw_x = pos.x - config.origin_x;
        let raw_y = pos.y - config.origin_y;
        if pos.z.abs() > 0.1 {
            let cam_dist = (config.width as f32).max(config.height as f32) * 1.5;
            let scale = (cam_dist / (cam_dist + pos.z)).clamp(0.05, 10.0);
            let cx = config.width as f32 * 0.5;
            let cy = config.height as f32 * 0.5;
            ((raw_x - cx) * scale + cx, (raw_y - cy) * scale + cy, scale)
        } else {
            (raw_x, raw_y, 1.0)
        }
    };

    let radius = (config.point_size * perspective_scale * 0.5).max(0.5);

    let min_x = ((local_x - radius).floor() as isize).max(0) as usize;
    let max_x = ((local_x + radius).ceil() as isize).min(config.width.saturating_sub(1) as isize) as usize;
    let min_y = ((local_y - radius).floor() as isize).max(0) as usize;
    let max_y = ((local_y + radius).ceil() as isize).min(config.height.saturating_sub(1) as isize) as usize;

    if min_x > max_x || min_y > max_y {
        return;
    }

    let color = config.point_color;

    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let dx = x as f32 + 0.5 - local_x;
            let dy = y as f32 + 0.5 - local_y;
            let dist = (dx * dx + dy * dy).sqrt();
            let coverage = smooth_falloff(1.0 - dist / radius.max(1.0));
            if coverage <= 0.0 {
                continue;
            }
            blend_pixel(
                output,
                config.row_bytes(),
                x,
                y,
                color,
                (coverage * color[3]).clamp(0.0, 1.0),
                config.blend_mode,
            );
        }
    }
}
