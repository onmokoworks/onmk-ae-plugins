use after_effects as ae;

use crate::engine::ParticleEngineConfig;
use crate::particle::{AppearanceConfig, ChildConfig, EmitterConfig, EmitterType, PhysicsConfig};
use crate::renderer::{
    ApplyMode, BlendMode, ImageColorMode, ImageFitMode, ImageSamplingConfig, ParticleShape,
    RenderConfig, TimeSamplingMode,
};
use crate::Params;

pub(crate) fn extract_engine_config(
    params: &ae::Parameters<Params>,
) -> Result<ParticleEngineConfig, ae::Error> {
    let emitter_type_val = params.get(Params::EmitterType)?.as_popup()?.value();
    let emitter_type = match emitter_type_val {
        1 => EmitterType::Point,
        2 => EmitterType::Box,
        3 => EmitterType::Sphere,
        4 => EmitterType::Grid,
        5 => EmitterType::LayerAlpha,
        6 => EmitterType::Path,
        _ => EmitterType::Point,
    };

    let (pos_x, pos_y) = params.get(Params::PositionPoint)?.as_point()?.value();
    let pos_z = params.get(Params::PositionZ)?.as_float_slider()?.value() as f32;
    let size_linked = params
        .get(Params::EmitterSizeLinked)?
        .as_checkbox()?
        .value();
    let emitter_size_x = params.get(Params::EmitterSizeX)?.as_float_slider()?.value() as f32;
    let emitter_size_y = if size_linked {
        emitter_size_x
    } else {
        params.get(Params::EmitterSizeY)?.as_float_slider()?.value() as f32
    };
    let emitter_size_z = if size_linked {
        emitter_size_x
    } else {
        params.get(Params::EmitterSizeZ)?.as_float_slider()?.value() as f32
    };
    let spread_deg = params.get(Params::Spread)?.as_float_slider()?.value() as f32;
    let dir_x = params.get(Params::DirectionX)?.as_float_slider()?.value() as f32;
    let dir_y = params.get(Params::DirectionY)?.as_float_slider()?.value() as f32;
    let dir_z = params.get(Params::DirectionZ)?.as_float_slider()?.value() as f32;

    let sprite_time_sampling_val = params.get(Params::SpriteTimeSampling)?.as_popup()?.value();
    let sprite_frame_count = params
        .get(Params::SpriteFrameCount)?
        .as_slider()?
        .value()
        .max(1) as u16;

    let emitter = EmitterConfig {
        emitter_type,
        position: glam::Vec3::new(pos_x, pos_y, pos_z),
        size: glam::Vec3::new(emitter_size_x, emitter_size_y, emitter_size_z),
        source_points: None,
        birth_rate: params.get(Params::BirthRate)?.as_float_slider()?.value() as f32,
        lifespan: params.get(Params::Lifespan)?.as_float_slider()?.value() as f32,
        lifespan_variation: params.get(Params::LifespanVar)?.as_float_slider()?.value() as f32,
        initial_speed: params.get(Params::Speed)?.as_float_slider()?.value() as f32,
        speed_variation: params.get(Params::SpeedVar)?.as_float_slider()?.value() as f32,
        initial_direction: glam::Vec3::new(dir_x, dir_y, dir_z).normalize_or_zero(),
        spread: spread_deg.to_radians(),
        initial_size: params.get(Params::InitialSize)?.as_float_slider()?.value() as f32,
        size_variation: params.get(Params::SizeVar)?.as_float_slider()?.value() as f32,
        initial_rotation: params.get(Params::Rotation)?.as_float_slider()?.value() as f32,
        rotation_variation: params.get(Params::RotationVar)?.as_float_slider()?.value() as f32,
        rotation_speed: params
            .get(Params::RotationSpeed)?
            .as_float_slider()?
            .value() as f32,
        sprite_frame_count,
        sprite_time_sampling: (sprite_time_sampling_val - 1).max(0) as u8,
        opacity_variation: params.get(Params::OpacityVar)?.as_float_slider()?.value() as f32,
        grid_res_x: params.get(Params::GridResX)?.as_slider()?.value().max(1) as u32,
        grid_res_y: params.get(Params::GridResY)?.as_slider()?.value().max(1) as u32,
        grid_res_z: params.get(Params::GridResZ)?.as_slider()?.value().max(1) as u32,
        emit_all_at_start: params.get(Params::EmitMode)?.as_popup()?.value() == 2,
    };

    let physics = PhysicsConfig {
        gravity: glam::Vec3::new(
            0.0,
            params
                .get(Params::GravityStrength)?
                .as_float_slider()?
                .value() as f32,
            0.0,
        ),
        wind: glam::Vec3::new(
            params.get(Params::WindX)?.as_float_slider()?.value() as f32,
            params.get(Params::WindY)?.as_float_slider()?.value() as f32,
            0.0,
        ),
        air_resistance: params
            .get(Params::AirResistance)?
            .as_float_slider()?
            .value() as f32,
        turbulence_strength: params.get(Params::TurbStrength)?.as_float_slider()?.value() as f32,
        turbulence_scale: params.get(Params::TurbScale)?.as_float_slider()?.value() as f32,
        turbulence_speed: params.get(Params::TurbSpeed)?.as_float_slider()?.value() as f32,
        bounce_floor_y: 10000.0,
        bounce_enabled: params.get(Params::BounceEnabled)?.as_checkbox()?.value(),
        bounce_damping: params
            .get(Params::BounceDamping)?
            .as_float_slider()?
            .value() as f32,
    };

    let color_start_pix = params.get(Params::ColorStart)?.as_color()?.value();
    let color_mode = params.get(Params::ColorMode)?.as_popup()?.value();
    let color_end_pix = if color_mode == 1 {
        color_start_pix
    } else {
        params.get(Params::ColorEnd)?.as_color()?.value()
    };
    let opacity_start = params.get(Params::OpacityStart)?.as_float_slider()?.value() as f32 / 100.0;
    let opacity_mid_a = params.get(Params::OpacityMidA)?.as_float_slider()?.value() as f32 / 100.0;
    let opacity_mid_b = params.get(Params::OpacityMidB)?.as_float_slider()?.value() as f32 / 100.0;
    let opacity_end = params.get(Params::OpacityEnd)?.as_float_slider()?.value() as f32 / 100.0;
    let size_life_start = params
        .get(Params::SizeLifeStart)?
        .as_float_slider()?
        .value() as f32;
    let size_life_mid_a = params.get(Params::SizeLifeMidA)?.as_float_slider()?.value() as f32;
    let size_life_mid_b = params.get(Params::SizeLifeMidB)?.as_float_slider()?.value() as f32;
    let size_life_end = params.get(Params::SizeLifeEnd)?.as_float_slider()?.value() as f32;

    let appearance = AppearanceConfig {
        color_start: [
            color_start_pix.red as f32 / 255.0,
            color_start_pix.green as f32 / 255.0,
            color_start_pix.blue as f32 / 255.0,
            opacity_start,
        ],
        color_end: [
            color_end_pix.red as f32 / 255.0,
            color_end_pix.green as f32 / 255.0,
            color_end_pix.blue as f32 / 255.0,
            opacity_end,
        ],
        size_over_life: [
            size_life_start,
            size_life_mid_a,
            size_life_mid_b,
            size_life_end,
        ],
        opacity_over_life: [opacity_start, opacity_mid_a, opacity_mid_b, opacity_end],
    };

    let child = ChildConfig {
        enabled: params.get(Params::ChildEnabled)?.as_checkbox()?.value(),
        count: params.get(Params::ChildCount)?.as_slider()?.value() as u32,
        inherit_velocity: params
            .get(Params::ChildInheritVel)?
            .as_float_slider()?
            .value() as f32,
        lifespan: params
            .get(Params::ChildLifespan)?
            .as_float_slider()?
            .value() as f32,
        initial_speed: params.get(Params::ChildSpeed)?.as_float_slider()?.value() as f32,
        spread: params
            .get(Params::ChildSpread)?
            .as_float_slider()?
            .value()
            .to_radians() as f32,
        size_scale: params
            .get(Params::ChildSizeScale)?
            .as_float_slider()?
            .value() as f32,
    };

    let shape_val = params.get(Params::Shape)?.as_popup()?.value();
    let shape = match shape_val {
        1 => ParticleShape::Circle,
        2 => ParticleShape::Square,
        3 => ParticleShape::Triangle,
        4 => ParticleShape::Star,
        5 => ParticleShape::Line,
        6 => ParticleShape::Image,
        _ => ParticleShape::Circle,
    };

    let blend_val = params.get(Params::BlendModeParam)?.as_popup()?.value();
    let blend_mode = match blend_val {
        1 => BlendMode::Normal,
        2 => BlendMode::Add,
        3 => BlendMode::Screen,
        _ => BlendMode::Normal,
    };

    let render = RenderConfig {
        width: 0,
        height: 0,
        row_stride: 0,
        origin_x: 0.0,
        origin_y: 0.0,
        frame_dt: 1.0 / 30.0,
        shape,
        blend_mode,
        motion_blur: params.get(Params::MotionBlur)?.as_float_slider()?.value() as f32,
        edge_softness: params.get(Params::EdgeSoftness)?.as_float_slider()?.value() as f32,
        dof_enabled: params.get(Params::DOFEnabled)?.as_checkbox()?.value(),
        dof_focal_distance: params.get(Params::DOFFocalDist)?.as_float_slider()?.value() as f32,
        dof_aperture: params.get(Params::DOFAperture)?.as_float_slider()?.value() as f32,
        composite_on_original: params.get(Params::CompositeOnOrig)?.as_checkbox()?.value(),
        apply_mode: match params.get(Params::ApplyMode)?.as_popup()?.value() {
            2 => ApplyMode::Normal,
            3 => ApplyMode::Add,
            4 => ApplyMode::Screen,
            _ => ApplyMode::OnTransparent,
        },
        size_multiplier: params
            .get(Params::SizeMultiplier)?
            .as_float_slider()?
            .value() as f32,
        sprite_images: Vec::new(),
        time_sampling: match sprite_time_sampling_val {
            2 => TimeSamplingMode::BirthTime,
            3 => TimeSamplingMode::RandomStill,
            4 => TimeSamplingMode::RandomPlay,
            5 => TimeSamplingMode::Cycle,
            _ => TimeSamplingMode::CurrentTime,
        },
        image_color_mode: match params.get(Params::ImageColorMode)?.as_popup()?.value() {
            2 => ImageColorMode::Source,
            _ => ImageColorMode::Tint,
        },
        image_fit_mode: match params.get(Params::ImageFitMode)?.as_popup()?.value() {
            2 => ImageFitMode::Stretch,
            _ => ImageFitMode::Contain,
        },
        image_sampling: ImageSamplingConfig {
            use_source_alpha: params.get(Params::UseSourceAlpha)?.as_checkbox()?.value(),
            source_premultiplied: params
                .get(Params::SourcePremultiplied)?
                .as_checkbox()?
                .value(),
            alpha_clip: params
                .get(Params::ImageAlphaClip)?
                .as_float_slider()?
                .value() as f32,
        },
        camera_projection: None,
    };

    let seed = params.get(Params::Seed)?.as_slider()?.value() as u64;

    Ok(ParticleEngineConfig {
        emitter,
        physics,
        appearance,
        child,
        render,
        seed,
    })
}
