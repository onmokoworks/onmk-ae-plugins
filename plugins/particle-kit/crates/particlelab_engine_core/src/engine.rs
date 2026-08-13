use std::time::Instant;

use crate::particle::{
    AppearanceConfig, ChildConfig, EmitterConfig, EmitterType, ParticleSystem, PhysicsConfig,
};
use crate::render_core::{RenderFrame, RenderSurface, RenderSurfaceError};
use crate::renderer::{
    self, ApplyMode, BlendMode, CameraProjection, ImageColorMode, ImageFitMode,
    ImageSamplingConfig, ParticleShape, RenderConfig, SpriteImage, TimeSamplingMode,
};
use std::sync::Arc;

const MAX_BIRTH_RATE: f32 = 100_000.0;
const MAX_LIFESPAN: f32 = 3_600.0;
const MAX_GRID_RESOLUTION: u32 = 512;
const MAX_CHILD_COUNT: u32 = 1_024;
const MAX_SIZE_VALUE: f32 = 64.0;
const MAX_DISTANCE_VALUE: f32 = 1_000_000.0;
const MAX_FORCE_VALUE: f32 = 100_000.0;

#[derive(Clone, Debug)]
pub struct ParticleEngineConfig {
    pub emitter: EmitterConfig,
    pub physics: PhysicsConfig,
    pub appearance: AppearanceConfig,
    pub child: ChildConfig,
    pub render: RenderConfig,
    pub seed: u64,
}

#[derive(Clone, Debug)]
pub struct ParticleRuntimeInputs {
    pub frame: RenderFrame,
    pub source_points: Option<Arc<Vec<glam::Vec3>>>,
    pub sprite_images: Vec<SpriteImage>,
    pub camera_projection: Option<CameraProjection>,
}

#[derive(Clone, Debug)]
pub struct ParticleRenderPlan {
    pub runtime: ParticleRuntimeInputs,
    pub final_apply_mode: Option<ApplyMode>,
}

impl ParticleEngineConfig {
    pub fn classic_default() -> Self {
        Self {
            emitter: EmitterConfig {
                emitter_type: EmitterType::Point,
                position: glam::Vec3::new(50.0, 50.0, 0.0),
                size: glam::Vec3::ZERO,
                source_points: None,
                birth_rate: 180.0,
                lifespan: 1.6,
                lifespan_variation: 0.15,
                initial_speed: 240.0,
                speed_variation: 0.0,
                initial_direction: glam::Vec3::new(0.0, -1.0, 0.0),
                spread: 18.0_f32.to_radians(),
                initial_size: 9.0,
                size_variation: 0.2,
                initial_rotation: 0.0,
                rotation_variation: 0.0,
                rotation_speed: 0.0,
                sprite_frame_count: 1,
                sprite_time_sampling: 0,
                opacity_variation: 0.0,
                grid_res_x: 8,
                grid_res_y: 8,
                grid_res_z: 1,
                emit_all_at_start: false,
            },
            physics: PhysicsConfig {
                gravity: glam::Vec3::new(0.0, 160.0, 0.0),
                wind: glam::Vec3::ZERO,
                air_resistance: 0.3,
                turbulence_strength: 12.0,
                turbulence_scale: 0.75,
                turbulence_speed: 1.0,
                bounce_floor_y: 10000.0,
                bounce_enabled: false,
                bounce_damping: 0.5,
            },
            appearance: AppearanceConfig {
                color_start: [1.0, 1.0, 1.0, 1.0],
                color_end: [1.0, 1.0, 1.0, 0.0],
                size_over_life: [1.0, 1.0, 0.65, 0.35],
                opacity_over_life: [1.0, 0.9, 0.45, 0.0],
            },
            child: ChildConfig {
                enabled: false,
                count: 3,
                inherit_velocity: 0.65,
                lifespan: 0.5,
                initial_speed: 80.0,
                spread: 110.0_f32.to_radians(),
                size_scale: 0.4,
            },
            render: RenderConfig {
                width: 0,
                height: 0,
                row_stride: 0,
                origin_x: 0.0,
                origin_y: 0.0,
                frame_dt: 1.0 / 30.0,
                shape: ParticleShape::Circle,
                blend_mode: BlendMode::Normal,
                motion_blur: 0.2,
                edge_softness: 0.0,
                dof_enabled: false,
                dof_focal_distance: 0.0,
                dof_aperture: 5.0,
                composite_on_original: true,
                apply_mode: ApplyMode::Normal,
                size_multiplier: 1.15,
                sprite_images: Vec::new(),
                time_sampling: TimeSamplingMode::CurrentTime,
                image_color_mode: ImageColorMode::Tint,
                image_fit_mode: ImageFitMode::Contain,
                image_sampling: ImageSamplingConfig {
                    use_source_alpha: true,
                    source_premultiplied: true,
                    alpha_clip: 0.01,
                },
                camera_projection: None,
            },
            seed: 12345,
        }
    }

    pub fn final_apply_mode(&self) -> Option<ApplyMode> {
        if self.render.apply_mode != ApplyMode::OnTransparent {
            Some(self.render.apply_mode)
        } else if self.render.composite_on_original {
            Some(ApplyMode::Normal)
        } else {
            None
        }
    }

    pub fn normalized_for_render(mut self) -> Self {
        let defaults = Self::classic_default();

        self.emitter.position = finite_vec3(self.emitter.position, defaults.emitter.position);
        self.emitter.size = finite_vec3(self.emitter.size, defaults.emitter.size).abs();
        self.emitter.source_points = None;
        self.emitter.birth_rate = finite_clamp(
            self.emitter.birth_rate,
            defaults.emitter.birth_rate,
            0.0,
            MAX_BIRTH_RATE,
        );
        self.emitter.lifespan = finite_clamp(
            self.emitter.lifespan,
            defaults.emitter.lifespan,
            0.01,
            MAX_LIFESPAN,
        );
        self.emitter.lifespan_variation = finite_clamp(
            self.emitter.lifespan_variation,
            defaults.emitter.lifespan_variation,
            0.0,
            1.0,
        );
        self.emitter.initial_speed = finite_clamp(
            self.emitter.initial_speed,
            defaults.emitter.initial_speed,
            0.0,
            MAX_FORCE_VALUE,
        );
        self.emitter.speed_variation = finite_clamp(
            self.emitter.speed_variation,
            defaults.emitter.speed_variation,
            0.0,
            1.0,
        );
        self.emitter.initial_direction = normalized_direction(
            self.emitter.initial_direction,
            defaults.emitter.initial_direction,
        );
        self.emitter.spread = finite_clamp(
            self.emitter.spread,
            defaults.emitter.spread,
            0.0,
            std::f32::consts::TAU,
        );
        self.emitter.initial_size = finite_clamp(
            self.emitter.initial_size,
            defaults.emitter.initial_size,
            0.01,
            MAX_SIZE_VALUE,
        );
        self.emitter.size_variation = finite_clamp(
            self.emitter.size_variation,
            defaults.emitter.size_variation,
            0.0,
            1.0,
        );
        self.emitter.initial_rotation = finite_or(
            self.emitter.initial_rotation,
            defaults.emitter.initial_rotation,
        );
        self.emitter.rotation_variation = finite_clamp(
            self.emitter.rotation_variation,
            defaults.emitter.rotation_variation,
            0.0,
            360.0,
        );
        self.emitter.rotation_speed =
            finite_or(self.emitter.rotation_speed, defaults.emitter.rotation_speed);
        self.emitter.sprite_frame_count = self.emitter.sprite_frame_count.max(1);
        self.emitter.sprite_time_sampling = self.emitter.sprite_time_sampling.min(4);
        self.emitter.opacity_variation = finite_clamp(
            self.emitter.opacity_variation,
            defaults.emitter.opacity_variation,
            0.0,
            1.0,
        );
        self.emitter.grid_res_x = self.emitter.grid_res_x.clamp(1, MAX_GRID_RESOLUTION);
        self.emitter.grid_res_y = self.emitter.grid_res_y.clamp(1, MAX_GRID_RESOLUTION);
        self.emitter.grid_res_z = self.emitter.grid_res_z.clamp(1, MAX_GRID_RESOLUTION);

        self.physics.gravity = finite_vec3(self.physics.gravity, defaults.physics.gravity)
            .clamp_length_max(MAX_FORCE_VALUE);
        self.physics.wind =
            finite_vec3(self.physics.wind, defaults.physics.wind).clamp_length_max(MAX_FORCE_VALUE);
        self.physics.air_resistance = finite_clamp(
            self.physics.air_resistance,
            defaults.physics.air_resistance,
            0.0,
            100.0,
        );
        self.physics.turbulence_strength = finite_clamp(
            self.physics.turbulence_strength,
            defaults.physics.turbulence_strength,
            0.0,
            MAX_FORCE_VALUE,
        );
        self.physics.turbulence_scale = finite_clamp(
            self.physics.turbulence_scale,
            defaults.physics.turbulence_scale,
            0.0001,
            MAX_DISTANCE_VALUE,
        );
        self.physics.turbulence_speed = finite_or(
            self.physics.turbulence_speed,
            defaults.physics.turbulence_speed,
        );
        self.physics.bounce_floor_y = finite_clamp(
            self.physics.bounce_floor_y,
            defaults.physics.bounce_floor_y,
            -MAX_DISTANCE_VALUE,
            MAX_DISTANCE_VALUE,
        );
        self.physics.bounce_damping = finite_clamp(
            self.physics.bounce_damping,
            defaults.physics.bounce_damping,
            0.0,
            1.0,
        );

        self.appearance.color_start =
            finite_color(self.appearance.color_start, defaults.appearance.color_start);
        self.appearance.color_end =
            finite_color(self.appearance.color_end, defaults.appearance.color_end);
        self.appearance.size_over_life = finite_size_curve(
            self.appearance.size_over_life,
            defaults.appearance.size_over_life,
        );
        self.appearance.opacity_over_life = finite_opacity_curve(
            self.appearance.opacity_over_life,
            defaults.appearance.opacity_over_life,
        );

        self.child.count = self.child.count.min(MAX_CHILD_COUNT);
        self.child.inherit_velocity = finite_clamp(
            self.child.inherit_velocity,
            defaults.child.inherit_velocity,
            0.0,
            10.0,
        );
        self.child.lifespan = finite_clamp(
            self.child.lifespan,
            defaults.child.lifespan,
            0.01,
            MAX_LIFESPAN,
        );
        self.child.initial_speed = finite_clamp(
            self.child.initial_speed,
            defaults.child.initial_speed,
            0.0,
            MAX_FORCE_VALUE,
        );
        self.child.spread = finite_clamp(
            self.child.spread,
            defaults.child.spread,
            0.0,
            std::f32::consts::TAU,
        );
        self.child.size_scale = finite_clamp(
            self.child.size_scale,
            defaults.child.size_scale,
            0.0,
            MAX_SIZE_VALUE,
        );

        self.render.width = 0;
        self.render.height = 0;
        self.render.row_stride = 0;
        self.render.origin_x = 0.0;
        self.render.origin_y = 0.0;
        self.render.frame_dt = defaults.render.frame_dt;
        self.render.motion_blur = finite_clamp(
            self.render.motion_blur,
            defaults.render.motion_blur,
            0.0,
            1.0,
        );
        self.render.edge_softness = finite_clamp(
            self.render.edge_softness,
            defaults.render.edge_softness,
            0.0,
            1.0,
        );
        self.render.dof_focal_distance = finite_clamp(
            self.render.dof_focal_distance,
            defaults.render.dof_focal_distance,
            -MAX_DISTANCE_VALUE,
            MAX_DISTANCE_VALUE,
        );
        self.render.dof_aperture = finite_clamp(
            self.render.dof_aperture,
            defaults.render.dof_aperture,
            0.001,
            MAX_DISTANCE_VALUE,
        );
        self.render.size_multiplier = finite_clamp(
            self.render.size_multiplier,
            defaults.render.size_multiplier,
            0.01,
            MAX_SIZE_VALUE,
        );
        self.render.sprite_images.clear();
        self.render.image_sampling.alpha_clip = finite_clamp(
            self.render.image_sampling.alpha_clip,
            defaults.render.image_sampling.alpha_clip,
            0.0,
            1.0,
        );
        self.render.camera_projection = None;

        self
    }
}

fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value
    } else {
        fallback
    }
}

fn finite_clamp(value: f32, fallback: f32, min: f32, max: f32) -> f32 {
    finite_or(value, fallback).clamp(min, max)
}

fn finite_vec3(value: glam::Vec3, fallback: glam::Vec3) -> glam::Vec3 {
    if value.is_finite() {
        value
    } else {
        fallback
    }
}

fn normalized_direction(value: glam::Vec3, fallback: glam::Vec3) -> glam::Vec3 {
    let value = finite_vec3(value, fallback);
    if value.length_squared() > 0.000001 {
        value.normalize()
    } else {
        fallback
    }
}

fn finite_color(value: [f32; 4], fallback: [f32; 4]) -> [f32; 4] {
    [
        finite_clamp(value[0], fallback[0], 0.0, 1.0),
        finite_clamp(value[1], fallback[1], 0.0, 1.0),
        finite_clamp(value[2], fallback[2], 0.0, 1.0),
        finite_clamp(value[3], fallback[3], 0.0, 1.0),
    ]
}

fn finite_size_curve(value: [f32; 4], fallback: [f32; 4]) -> [f32; 4] {
    [
        finite_clamp(value[0], fallback[0], 0.0, MAX_SIZE_VALUE),
        finite_clamp(value[1], fallback[1], 0.0, MAX_SIZE_VALUE),
        finite_clamp(value[2], fallback[2], 0.0, MAX_SIZE_VALUE),
        finite_clamp(value[3], fallback[3], 0.0, MAX_SIZE_VALUE),
    ]
}

fn finite_opacity_curve(value: [f32; 4], fallback: [f32; 4]) -> [f32; 4] {
    [
        finite_clamp(value[0], fallback[0], 0.0, 1.0),
        finite_clamp(value[1], fallback[1], 0.0, 1.0),
        finite_clamp(value[2], fallback[2], 0.0, 1.0),
        finite_clamp(value[3], fallback[3], 0.0, 1.0),
    ]
}

impl ParticleRuntimeInputs {
    pub fn from_frame(frame: RenderFrame) -> Self {
        Self {
            frame,
            source_points: None,
            sprite_images: Vec::new(),
            camera_projection: None,
        }
    }

    pub fn new(
        output_width: usize,
        output_height: usize,
        origin_x: i32,
        origin_y: i32,
        time: f32,
        dt: f32,
    ) -> Result<Self, RenderSurfaceError> {
        Ok(Self::from_frame(RenderFrame::argb8(
            output_width,
            output_height,
            origin_x,
            origin_y,
            time,
            dt,
        )?))
    }

    pub fn argb8_with_row_bytes(
        output_width: usize,
        output_height: usize,
        row_bytes: usize,
        origin_x: i32,
        origin_y: i32,
        time: f32,
        dt: f32,
    ) -> Result<Self, RenderSurfaceError> {
        Ok(Self::from_frame(RenderFrame::argb8_with_row_bytes(
            output_width,
            output_height,
            row_bytes,
            origin_x,
            origin_y,
            time,
            dt,
        )?))
    }

    pub fn surface(&self) -> Result<RenderSurface, RenderSurfaceError> {
        Ok(self.frame.surface)
    }
}

impl ParticleRenderPlan {
    pub fn new(engine: &ParticleEngineConfig, runtime: ParticleRuntimeInputs) -> Self {
        Self {
            runtime,
            final_apply_mode: engine.final_apply_mode(),
        }
    }

    pub fn apply_to_emitter_config(&self, emitter: &mut EmitterConfig) {
        emitter.source_points = self.runtime.source_points.clone();
    }

    pub fn apply_to_render_config(&self, render: &mut RenderConfig) {
        if let Ok(surface) = self.runtime.surface() {
            render.width = surface.width;
            render.height = surface.height;
            render.row_stride = surface.row_bytes;
        } else {
            render.width = 0;
            render.height = 0;
            render.row_stride = 0;
        }
        render.origin_x = self.runtime.frame.origin_x as f32;
        render.origin_y = self.runtime.frame.origin_y as f32;
        render.frame_dt = self.runtime.frame.dt.max(1.0 / 240.0);
        render.sprite_images = self.runtime.sprite_images.clone();
        render.camera_projection = self.runtime.camera_projection;
    }
}

pub fn render_particle_engine_8bit<E>(
    mut engine: ParticleEngineConfig,
    plan: ParticleRenderPlan,
    output: &mut [u8],
    deadline: Option<Instant>,
    mut check_abort: impl FnMut() -> Result<(), E>,
) -> Result<usize, E> {
    engine = engine.normalized_for_render();
    plan.apply_to_emitter_config(&mut engine.emitter);
    plan.apply_to_render_config(&mut engine.render);

    let mut system = ParticleSystem::new(
        engine.emitter,
        engine.physics,
        engine.appearance,
        engine.child,
        engine.seed,
    );
    system.simulate_to_time(plan.runtime.frame.time, plan.runtime.frame.dt);
    let particles = system.get_particles();
    let particle_count = particles.len();

    check_abort()?;
    if let Ok(surface) = plan.runtime.surface() {
        if !surface.is_empty() && surface.fits_buffer(output) {
            renderer::render_particles_8bit(particles, &engine.render, output, deadline);
        }
    }

    Ok(particle_count)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runtime(
        output_width: usize,
        output_height: usize,
        origin_x: i32,
        origin_y: i32,
        time: f32,
        dt: f32,
    ) -> ParticleRuntimeInputs {
        ParticleRuntimeInputs::new(output_width, output_height, origin_x, origin_y, time, dt)
            .unwrap()
    }

    #[test]
    fn render_plan_resolves_legacy_composite_mode() {
        let mut engine = ParticleEngineConfig::classic_default();
        engine.render.apply_mode = ApplyMode::OnTransparent;
        engine.render.composite_on_original = true;
        let plan = ParticleRenderPlan::new(&engine, runtime(320, 240, -10, 20, 1.5, 1.0 / 30.0));
        assert_eq!(plan.final_apply_mode, Some(ApplyMode::Normal));

        engine.render.composite_on_original = false;
        let plan = ParticleRenderPlan::new(&engine, runtime(320, 240, -10, 20, 1.5, 1.0 / 30.0));
        assert_eq!(plan.final_apply_mode, None);
    }

    #[test]
    fn render_plan_applies_output_surface_to_render_config() {
        let engine = ParticleEngineConfig::classic_default();
        let mut render = engine.render.clone();
        let plan = ParticleRenderPlan::new(&engine, runtime(640, 360, 12, -8, 2.0, 0.001));

        plan.apply_to_render_config(&mut render);

        assert_eq!(render.width, 640);
        assert_eq!(render.height, 360);
        assert_eq!(render.row_stride, 2_560);
        assert_eq!(render.origin_x, 12.0);
        assert_eq!(render.origin_y, -8.0);
        assert_eq!(render.frame_dt, 1.0 / 240.0);
    }

    #[test]
    fn runtime_inputs_accept_host_row_stride() {
        let runtime =
            ParticleRuntimeInputs::argb8_with_row_bytes(32, 16, 160, -4, 2, 0.75, 1.0 / 30.0)
                .unwrap();
        assert_eq!(runtime.frame.surface.width, 32);
        assert_eq!(runtime.frame.surface.height, 16);
        assert_eq!(runtime.frame.surface.row_bytes, 160);
        assert_eq!(runtime.frame.surface.len_bytes, 2_560);
        assert_eq!(runtime.frame.origin_x, -4);
        assert_eq!(runtime.frame.origin_y, 2);
    }

    #[test]
    fn render_plan_applies_runtime_assets_without_mutating_engine_config() {
        let engine = ParticleEngineConfig::classic_default();
        let mut runtime = runtime(128, 96, 3, 4, 0.5, 1.0 / 30.0);
        let source_points = Arc::new(vec![glam::Vec3::new(1.0, 2.0, 3.0)]);
        runtime.source_points = Some(source_points.clone());
        runtime.sprite_images = vec![SpriteImage {
            width: 1,
            height: 1,
            pixels: Arc::new(vec![255, 255, 255, 255]),
            mips: Arc::new(Vec::new()),
        }];
        runtime.camera_projection = Some(CameraProjection {
            matrix: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
            invert_matrix: false,
            image_plane_dist: 1000.0,
            image_plane_width: 1920.0,
            image_plane_height: 1080.0,
        });

        let plan = ParticleRenderPlan::new(&engine, runtime);
        let mut emitter = engine.emitter.clone();
        let mut render = engine.render.clone();

        plan.apply_to_emitter_config(&mut emitter);
        plan.apply_to_render_config(&mut render);

        assert!(engine.emitter.source_points.is_none());
        assert!(engine.render.sprite_images.is_empty());
        assert!(engine.render.camera_projection.is_none());
        assert_eq!(emitter.source_points.as_ref().unwrap().len(), 1);
        assert_eq!(render.sprite_images.len(), 1);
        assert!(render.camera_projection.is_some());
    }

    #[test]
    fn engine_config_normalization_sanitizes_untrusted_values() {
        let mut engine = ParticleEngineConfig::classic_default();
        engine.emitter.source_points = Some(Arc::new(vec![glam::Vec3::ZERO]));
        engine.emitter.birth_rate = f32::NAN;
        engine.emitter.lifespan = -10.0;
        engine.emitter.initial_direction = glam::Vec3::ZERO;
        engine.emitter.speed_variation = 99.0;
        engine.emitter.grid_res_x = 0;
        engine.emitter.grid_res_y = u32::MAX;
        engine.emitter.grid_res_z = 4;
        engine.physics.gravity = glam::Vec3::new(f32::INFINITY, 0.0, 0.0);
        engine.physics.bounce_damping = -1.0;
        engine.appearance.color_start = [2.0, -1.0, f32::NAN, 0.5];
        engine.appearance.opacity_over_life = [0.0, 0.5, 2.0, f32::NAN];
        engine.child.count = u32::MAX;
        engine.child.lifespan = f32::NAN;
        engine.render.width = 999;
        engine.render.height = 888;
        engine.render.origin_x = 42.0;
        engine.render.sprite_images = vec![SpriteImage {
            width: 1,
            height: 1,
            pixels: Arc::new(vec![255, 255, 255, 255]),
            mips: Arc::new(Vec::new()),
        }];
        engine.render.camera_projection = Some(CameraProjection {
            matrix: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
            invert_matrix: false,
            image_plane_dist: 1000.0,
            image_plane_width: 1920.0,
            image_plane_height: 1080.0,
        });
        engine.render.motion_blur = 8.0;
        engine.render.image_sampling.alpha_clip = -2.0;

        let normalized = engine.normalized_for_render();
        let default = ParticleEngineConfig::classic_default();

        assert!(normalized.emitter.source_points.is_none());
        assert_eq!(normalized.emitter.birth_rate, default.emitter.birth_rate);
        assert_eq!(normalized.emitter.lifespan, 0.01);
        assert_eq!(normalized.emitter.speed_variation, 1.0);
        assert_eq!(
            normalized.emitter.initial_direction,
            default.emitter.initial_direction
        );
        assert_eq!(normalized.emitter.grid_res_x, 1);
        assert_eq!(normalized.emitter.grid_res_y, MAX_GRID_RESOLUTION);
        assert_eq!(normalized.emitter.grid_res_z, 4);
        assert_eq!(normalized.physics.gravity, default.physics.gravity);
        assert_eq!(normalized.physics.bounce_damping, 0.0);
        assert_eq!(normalized.appearance.color_start, [1.0, 0.0, 1.0, 0.5]);
        assert_eq!(
            normalized.appearance.opacity_over_life,
            [0.0, 0.5, 1.0, 0.0]
        );
        assert_eq!(normalized.child.count, MAX_CHILD_COUNT);
        assert_eq!(normalized.child.lifespan, default.child.lifespan);
        assert_eq!(normalized.render.width, 0);
        assert_eq!(normalized.render.height, 0);
        assert_eq!(normalized.render.origin_x, 0.0);
        assert!(normalized.render.sprite_images.is_empty());
        assert!(normalized.render.camera_projection.is_none());
        assert_eq!(normalized.render.motion_blur, 1.0);
        assert_eq!(normalized.render.image_sampling.alpha_clip, 0.0);
    }

    #[test]
    fn render_helper_simulates_and_draws_particles() {
        let engine = ParticleEngineConfig::classic_default();
        let plan = ParticleRenderPlan::new(&engine, runtime(64, 64, 0, 0, 0.25, 1.0 / 30.0));
        let mut output = vec![0u8; 64 * 64 * 4];

        let count =
            render_particle_engine_8bit(engine, plan, &mut output, None, || Ok::<(), ()>(()))
                .unwrap();

        assert!(count > 0);
    }

    #[test]
    fn render_helper_ignores_undersized_output_buffers() {
        let engine = ParticleEngineConfig::classic_default();
        let plan = ParticleRenderPlan::new(&engine, runtime(16, 16, 0, 0, 0.5, 1.0 / 30.0));
        let mut output = vec![7u8; 15];

        render_particle_engine_8bit(engine, plan, &mut output, None, || Ok::<(), ()>(())).unwrap();

        assert_eq!(output, vec![7u8; 15]);
    }
}
