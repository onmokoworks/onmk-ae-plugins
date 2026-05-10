use crate::particle::Particle;
use glam::{DMat4, DVec4};
use std::sync::Arc;
use std::time::Instant;

const MAX_MOTION_BLUR_SAMPLES: usize = 8;
const MAX_PARTICLE_PIXEL_BUDGET: usize = 4_000_000;

#[derive(Clone, Copy, Debug)]
pub enum ParticleShape {
    Circle,
    Square,
    Triangle,
    Star,
    Line,
    Image,
}

#[derive(Clone, Copy, Debug)]
pub enum BlendMode {
    Normal,
    Add,
    Screen,
}

#[derive(Clone, Debug)]
pub struct MipLevel {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct SpriteImage {
    pub width: usize,
    pub height: usize,
    pub pixels: Arc<Vec<u8>>,
    pub mips: Arc<Vec<MipLevel>>,
}

#[derive(Clone, Copy, Debug)]
pub enum ImageColorMode {
    Tint,
    Source,
}

#[derive(Clone, Copy, Debug)]
pub enum ImageFitMode {
    Contain,
    Stretch,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum TimeSamplingMode {
    CurrentTime,
    BirthTime,
    RandomStill,
    RandomPlay,
    Cycle,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ApplyMode {
    OnTransparent,
    Normal,
    Add,
    Screen,
}

#[derive(Clone, Copy, Debug)]
pub struct ImageSamplingConfig {
    pub use_source_alpha: bool,
    pub source_premultiplied: bool,
    pub alpha_clip: f32,
}

#[derive(Clone, Debug)]
pub struct RenderConfig {
    pub width: usize,
    pub height: usize,
    pub row_stride: usize, // bytes per row (0 = width*4)
    pub origin_x: f32,
    pub origin_y: f32,
    pub frame_dt: f32,
    pub shape: ParticleShape,
    pub blend_mode: BlendMode,
    pub motion_blur: f32,
    pub edge_softness: f32,
    pub dof_enabled: bool,
    pub dof_focal_distance: f32,
    pub dof_aperture: f32,
    pub composite_on_original: bool,
    pub apply_mode: ApplyMode,
    pub size_multiplier: f32,
    pub sprite_images: Vec<SpriteImage>,
    pub time_sampling: TimeSamplingMode,
    pub image_color_mode: ImageColorMode,
    pub image_fit_mode: ImageFitMode,
    pub image_sampling: ImageSamplingConfig,
    pub camera_projection: Option<CameraProjection>,
}

impl RenderConfig {
    pub fn row_bytes(&self) -> usize {
        if self.row_stride > 0 {
            self.row_stride
        } else {
            self.width * 4
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct CameraProjection {
    pub matrix: [[f64; 4]; 4],
    pub invert_matrix: bool,
    pub image_plane_dist: f64,
    pub image_plane_width: f32,
    pub image_plane_height: f32,
}

pub(crate) fn project_point_3d(
    pos: glam::Vec3,
    projection: &CameraProjection,
) -> Option<(f32, f32, f32)> {
    let matrix = if projection.invert_matrix {
        invert_camera_matrix(projection.matrix)?
    } else {
        projection.matrix
    };
    let m = &matrix;
    let point = DVec4::new(pos.x as f64, pos.y as f64, pos.z as f64, 1.0);
    let eye = DVec4::new(
        m[0][0] * point.x + m[0][1] * point.y + m[0][2] * point.z + m[0][3] * point.w,
        m[1][0] * point.x + m[1][1] * point.y + m[1][2] * point.z + m[1][3] * point.w,
        m[2][0] * point.x + m[2][1] * point.y + m[2][2] * point.z + m[2][3] * point.w,
        m[3][0] * point.x + m[3][1] * point.y + m[3][2] * point.z + m[3][3] * point.w,
    );
    if eye.w.abs() < 1e-10 {
        return None;
    }
    let eye_x = eye.x / eye.w;
    let eye_y = eye.y / eye.w;
    let eye_z = eye.z / eye.w;
    let depth = eye_z;
    if depth <= 1.0 {
        return None;
    }
    let dist = projection.image_plane_dist.max(1e-4);
    let screen_x = (eye_x * dist / depth) as f32 + projection.image_plane_width * 0.5;
    let screen_y = (eye_y * dist / depth) as f32 + projection.image_plane_height * 0.5;
    let limit = (projection
        .image_plane_width
        .max(projection.image_plane_height))
        * 4.0;
    if screen_x.abs() > limit || screen_y.abs() > limit {
        return None;
    }
    if !screen_x.is_finite() || !screen_y.is_finite() {
        return None;
    }
    Some((screen_x, screen_y, depth as f32))
}

fn transform_particle_3d(p: &Particle, projection: &CameraProjection) -> Option<(f32, f32, f32)> {
    let view = if projection.invert_matrix {
        invert_camera_matrix(projection.matrix)?
    } else {
        projection.matrix
    };

    let m = &view;
    let px = p.position.x as f64;
    let py = p.position.y as f64;
    let pz = p.position.z as f64;
    let eye_x = m[0][0] * px + m[0][1] * py + m[0][2] * pz + m[0][3];
    let eye_y = m[1][0] * px + m[1][1] * py + m[1][2] * pz + m[1][3];
    let eye_z = m[2][0] * px + m[2][1] * py + m[2][2] * pz + m[2][3];

    // AE: Z positive = into screen. Objects in front of camera have positive eye_z.
    let depth = eye_z;
    if depth <= 1.0 {
        return None; // Behind camera or too close — skip
    }

    let dist = projection.image_plane_dist.max(1e-4);
    let proj_scale = dist / depth;
    let screen_x = (eye_x * proj_scale) as f32 + projection.image_plane_width * 0.5;
    let screen_y = (eye_y * proj_scale) as f32 + projection.image_plane_height * 0.5;

    // Safety: reject extreme screen positions to prevent huge draw rects
    let limit = (projection
        .image_plane_width
        .max(projection.image_plane_height))
        * 4.0;
    if !screen_x.is_finite()
        || !screen_y.is_finite()
        || screen_x.abs() > limit
        || screen_y.abs() > limit
    {
        return None;
    }
    Some((screen_x, screen_y, depth as f32))
}

pub fn render_particles_8bit(
    particles: &[Particle],
    config: &RenderConfig,
    output: &mut [u8],
    deadline: Option<Instant>,
) {
    // Sort particles back-to-front by z-depth for correct alpha compositing
    let mut sorted_indices: Vec<usize> = (0..particles.len()).collect();
    sorted_indices.sort_unstable_by(|&a, &b| {
        let za = particles[a].position.z;
        let zb = particles[b].position.z;
        // Back-to-front: larger z (farther) first
        zb.partial_cmp(&za).unwrap_or(std::cmp::Ordering::Equal)
    });

    for (i, &idx) in sorted_indices.iter().enumerate() {
        // Check time budget every 16 particles. User-initiated aborts are
        // caught at the SmartRender level via `in_data.interact().abort()`;
        // we intentionally do NOT carry a global cancel flag here because
        // param-change-triggered cancellation caused AE's frame cache to be
        // poisoned with unwritten output buffers.
        if i & 15 == 15 {
            if let Some(dl) = deadline {
                if Instant::now() > dl {
                    break;
                }
            }
        }
        draw_particle(&particles[idx], config, output);
    }
}

fn draw_particle(p: &Particle, config: &RenderConfig, output: &mut [u8]) {
    let (local_x, local_y, perspective_scale) = if let Some(ref projection) =
        config.camera_projection
    {
        match transform_particle_3d(p, projection) {
            Some((sx, sy, depth)) => {
                let lx = sx - config.origin_x;
                let ly = sy - config.origin_y;
                let scale = (projection.image_plane_dist as f32 / depth.max(1.0)).clamp(0.05, 10.0);
                (lx, ly, scale)
            }
            None => {
                // Behind camera or degenerate — skip this particle
                return;
            }
        }
    } else {
        // No camera: use XY position directly, ignore Z (matches AE behavior)
        let raw_x = p.position.x - config.origin_x;
        let raw_y = p.position.y - config.origin_y;
        (raw_x, raw_y, 1.0)
    };
    let motion_blur_amount = config.motion_blur.clamp(0.0, 1.0);
    let blur_scale = 1.0 + motion_blur_amount * 0.5;
    let dof_scale = if config.dof_enabled {
        let depth_delta = (p.position.z - config.dof_focal_distance).abs();
        1.0 + depth_delta / (config.dof_aperture.max(0.001) * 100.0)
    } else {
        1.0
    };
    let radius =
        (p.size * config.size_multiplier * blur_scale * dof_scale * perspective_scale * 0.5)
            .max(0.5);
    let blur_offset = p.velocity * config.frame_dt * motion_blur_amount;
    let blur_len = blur_offset.length() * perspective_scale;
    let blur_samples = if blur_len > 0.5 {
        (blur_len / 4.0).ceil() as usize
    } else {
        1
    }
    .clamp(1, MAX_MOTION_BLUR_SAMPLES);
    let sample_weight = 1.0 / blur_samples as f32;

    if matches!(config.shape, ParticleShape::Image) {
        for sample_index in 0..blur_samples {
            let blur_t = sample_t(sample_index, blur_samples);
            let sample_x = local_x - blur_offset.x * (blur_t - 0.5);
            let sample_y = local_y - blur_offset.y * (blur_t - 0.5);
            draw_image_particle(p, config, output, sample_x, sample_y, radius, sample_weight);
        }
        return;
    }

    let feather_extent = radius * config.edge_softness.clamp(0.0, 1.0);
    let draw_radius = radius + feather_extent;

    // NOTE: clamp the signed (isize) bounds to the viewport BEFORE casting to
    // usize. Particles that live far to the left of / above the output rect
    // (easy to produce with a large Emitter Size and small comp) produce
    // negative `max_x` / `max_y` values; a direct `as usize` cast on a
    // negative isize wraps to a huge unsigned value, and the subsequent
    // `for x in min_x..=max_x` loop would run for billions of iterations —
    // stalling SmartRender, tripping the panic boundary / deadline, and
    // leaving AE to cache an unwritten output buffer.
    let w = config.width as isize;
    let h = config.height as isize;
    if w <= 0 || h <= 0 {
        return;
    }
    let min_xi = (local_x - draw_radius).floor() as isize;
    let max_xi = (local_x + draw_radius).ceil() as isize;
    let min_yi = (local_y - draw_radius).floor() as isize;
    let max_yi = (local_y + draw_radius).ceil() as isize;

    // Entirely outside the viewport — nothing to draw.
    if max_xi < 0 || max_yi < 0 || min_xi >= w || min_yi >= h {
        return;
    }

    let min_x = min_xi.max(0) as usize;
    let max_x = max_xi.min(w - 1).max(0) as usize;
    let min_y = min_yi.max(0) as usize;
    let max_y = max_yi.min(h - 1).max(0) as usize;

    if min_x > max_x || min_y > max_y {
        return;
    }

    let pixel_area = (max_x - min_x + 1) * (max_y - min_y + 1);
    if pixel_area > MAX_PARTICLE_PIXEL_BUDGET {
        return;
    }

    let feather = radius.max(1.0);
    for y in min_y..=max_y {
        for x in min_x..=max_x {
            let coverage = shape_coverage(
                config.shape,
                p,
                config,
                local_x,
                local_y,
                x as f32 + 0.5,
                y as f32 + 0.5,
                radius,
                feather,
            );
            if coverage <= 0.0 {
                continue;
            }
            blend_pixel(
                output,
                config.row_bytes(),
                x,
                y,
                p.color,
                (coverage * sample_weight).clamp(0.0, 1.0),
                config.blend_mode,
            );
        }
    }
}

fn sample_t(index: usize, count: usize) -> f32 {
    if count <= 1 {
        0.5
    } else {
        index as f32 / (count - 1) as f32
    }
}

fn shape_coverage(
    shape: ParticleShape,
    p: &Particle,
    config: &RenderConfig,
    center_x: f32,
    center_y: f32,
    px: f32,
    py: f32,
    radius: f32,
    _feather: f32,
) -> f32 {
    let dx = px - center_x;
    let dy = py - center_y;
    let dist = (dx * dx + dy * dy).sqrt();
    let edge_softness = config.edge_softness.clamp(0.0, 1.0);

    // softness=0: hard circle with 1px AA at edge
    // softness>0: solid core at radius, then feather OUTSIDE the radius
    //   feather width = radius * softness
    let coverage_at = |d: f32, r: f32, softness: f32| -> f32 {
        if softness <= 0.01 {
            // Hard edge with 1px AA
            (r - d + 0.5).clamp(0.0, 1.0)
        } else {
            let feather = (r * softness).max(1.0);
            if d <= r {
                1.0 // Fully inside core
            } else {
                // Smooth falloff from radius to radius+feather
                smooth_falloff(1.0 - (d - r) / feather)
            }
        }
    };

    let circle_coverage = coverage_at(dist, radius, edge_softness);

    let base = match shape {
        ParticleShape::Circle => circle_coverage,
        ParticleShape::Square => {
            let edge = dx.abs().max(dy.abs());
            coverage_at(edge, radius, edge_softness)
        }
        ParticleShape::Triangle => {
            let angle = (dy.atan2(dx) - p.rotation.to_radians()).rem_euclid(std::f32::consts::TAU);
            let sector = (angle / (std::f32::consts::TAU / 3.0)).fract();
            let radial = dist / radius.max(1.0);
            smooth_falloff(1.0 - radial - (sector - 0.5).abs() * 0.5)
        }
        ParticleShape::Star => {
            let angle = (dy.atan2(dx) - p.rotation.to_radians()).rem_euclid(std::f32::consts::TAU);
            let radial = dist / radius.max(1.0);
            let spikes = (angle * 5.0).cos().abs();
            smooth_falloff(1.0 - radial * (0.6 + spikes * 0.6))
        }
        ParticleShape::Line => {
            let angle = p.rotation.to_radians();
            let along = dx * angle.cos() + dy * angle.sin();
            let across = -dx * angle.sin() + dy * angle.cos();
            let line_len = radius * 1.35;
            let line_width = (radius * 0.12).max(0.8);
            let along_term = smooth_falloff(1.0 - along.abs() / line_len.max(1.0));
            let across_term = smooth_falloff(1.0 - across.abs() / line_width);
            along_term * across_term
        }
        ParticleShape::Image => 0.0,
    };

    base.clamp(0.0, 1.0) * p.color[3].clamp(0.0, 1.0)
}

fn select_sprite<'a>(p: &Particle, config: &'a RenderConfig) -> Option<&'a SpriteImage> {
    if config.sprite_images.is_empty() {
        return None;
    }
    let n = config.sprite_images.len();
    if n == 1 {
        return Some(&config.sprite_images[0]);
    }
    let idx = match config.time_sampling {
        TimeSamplingMode::CurrentTime => 0,
        TimeSamplingMode::BirthTime | TimeSamplingMode::RandomStill => p.sprite_frame as usize % n,
        TimeSamplingMode::RandomPlay => {
            let start = p.sprite_frame as usize;
            let age_frames = (p.age * 30.0).floor() as usize;
            (start + age_frames) % n
        }
        TimeSamplingMode::Cycle => {
            let t = (p.age / p.lifespan.max(0.001)).clamp(0.0, 0.999);
            (t * n as f32).floor() as usize
        }
    };
    Some(&config.sprite_images[idx.min(n - 1)])
}

fn draw_image_particle(
    p: &Particle,
    config: &RenderConfig,
    output: &mut [u8],
    local_x: f32,
    local_y: f32,
    radius: f32,
    sample_weight: f32,
) {
    let Some(sprite) = select_sprite(p, config) else {
        return;
    };
    if sprite.width == 0 || sprite.height == 0 || sprite.pixels.is_empty() {
        return;
    }

    let dest_w = (radius * 2.0).max(1.0);
    let dest_h = match config.image_fit_mode {
        ImageFitMode::Contain => dest_w * (sprite.height as f32 / sprite.width as f32),
        ImageFitMode::Stretch => dest_w,
    };

    // Select mip level based on texel-to-pixel ratio
    let texel_ratio = (sprite.width as f32 / dest_w).max(sprite.height as f32 / dest_h);
    let mip_idx = if texel_ratio > 1.5 {
        ((texel_ratio.log2().floor() as usize).max(1) - 1).min(sprite.mips.len().saturating_sub(1))
    } else {
        usize::MAX
    };
    let (sw, sh, pix): (usize, usize, &[u8]) = if mip_idx < sprite.mips.len() {
        let m = &sprite.mips[mip_idx];
        (m.width, m.height, &m.pixels)
    } else {
        (sprite.width, sprite.height, &sprite.pixels)
    };
    if sw == 0 || sh == 0 || pix.is_empty() {
        return;
    }

    let rotation = p.rotation.to_radians();
    let sin_r = rotation.sin();
    let cos_r = rotation.cos();
    let has_rotation = sin_r.abs() > 0.001;
    let (half_ext_x, half_ext_y) = if has_rotation {
        let hw = dest_w * 0.5;
        let hh = dest_h * 0.5;
        let ex = (hw * cos_r.abs() + hh * sin_r.abs()).ceil();
        let ey = (hw * sin_r.abs() + hh * cos_r.abs()).ceil();
        (ex, ey)
    } else {
        (dest_w * 0.5, dest_h * 0.5)
    };
    let min_x = (local_x - half_ext_x).floor() as isize;
    let min_y = (local_y - half_ext_y).floor() as isize;
    let max_x = (local_x + half_ext_x).ceil() as isize;
    let max_y = (local_y + half_ext_y).ceil() as isize;

    let clamped_w = (max_x.min(config.width as isize) - min_x.max(0)).max(0) as usize;
    let clamped_h = (max_y.min(config.height as isize) - min_y.max(0)).max(0) as usize;
    if clamped_w * clamped_h > MAX_PARTICLE_PIXEL_BUDGET {
        return;
    }

    for y in min_y.max(0)..=max_y.min(config.height.saturating_sub(1) as isize) {
        for x in min_x.max(0)..=max_x.min(config.width.saturating_sub(1) as isize) {
            let dx = (x as f32 + 0.5) - local_x;
            let dy = (y as f32 + 0.5) - local_y;
            let rx = dx * cos_r + dy * sin_r;
            let ry = -dx * sin_r + dy * cos_r;
            let u = (rx / dest_w) + 0.5;
            let v = (ry / dest_h) + 0.5;

            // Bilinear sample coordinates
            let fx = u * sw as f32 - 0.5;
            let fy = v * sh as f32 - 0.5;
            if fx < -0.5 || fx >= sw as f32 - 0.5 || fy < -0.5 || fy >= sh as f32 - 0.5 {
                continue;
            }
            let ix0 = fx.floor() as isize;
            let iy0 = fy.floor() as isize;
            let frac_x = fx - ix0 as f32;
            let frac_y = fy - iy0 as f32;

            let sample = |cx: isize, cy: isize| -> [f32; 4] {
                let cx = cx.clamp(0, sw as isize - 1) as usize;
                let cy = cy.clamp(0, sh as isize - 1) as usize;
                let i = (cy * sw + cx) * 4;
                if i + 3 < pix.len() {
                    [
                        pix[i] as f32,
                        pix[i + 1] as f32,
                        pix[i + 2] as f32,
                        pix[i + 3] as f32,
                    ]
                } else {
                    [0.0; 4]
                }
            };
            let s00 = sample(ix0, iy0);
            let s10 = sample(ix0 + 1, iy0);
            let s01 = sample(ix0, iy0 + 1);
            let s11 = sample(ix0 + 1, iy0 + 1);

            let inv_fx = 1.0 - frac_x;
            let inv_fy = 1.0 - frac_y;
            let lerped = [
                (s00[0] * inv_fx + s10[0] * frac_x) * inv_fy
                    + (s01[0] * inv_fx + s11[0] * frac_x) * frac_y,
                (s00[1] * inv_fx + s10[1] * frac_x) * inv_fy
                    + (s01[1] * inv_fx + s11[1] * frac_x) * frac_y,
                (s00[2] * inv_fx + s10[2] * frac_x) * inv_fy
                    + (s01[2] * inv_fx + s11[2] * frac_x) * frac_y,
                (s00[3] * inv_fx + s10[3] * frac_x) * inv_fy
                    + (s01[3] * inv_fx + s11[3] * frac_x) * frac_y,
            ];

            let source_alpha = lerped[0] / 255.0;
            if config.image_sampling.use_source_alpha && source_alpha <= 0.0 {
                continue;
            }

            // Unpremultiply: AE layers are premultiplied, so recover straight RGB
            let sprite_rgb =
                if config.image_sampling.source_premultiplied && source_alpha > 1.0 / 255.0 {
                    let inv_a = 1.0 / source_alpha;
                    [
                        (lerped[1] / 255.0 * inv_a).clamp(0.0, 1.0),
                        (lerped[2] / 255.0 * inv_a).clamp(0.0, 1.0),
                        (lerped[3] / 255.0 * inv_a).clamp(0.0, 1.0),
                    ]
                } else {
                    [lerped[1] / 255.0, lerped[2] / 255.0, lerped[3] / 255.0]
                };
            let alpha = if config.image_sampling.use_source_alpha {
                source_alpha
            } else {
                1.0
            };
            if alpha < config.image_sampling.alpha_clip {
                continue;
            }
            let color = match config.image_color_mode {
                ImageColorMode::Tint => [
                    sprite_rgb[0] * p.color[0],
                    sprite_rgb[1] * p.color[1],
                    sprite_rgb[2] * p.color[2],
                    p.color[3] * alpha,
                ],
                ImageColorMode::Source => [
                    sprite_rgb[0],
                    sprite_rgb[1],
                    sprite_rgb[2],
                    p.color[3] * alpha,
                ],
            };
            blend_pixel(
                output,
                config.row_bytes(),
                x as usize,
                y as usize,
                color,
                sample_weight,
                config.blend_mode,
            );
        }
    }
}

pub fn invert_camera_matrix(matrix: [[f64; 4]; 4]) -> Option<[[f64; 4]; 4]> {
    let col_major = [
        matrix[0][0],
        matrix[1][0],
        matrix[2][0],
        matrix[3][0],
        matrix[0][1],
        matrix[1][1],
        matrix[2][1],
        matrix[3][1],
        matrix[0][2],
        matrix[1][2],
        matrix[2][2],
        matrix[3][2],
        matrix[0][3],
        matrix[1][3],
        matrix[2][3],
        matrix[3][3],
    ];
    let inv = DMat4::from_cols_array(&col_major).inverse();
    if !inv.is_finite() {
        return None;
    }
    let cols = inv.to_cols_array();
    Some([
        [cols[0], cols[4], cols[8], cols[12]],
        [cols[1], cols[5], cols[9], cols[13]],
        [cols[2], cols[6], cols[10], cols[14]],
        [cols[3], cols[7], cols[11], cols[15]],
    ])
}

pub(crate) fn blend_pixel(
    output: &mut [u8],
    row_bytes: usize,
    x: usize,
    y: usize,
    color: [f32; 4],
    coverage: f32,
    blend_mode: BlendMode,
) {
    let idx = y * row_bytes + x * 4;
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
        BlendMode::Normal => [
            dst_rgb[0] * (1.0 - src_a) + src_rgb[0] * src_a,
            dst_rgb[1] * (1.0 - src_a) + src_rgb[1] * src_a,
            dst_rgb[2] * (1.0 - src_a) + src_rgb[2] * src_a,
        ],
        BlendMode::Add => [
            (dst_rgb[0] + src_rgb[0] * src_a * 1.2).clamp(0.0, 1.0),
            (dst_rgb[1] + src_rgb[1] * src_a * 1.2).clamp(0.0, 1.0),
            (dst_rgb[2] + src_rgb[2] * src_a * 1.2).clamp(0.0, 1.0),
        ],
        BlendMode::Screen => [
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

pub(crate) fn smooth_falloff(value: f32) -> f32 {
    let t = value.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub fn composite_apply(
    original: &[u8],
    orig_w: usize,
    orig_h: usize,
    orig_ox: i32,
    orig_oy: i32,
    particles: &[u8],
    part_w: usize,
    part_h: usize,
    part_ox: i32,
    part_oy: i32,
    output: &mut [u8],
    out_w: usize,
    out_h: usize,
    out_ox: i32,
    out_oy: i32,
    mode: ApplyMode,
) {
    let unpremultiply = |rgb: f32, a: f32| -> f32 {
        if a > 0.0 {
            (rgb / a).clamp(0.0, 1.0)
        } else {
            0.0
        }
    };

    for gy in out_oy..(out_oy + out_h as i32) {
        for gx in out_ox..(out_ox + out_w as i32) {
            let ox = (gx - out_ox) as usize;
            let oy = (gy - out_oy) as usize;
            let out_i = (oy * out_w + ox) * 4;
            if out_i + 3 >= output.len() {
                continue;
            }

            let (oa, or, og, ob) = {
                let lx = gx - orig_ox;
                let ly = gy - orig_oy;
                if lx >= 0 && (lx as usize) < orig_w && ly >= 0 && (ly as usize) < orig_h {
                    let i = (ly as usize * orig_w + lx as usize) * 4;
                    if i + 3 < original.len() {
                        (
                            original[i] as f32 / 255.0,
                            original[i + 1] as f32 / 255.0,
                            original[i + 2] as f32 / 255.0,
                            original[i + 3] as f32 / 255.0,
                        )
                    } else {
                        (0.0, 0.0, 0.0, 0.0)
                    }
                } else {
                    (0.0, 0.0, 0.0, 0.0)
                }
            };

            let (pa, pr, pg, pb) = {
                let lx = gx - part_ox;
                let ly = gy - part_oy;
                if lx >= 0 && (lx as usize) < part_w && ly >= 0 && (ly as usize) < part_h {
                    let i = (ly as usize * part_w + lx as usize) * 4;
                    if i + 3 < particles.len() {
                        (
                            particles[i] as f32 / 255.0,
                            particles[i + 1] as f32 / 255.0,
                            particles[i + 2] as f32 / 255.0,
                            particles[i + 3] as f32 / 255.0,
                        )
                    } else {
                        (0.0, 0.0, 0.0, 0.0)
                    }
                } else {
                    (0.0, 0.0, 0.0, 0.0)
                }
            };

            let (ra, rr, rg, rb) = match mode {
                ApplyMode::OnTransparent => (pa, pr, pg, pb),
                ApplyMode::Normal => {
                    let a = pa + oa * (1.0 - pa);
                    if a > 0.0 {
                        let rr = pr + or * (1.0 - pa);
                        let rg = pg + og * (1.0 - pa);
                        let rb = pb + ob * (1.0 - pa);
                        (a, rr.min(a), rg.min(a), rb.min(a))
                    } else {
                        (0.0, 0.0, 0.0, 0.0)
                    }
                }
                ApplyMode::Add => {
                    let a = (pa + oa * (1.0 - pa)).clamp(0.0, 1.0);
                    (
                        a,
                        (or + pr).clamp(0.0, a),
                        (og + pg).clamp(0.0, a),
                        (ob + pb).clamp(0.0, a),
                    )
                }
                ApplyMode::Screen => {
                    let a = (pa + oa * (1.0 - pa)).clamp(0.0, 1.0);
                    let sr = unpremultiply(pr, pa);
                    let sg = unpremultiply(pg, pa);
                    let sb = unpremultiply(pb, pa);
                    let dr = unpremultiply(or, oa);
                    let dg = unpremultiply(og, oa);
                    let db = unpremultiply(ob, oa);
                    let rr = (1.0 - (1.0 - dr) * (1.0 - sr)) * a;
                    let rg = (1.0 - (1.0 - dg) * (1.0 - sg)) * a;
                    let rb = (1.0 - (1.0 - db) * (1.0 - sb)) * a;
                    (a, rr.min(a), rg.min(a), rb.min(a))
                }
            };

            output[out_i] = (ra.clamp(0.0, 1.0) * 255.0).round() as u8;
            output[out_i + 1] = (rr.clamp(0.0, 1.0) * 255.0).round() as u8;
            output[out_i + 2] = (rg.clamp(0.0, 1.0) * 255.0).round() as u8;
            output[out_i + 3] = (rb.clamp(0.0, 1.0) * 255.0).round() as u8;
        }
    }
}
