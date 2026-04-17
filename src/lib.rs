use after_effects as ae;
use serde::{Deserialize, Serialize};
use std::time::Instant;
#[cfg(windows)]
use std::fs;
#[cfg(windows)]
use std::fs::OpenOptions;
#[cfg(windows)]
use std::io::Write;
#[cfg(windows)]
use std::path::PathBuf;
use std::panic::{self, AssertUnwindSafe};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock, OnceLock};

mod particle;
mod plexus;
mod plexus_render;
mod renderer;

use particle::{AppearanceConfig, ChildConfig, EmitterConfig, EmitterType, ParticleSystem, PhysicsConfig};
use plexus::{PlexusConfig, PointGroupConfig, PointSourceType};
use renderer::{invert_camera_matrix, project_point_3d, BlendMode, CameraProjection, ImageColorMode, ImageFitMode, ImageSamplingConfig, ParticleShape, RenderConfig, SpriteImage};


// ---- Parameter IDs ----

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    PresetGroupStart,
    SavePreset,
    LoadPreset,
    DeletePreset,
    OpenPresetFolder,
    PresetGroupEnd,
    EmitterGroupStart,
    // Emitter
    EmitterType,
    PositionPoint,
    PositionZ,
    ImageSourceLayer,
    ImageProxyScale,
    RefreshImageCache,
    EmitterSizeLinked,
    EmitterSizeX,
    EmitterSizeY,
    EmitterSizeZ,
    BirthRate,
    Lifespan,
    LifespanVar,
    EmitterGroupEnd,
    MotionGroupStart,
    // Motion
    Speed,
    SpeedVar,
    DirectionX,
    DirectionY,
    DirectionZ,
    Spread,
    InitialSize,
    SizeVar,
    Rotation,
    RotationSpeed,
    MotionGroupEnd,
    PhysicsGroupStart,
    // Physics
    GravityStrength,
    WindX,
    WindY,
    TurbStrength,
    TurbScale,
    TurbSpeed,
    AirResistance,
    BounceEnabled,
    BounceDamping,
    PhysicsGroupEnd,
    AppearanceGroupStart,
    // Appearance
    ColorMode,
    ColorStart,
    ColorEnd,
    OpacityCurvePreset,
    OpacityStart,
    OpacityMidA,
    OpacityMidB,
    OpacityEnd,
    SizeCurvePreset,
    SizeLifeStart,
    SizeLifeMidA,
    SizeLifeMidB,
    SizeLifeEnd,
    AppearanceGroupEnd,
    RenderingGroupStart,
    // Rendering
    Shape,
    ImageColorMode,
    ImageFitMode,
    UseSourceAlpha,
    SourcePremultiplied,
    ImageAlphaClip,
    BlendModeParam,
    MotionBlur,
    EdgeSoftness,
    DOFEnabled,
    DOFFocalDist,
    DOFAperture,
    SizeMultiplier,
    CompositeOnOrig,
    RenderingGroupEnd,
    ChildGroupStart,
    // Child
    ChildEnabled,
    ChildCount,
    ChildInheritVel,
    ChildLifespan,
    ChildSpeed,
    ChildSpread,
    ChildSizeScale,
    ChildGroupEnd,
    SystemGroupStart,
    // System
    Seed,
    SystemGroupEnd,
    // ---- Plexus ----
    PluginMode,
    PlexusGroupStart,
    PlexusGroupEnd,
    // Point Group A
    PointGroupAStart,
    PointGroupAEnd,
    PointAEnabled,
    PointASourceType,
    PointASourceLayer,
    PointAGridResX,
    PointAGridResY,
    PointAGridResZ,
    PointAGridSpacing,
    PointAMaxPoints,
    // Point Group B
    PointGroupBStart,
    PointGroupBEnd,
    PointBEnabled,
    PointBSourceType,
    PointBSourceLayer,
    PointBGridResX,
    PointBGridResY,
    PointBGridSpacing,
    // Noise
    NoiseGroupStart,
    NoiseGroupEnd,
    NoiseEnabled,
    NoiseAmplitude,
    NoiseFrequency,
    NoiseSpeed,
    NoiseOctaves,
    NoiseAxisScale,
    // Lines
    LinesGroupStart,
    LinesGroupEnd,
    LinesEnabled,
    LinesMaxDistance,
    LinesWidth,
    LinesOpacityFalloff,
    LinesColor,
    // Mesh
    MeshGroupStart,
    MeshGroupEnd,
    MeshEnabled,
    MeshMaxEdge,
    MeshOpacity,
    MeshColor,
    // Beams
    BeamsGroupStart,
    BeamsGroupEnd,
    BeamsEnabled,
    BeamsSourceGroup,
    BeamsMaxDistance,
    BeamsWidth,
    BeamsColor,
    // Plexus Rendering
    PlexusRenderGroupStart,
    PlexusRenderGroupEnd,
    PlexusPointSize,
    PlexusPointColor,
    // ---- Added later (must be at end to preserve AE param order) ----
    SpriteSourceLayer,
    PathSampleDensity,
}

// ---- Plugin ----

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
struct PresetColor {
    alpha: u8,
    red: u8,
    green: u8,
    blue: u8,
}

impl From<ae::Pixel8> for PresetColor {
    fn from(value: ae::Pixel8) -> Self {
        Self {
            alpha: value.alpha,
            red: value.red,
            green: value.green,
            blue: value.blue,
        }
    }
}

impl From<PresetColor> for ae::Pixel8 {
    fn from(value: PresetColor) -> Self {
        Self {
            alpha: value.alpha,
            red: value.red,
            green: value.green,
            blue: value.blue,
        }
    }
}

/// Current preset format version. Fields not present in older presets are
/// filled in by `#[serde(default)]`, so forward-compat is automatic as long
/// as we never rename / repurpose a field.
const PRESET_VERSION: u32 = 3;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
struct PresetSnapshot {
    version: u32,
    name: String,
    // ---- Emitter ----
    emitter_type: i32,
    position_point: (f32, f32),
    position_z: f64,
    image_proxy_scale: i32,
    path_sample_density: f64,
    emitter_size_linked: bool,
    emitter_size_x: f64,
    emitter_size_y: f64,
    emitter_size_z: f64,
    birth_rate: f64,
    lifespan: f64,
    lifespan_var: f64,
    // ---- Motion ----
    speed: f64,
    speed_var: f64,
    direction_x: f64,
    direction_y: f64,
    direction_z: f64,
    spread: f64,
    initial_size: f64,
    size_var: f64,
    rotation: f64,
    rotation_speed: f64,
    // ---- Physics ----
    gravity_strength: f64,
    wind_x: f64,
    wind_y: f64,
    turb_strength: f64,
    turb_scale: f64,
    turb_speed: f64,
    air_resistance: f64,
    bounce_enabled: bool,
    bounce_damping: f64,
    // ---- Appearance ----
    color_mode: i32,
    color_start: PresetColor,
    color_end: PresetColor,
    opacity_curve_preset: i32,
    opacity_start: f64,
    opacity_mid_a: f64,
    opacity_mid_b: f64,
    opacity_end: f64,
    size_curve_preset: i32,
    size_life_start: f64,
    size_life_mid_a: f64,
    size_life_mid_b: f64,
    size_life_end: f64,
    // ---- Rendering ----
    shape: i32,
    image_color_mode: i32,
    image_fit_mode: i32,
    use_source_alpha: bool,
    source_premultiplied: bool,
    image_alpha_clip: f64,
    blend_mode: i32,
    motion_blur: f64,
    edge_softness: f64,
    dof_enabled: bool,
    dof_focal_dist: f64,
    dof_aperture: f64,
    size_multiplier: f64,
    composite_on_orig: bool,
    // ---- Child ----
    child_enabled: bool,
    child_count: i32,
    child_inherit_vel: f64,
    child_lifespan: f64,
    child_speed: f64,
    child_spread: f64,
    child_size_scale: f64,
    // ---- System ----
    seed: i32,
    // ---- Plexus ----
    plugin_mode: i32,
    point_a_enabled: bool,
    point_a_source_type: i32,
    point_a_grid_res_x: i32,
    point_a_grid_res_y: i32,
    point_a_grid_res_z: i32,
    point_a_grid_spacing: f64,
    point_a_max_points: i32,
    point_b_enabled: bool,
    point_b_source_type: i32,
    point_b_grid_res_x: i32,
    point_b_grid_res_y: i32,
    point_b_grid_spacing: f64,
    noise_enabled: bool,
    noise_amplitude: f64,
    noise_frequency: f64,
    noise_speed: f64,
    noise_octaves: i32,
    noise_axis_scale: i32,
    lines_enabled: bool,
    lines_max_distance: f64,
    lines_width: f64,
    lines_opacity_falloff: f64,
    lines_color: PresetColor,
    mesh_enabled: bool,
    mesh_max_edge: f64,
    mesh_opacity: f64,
    mesh_color: PresetColor,
    beams_enabled: bool,
    beams_source_group: i32,
    beams_max_distance: f64,
    beams_width: f64,
    beams_color: PresetColor,
    plexus_point_size: f64,
    plexus_point_color: PresetColor,
}

impl Default for PresetSnapshot {
    // IMPORTANT: every numeric default here MUST match the `f.set_default(…)`
    // of the corresponding parameter in `params_setup`. When the two diverge,
    // a preset file that omits a field (older-version preset, hand-edited
    // JSON, corrupted write) is loaded via `#[serde(default)]` and silently
    // rewrites that param to a wrong value on `apply_preset`. A few of these
    // were dangerously wrong in v2 (size_life_* at 35–100 vs. param range
    // 0.0–5.0; size_multiplier at 100.0 vs. range 0.01–10.0), so fixing them
    // is the main reason the preset format was bumped to v3.
    fn default() -> Self {
        Self {
            version: PRESET_VERSION,
            name: String::new(),
            emitter_type: 1,
            position_point: (50.0, 50.0),
            position_z: 0.0,
            image_proxy_scale: 3,          // param default: /8
            path_sample_density: 10.0,
            emitter_size_linked: true,
            emitter_size_x: 0.0,
            emitter_size_y: 0.0,
            emitter_size_z: 0.0,
            birth_rate: 180.0,
            lifespan: 1.6,
            lifespan_var: 0.15,
            speed: 240.0,
            speed_var: 0.2,
            direction_x: 0.0,
            direction_y: -1.0,
            direction_z: 0.0,
            spread: 18.0,
            initial_size: 9.0,
            size_var: 0.2,
            rotation: 0.0,
            rotation_speed: 0.0,
            gravity_strength: 160.0,
            wind_x: 0.0,
            wind_y: 0.0,
            turb_strength: 12.0,
            turb_scale: 0.75,
            turb_speed: 1.0,
            air_resistance: 0.3,
            bounce_enabled: false,
            bounce_damping: 0.5,
            color_mode: 2,                 // Gradient
            color_start: PresetColor { alpha: 255, red: 255, green: 255, blue: 255 },
            color_end: PresetColor { alpha: 255, red: 255, green: 255, blue: 255 },
            opacity_curve_preset: 3,       // Fade Out
            opacity_start: 100.0,
            opacity_mid_a: 90.0,
            opacity_mid_b: 45.0,
            opacity_end: 0.0,
            size_curve_preset: 3,          // Shrink
            size_life_start: 1.0,
            size_life_mid_a: 1.0,
            size_life_mid_b: 0.65,
            size_life_end: 0.35,
            shape: 1,
            image_color_mode: 1,
            image_fit_mode: 1,
            use_source_alpha: true,
            source_premultiplied: true,
            image_alpha_clip: 0.01,
            blend_mode: 1,                 // Normal
            motion_blur: 0.2,
            edge_softness: 0.0,
            dof_enabled: false,
            dof_focal_dist: 0.0,
            dof_aperture: 5.0,
            size_multiplier: 1.15,
            composite_on_orig: true,
            child_enabled: false,
            child_count: 3,
            child_inherit_vel: 0.65,
            child_lifespan: 0.5,
            child_speed: 80.0,
            child_spread: 110.0,
            child_size_scale: 0.4,
            seed: 12345,
            // ---- Plexus defaults (match params_setup) ----
            plugin_mode: 1,                // Particles
            point_a_enabled: true,
            point_a_source_type: 1,
            point_a_grid_res_x: 10,
            point_a_grid_res_y: 10,
            point_a_grid_res_z: 1,
            point_a_grid_spacing: 50.0,
            point_a_max_points: 5000,
            point_b_enabled: false,
            point_b_source_type: 1,
            point_b_grid_res_x: 10,
            point_b_grid_res_y: 10,
            point_b_grid_spacing: 50.0,
            noise_enabled: false,
            noise_amplitude: 50.0,
            noise_frequency: 0.01,
            noise_speed: 1.0,
            noise_octaves: 2,
            noise_axis_scale: 1,
            lines_enabled: true,
            lines_max_distance: 120.0,
            lines_width: 1.0,
            lines_opacity_falloff: 0.8,
            lines_color: PresetColor { alpha: 255, red: 255, green: 255, blue: 255 },
            mesh_enabled: false,
            mesh_max_edge: 150.0,
            mesh_opacity: 30.0,
            mesh_color: PresetColor { alpha: 255, red: 100, green: 150, blue: 255 },
            beams_enabled: false,
            beams_source_group: 1,
            beams_max_distance: 300.0,
            beams_width: 2.0,
            beams_color: PresetColor { alpha: 255, red: 100, green: 200, blue: 255 },
            plexus_point_size: 4.0,
            plexus_point_color: PresetColor { alpha: 255, red: 255, green: 255, blue: 255 },
        }
    }
}

#[derive(Default)]
struct Plugin;

ae::define_effect!(Plugin, (), Params);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct ImageCacheKey {
    instance_id: i32,
    time: i32,
    time_step: i32,
    time_scale: u32,
    proxy_divisor: u8,
    source_width: u32,
    source_height: u32,
    source_origin_x: i32,
    source_origin_y: i32,
    source_signature: u64,
    generation: u64,
}

type SpriteCacheMap = std::collections::HashMap<ImageCacheKey, Arc<SpriteImage>>;
type EmitterPointCacheMap = std::collections::HashMap<ImageCacheKey, Arc<Vec<glam::Vec3>>>;

static IMAGE_CACHE: OnceLock<RwLock<SpriteCacheMap>> = OnceLock::new();
static EMITTER_CACHE: OnceLock<RwLock<EmitterPointCacheMap>> = OnceLock::new();
static IMAGE_CACHE_GENERATION: AtomicU64 = AtomicU64::new(1);
/// Data collected in SmartPreRender (main thread) and passed to SmartRender (render thread).
/// This avoids calling AE param/camera APIs from render threads.
struct SmartRenderData {
    mode: i32,
    emitter: EmitterConfig,
    physics: PhysicsConfig,
    appearance: AppearanceConfig,
    child: ChildConfig,
    render_cfg: RenderConfig,
    seed: u64,
    camera_projection: Option<CameraProjection>,
    t: f32,
    dt: f32,
    plexus_cfg: PlexusConfig,
    expected_output_w: usize,
    expected_output_h: usize,
    expected_origin_x: i32,
    expected_origin_y: i32,
}

const DEBUG_MODULE: &str = "ONMK_ParticleLab";
const MAX_RENDER_BYTES: usize = 256 * 1024 * 1024;
const MAX_OUTPUT_PIXELS: i64 = 8_000_000; // ~2828x2828 max
const MIN_SMART_PRE_RENDER_MARGIN: i32 = 128;
const MAX_SMART_PRE_RENDER_MARGIN: i32 = 2048;
const RENDER_TIME_BUDGET_MS: u128 = 800;

#[cfg(windows)]
fn debug_log(level: &str, message: impl AsRef<str>) {
    let sanitize = |text: &str| text.replace(['\r', '\n', '\t'], " ");
    let line = format!(
        "{}\t{:?}\t{}\t{}\t{}\n",
        std::process::id(),
        std::thread::current().id(),
        sanitize(level),
        DEBUG_MODULE,
        sanitize(message.as_ref()),
    );

    if let Ok(mut pipe) = OpenOptions::new().write(true).open(r"\\.\pipe\AEExternalDebug") {
        let _ = pipe.write_all(line.as_bytes());
        let _ = pipe.flush();
    }

    // Also write to file for post-mortem debugging
    if let Ok(profile) = std::env::var("USERPROFILE") {
        let log_path = std::path::Path::new(&profile)
            .join("Documents")
            .join("ParticleLab")
            .join("debug.log");
        if let Some(parent) = log_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&log_path) {
            let _ = f.write_all(line.as_bytes());
        }
    }
}

#[cfg(not(windows))]
fn debug_log(_level: &str, _message: impl AsRef<str>) {}

fn debug_info(message: impl AsRef<str>) {
    debug_log("INFO", message);
}

fn debug_error(message: impl AsRef<str>) {
    debug_log("ERROR", message);
}

fn panic_payload_message(payload: Box<dyn std::any::Any + Send>) -> String {
    match payload.downcast::<String>() {
        Ok(message) => *message,
        Err(payload) => match payload.downcast::<&'static str>() {
            Ok(message) => (*message).to_string(),
            Err(_) => "non-string panic payload".to_string(),
        },
    }
}

fn command_name(cmd: &ae::Command) -> &'static str {
    match cmd {
        ae::Command::About => "About",
        ae::Command::GlobalSetup => "GlobalSetup",
        ae::Command::GlobalSetdown => "GlobalSetdown",
        ae::Command::ParamsSetup => "ParamsSetup",
        ae::Command::SequenceSetup => "SequenceSetup",
        ae::Command::SequenceSetdown => "SequenceSetdown",
        ae::Command::SequenceResetup => "SequenceResetup",
        ae::Command::SequenceFlatten => "SequenceFlatten",
        ae::Command::FrameSetup { .. } => "FrameSetup",
        ae::Command::FrameSetdown => "FrameSetdown",
        ae::Command::Render { .. } => "Render",
        ae::Command::SmartPreRender { .. } => "SmartPreRender",
        ae::Command::SmartRender { .. } => "SmartRender",
        ae::Command::SmartRenderGpu { .. } => "SmartRenderGpu",
        ae::Command::QueryDynamicFlags => "QueryDynamicFlags",
        ae::Command::UserChangedParam { .. } => "UserChangedParam",
        ae::Command::UpdateParamsUi => "UpdateParamsUi",
        _ => "Other",
    }
}

fn should_refresh_ui(param: Params) -> bool {
    matches!(
        param,
        Params::Shape
            | Params::EmitterType
            | Params::UseSourceAlpha
            | Params::EmitterSizeLinked
            | Params::RefreshImageCache
            | Params::OpacityCurvePreset
            | Params::SizeCurvePreset
            | Params::PluginMode
            | Params::PointASourceType
            | Params::PointBSourceType
    )
}

fn should_invalidate_source_cache(param: Params) -> bool {
    matches!(
        param,
        Params::ImageSourceLayer
            | Params::ImageProxyScale
            | Params::RefreshImageCache
    )
}

impl AdobePluginGlobal for Plugin {
    fn params_setup(
        &self,
        params: &mut ae::Parameters<Params>,
        _in_data: ae::InData,
        _out_data: ae::OutData,
    ) -> Result<(), ae::Error> {
        params.add_group(Params::PresetGroupStart, Params::PresetGroupEnd, "Presets", true, |params| {
            params.add(Params::SavePreset, "Save Preset", ae::ButtonDef::setup(|f| {
                f.set_label("Save");
            }))?;
            params.add(Params::LoadPreset, "Load Preset", ae::ButtonDef::setup(|f| {
                f.set_label("Load");
            }))?;
            params.add(Params::DeletePreset, "Delete Preset", ae::ButtonDef::setup(|f| {
                f.set_label("Delete");
            }))?;
            params.add(Params::OpenPresetFolder, "Open Folder", ae::ButtonDef::setup(|f| {
                f.set_label("Open Folder");
            }))?;
            Ok(())
        })?;

        params.add_group(Params::EmitterGroupStart, Params::EmitterGroupEnd, "Emitter", false, |params| {
            params.add_with_flags(Params::EmitterType, "Emitter Type", ae::PopupDef::setup(|f| {
                f.set_options(&["Point", "Box", "Sphere", "Grid", "Layer Alpha", "Path"]);
                f.set_default(1);
            }), ae::ParamFlag::SUPERVISE, ae::ParamUIFlags::empty())?;
            params.add(Params::PositionPoint, "Position", ae::PointDef::setup(|f| {
                f.set_default((50.0, 50.0));
                f.set_restrict_bounds(false);
            }))?;
            params.add(Params::PositionZ, "Position Z", ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(-10000.0); f.set_valid_max(10000.0);
                f.set_slider_min(-2000.0); f.set_slider_max(2000.0);
                f.set_default(0.0); f.set_precision(1);
            }))?;
            params.add(Params::ImageSourceLayer, "Emitter Source Layer", ae::LayerDef::new())?;
            params.add(Params::RefreshImageCache, "Refresh Image Cache", ae::ButtonDef::setup(|f| {
                f.set_label("Refresh");
            }))?;
            params.add(Params::PathSampleDensity, "Path Density", ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(1.0); f.set_valid_max(100.0);
                f.set_slider_min(1.0); f.set_slider_max(50.0);
                f.set_default(10.0); f.set_precision(0);
            }))?;
            params.add(Params::EmitterSizeLinked, "Uniform Box Size", ae::CheckBoxDef::setup(|f| {
                f.set_default(true); f.set_label("Enable");
            }))?;
            params.add(Params::EmitterSizeX, "Emitter Size X", ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0); f.set_valid_max(10000.0);
                f.set_slider_min(0.0); f.set_slider_max(2000.0);
                f.set_default(0.0); f.set_precision(1);
            }))?;
            params.add(Params::EmitterSizeY, "Emitter Size Y", ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0); f.set_valid_max(10000.0);
                f.set_slider_min(0.0); f.set_slider_max(2000.0);
                f.set_default(0.0); f.set_precision(1);
            }))?;
            params.add(Params::EmitterSizeZ, "Emitter Size Z", ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0); f.set_valid_max(10000.0);
                f.set_slider_min(0.0); f.set_slider_max(2000.0);
                f.set_default(0.0); f.set_precision(1);
            }))?;
            params.add(Params::BirthRate, "Birth Rate", ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0); f.set_valid_max(10000.0);
                f.set_slider_min(0.0); f.set_slider_max(1000.0);
                f.set_default(180.0); f.set_precision(1);
            }))?;
            params.add(Params::Lifespan, "Lifespan (sec)", ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.01); f.set_valid_max(30.0);
                f.set_slider_min(0.1); f.set_slider_max(10.0);
                f.set_default(1.6); f.set_precision(2);
            }))?;
            params.add(Params::LifespanVar, "Lifespan Variation", ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0); f.set_valid_max(1.0);
                f.set_slider_min(0.0); f.set_slider_max(1.0);
                f.set_default(0.15); f.set_precision(2);
            }))?;
            Ok(())
        })?;

        params.add_group(Params::MotionGroupStart, Params::MotionGroupEnd, "Motion", false, |params| {
            params.add(Params::Speed, "Speed", ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0); f.set_valid_max(5000.0);
                f.set_slider_min(0.0); f.set_slider_max(500.0);
                f.set_default(240.0); f.set_precision(1);
            }))?;
            params.add(Params::SpeedVar, "Speed Variation", ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0); f.set_valid_max(1.0);
                f.set_slider_min(0.0); f.set_slider_max(1.0);
                f.set_default(0.2); f.set_precision(2);
            }))?;
            params.add(Params::DirectionX, "Direction X", ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(-1.0); f.set_valid_max(1.0);
                f.set_slider_min(-1.0); f.set_slider_max(1.0);
                f.set_default(0.0); f.set_precision(2);
            }))?;
            params.add(Params::DirectionY, "Direction Y", ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(-1.0); f.set_valid_max(1.0);
                f.set_slider_min(-1.0); f.set_slider_max(1.0);
                f.set_default(-1.0); f.set_precision(2);
            }))?;
            params.add(Params::DirectionZ, "Direction Z", ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(-1.0); f.set_valid_max(1.0);
                f.set_slider_min(-1.0); f.set_slider_max(1.0);
                f.set_default(0.0); f.set_precision(2);
            }))?;
            params.add(Params::Spread, "Spread (deg)", ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0); f.set_valid_max(180.0);
                f.set_slider_min(0.0); f.set_slider_max(180.0);
                f.set_default(18.0); f.set_precision(1);
            }))?;
            params.add(Params::InitialSize, "Particle Size", ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.1); f.set_valid_max(500.0);
                f.set_slider_min(0.5); f.set_slider_max(100.0);
                f.set_default(9.0); f.set_precision(1);
            }))?;
            params.add(Params::SizeVar, "Size Variation", ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0); f.set_valid_max(1.0);
                f.set_slider_min(0.0); f.set_slider_max(1.0);
                f.set_default(0.2); f.set_precision(2);
            }))?;
            params.add(Params::Rotation, "Initial Rotation", ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0); f.set_valid_max(360.0);
                f.set_slider_min(0.0); f.set_slider_max(360.0);
                f.set_default(0.0); f.set_precision(1);
            }))?;
            params.add(Params::RotationSpeed, "Rotation Speed", ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(-1000.0); f.set_valid_max(1000.0);
                f.set_slider_min(-360.0); f.set_slider_max(360.0);
                f.set_default(0.0); f.set_precision(1);
            }))?;
            Ok(())
        })?;

        params.add_group(Params::PhysicsGroupStart, Params::PhysicsGroupEnd, "Physics", false, |params| {
            params.add_with_flags(
                Params::GravityStrength,
                "Gravity",
                ae::FloatSliderDef::setup(|f| {
                    f.set_valid_min(-2000.0); f.set_valid_max(2000.0);
                    f.set_slider_min(-500.0); f.set_slider_max(500.0);
                    f.set_default(160.0); f.set_precision(1);
                }),
                ae::ParamFlag::CANNOT_TIME_VARY,
                ae::ParamUIFlags::empty(),
            )?;
            params.add(Params::WindX, "Wind X", ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(-2000.0); f.set_valid_max(2000.0);
                f.set_slider_min(-500.0); f.set_slider_max(500.0);
                f.set_default(0.0); f.set_precision(1);
            }))?;
            params.add(Params::WindY, "Wind Y", ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(-2000.0); f.set_valid_max(2000.0);
                f.set_slider_min(-500.0); f.set_slider_max(500.0);
                f.set_default(0.0); f.set_precision(1);
            }))?;
            params.add(Params::TurbStrength, "Turbulence", ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0); f.set_valid_max(1000.0);
                f.set_slider_min(0.0); f.set_slider_max(200.0);
                f.set_default(12.0); f.set_precision(1);
            }))?;
            params.add(Params::TurbScale, "Turbulence Scale", ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.01); f.set_valid_max(100.0);
                f.set_slider_min(0.1); f.set_slider_max(10.0);
                f.set_default(0.75); f.set_precision(2);
            }))?;
            params.add(Params::TurbSpeed, "Turbulence Speed", ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0); f.set_valid_max(10.0);
                f.set_slider_min(0.0); f.set_slider_max(5.0);
                f.set_default(1.0); f.set_precision(2);
            }))?;
            params.add(Params::AirResistance, "Air Resistance", ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0); f.set_valid_max(20.0);
                f.set_slider_min(0.0); f.set_slider_max(5.0);
                f.set_default(0.3); f.set_precision(2);
            }))?;
            params.add(Params::BounceEnabled, "Bounce", ae::CheckBoxDef::setup(|f| {
                f.set_default(false); f.set_label("Enable");
            }))?;
            params.add(Params::BounceDamping, "Bounce Damping", ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0); f.set_valid_max(1.0);
                f.set_slider_min(0.0); f.set_slider_max(1.0);
                f.set_default(0.5); f.set_precision(2);
            }))?;
            Ok(())
        })?;

        params.add_group(Params::AppearanceGroupStart, Params::AppearanceGroupEnd, "Appearance", false, |params| {
            params.add(Params::ColorMode, "Color Mode", ae::PopupDef::setup(|f| {
                f.set_options(&["Single", "Gradient"]);
                f.set_default(2);
            }))?;
            params.add(Params::ColorStart, "Color Start", ae::ColorDef::setup(|f| {
                f.set_default(ae::Pixel8 { alpha: 255, red: 255, green: 255, blue: 255 });
            }))?;
            params.add(Params::ColorEnd, "Color End", ae::ColorDef::setup(|f| {
                f.set_default(ae::Pixel8 { alpha: 255, red: 255, green: 255, blue: 255 });
            }))?;
            params.add_with_flags(Params::OpacityCurvePreset, "Opacity Curve", ae::PopupDef::setup(|f| {
                f.set_options(&["Custom", "Constant", "Fade Out", "Fade In-Out", "Ease Out", "Quick Fade"]);
                f.set_default(3); // Fade Out
            }), ae::ParamFlag::SUPERVISE, ae::ParamUIFlags::empty())?;
            params.add(Params::OpacityStart, "Opacity Start", ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0); f.set_valid_max(100.0);
                f.set_slider_min(0.0); f.set_slider_max(100.0);
                f.set_default(100.0); f.set_precision(1);
            }))?;
            params.add(Params::OpacityMidA, "Opacity 33%", ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0); f.set_valid_max(100.0);
                f.set_slider_min(0.0); f.set_slider_max(100.0);
                f.set_default(90.0); f.set_precision(1);
            }))?;
            params.add(Params::OpacityMidB, "Opacity 66%", ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0); f.set_valid_max(100.0);
                f.set_slider_min(0.0); f.set_slider_max(100.0);
                f.set_default(45.0); f.set_precision(1);
            }))?;
            params.add(Params::OpacityEnd, "Opacity End", ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0); f.set_valid_max(100.0);
                f.set_slider_min(0.0); f.set_slider_max(100.0);
                f.set_default(0.0); f.set_precision(1);
            }))?;
            params.add_with_flags(Params::SizeCurvePreset, "Size Curve", ae::PopupDef::setup(|f| {
                f.set_options(&["Custom", "Constant", "Shrink", "Grow-Shrink", "Grow", "Pop-Shrink"]);
                f.set_default(3); // Shrink
            }), ae::ParamFlag::SUPERVISE, ae::ParamUIFlags::empty())?;
            params.add(Params::SizeLifeStart, "Size 0%", ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0); f.set_valid_max(5.0);
                f.set_slider_min(0.0); f.set_slider_max(3.0);
                f.set_default(1.0); f.set_precision(2);
            }))?;
            params.add(Params::SizeLifeMidA, "Size 33%", ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0); f.set_valid_max(5.0);
                f.set_slider_min(0.0); f.set_slider_max(3.0);
                f.set_default(1.0); f.set_precision(2);
            }))?;
            params.add(Params::SizeLifeMidB, "Size 66%", ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0); f.set_valid_max(5.0);
                f.set_slider_min(0.0); f.set_slider_max(3.0);
                f.set_default(0.65); f.set_precision(2);
            }))?;
            params.add(Params::SizeLifeEnd, "Size 100%", ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0); f.set_valid_max(5.0);
                f.set_slider_min(0.0); f.set_slider_max(3.0);
                f.set_default(0.35); f.set_precision(2);
            }))?;
            Ok(())
        })?;

        params.add_group(Params::RenderingGroupStart, Params::RenderingGroupEnd, "Rendering", false, |params| {
            params.add_with_flags(Params::Shape, "Shape", ae::PopupDef::setup(|f| {
                f.set_options(&["Circle", "Square", "Triangle", "Star", "Line", "Image"]);
                f.set_default(1);
            }), ae::ParamFlag::SUPERVISE, ae::ParamUIFlags::empty())?;
            params.add(Params::SpriteSourceLayer, "Sprite Source", ae::LayerDef::new())?;
            params.add(Params::ImageProxyScale, "Image Proxy", ae::PopupDef::setup(|f| {
                f.set_options(&["Full", "/2", "/4", "/8"]);
                f.set_default(3);
            }))?;
            params.add(Params::EdgeSoftness, "Edge Softness", ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0); f.set_valid_max(1.0);
                f.set_slider_min(0.0); f.set_slider_max(1.0);
                f.set_default(0.0); f.set_precision(2);
            }))?;
            params.add(Params::ImageColorMode, "Image Color", ae::PopupDef::setup(|f| {
                f.set_options(&["Tint", "Source"]);
                f.set_default(1);
            }))?;
            params.add(Params::ImageFitMode, "Image Fit", ae::PopupDef::setup(|f| {
                f.set_options(&["Contain", "Stretch"]);
                f.set_default(1);
            }))?;
            params.add(Params::UseSourceAlpha, "Use Source Alpha", ae::CheckBoxDef::setup(|f| {
                f.set_default(true); f.set_label("Enable");
            }))?;
            params.add(Params::SourcePremultiplied, "Source Premultiplied", ae::CheckBoxDef::setup(|f| {
                f.set_default(true); f.set_label("Enable");
            }))?;
            params.add(Params::ImageAlphaClip, "Alpha Clip", ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0); f.set_valid_max(1.0);
                f.set_slider_min(0.0); f.set_slider_max(1.0);
                f.set_default(0.01); f.set_precision(2);
            }))?;
            params.add(Params::BlendModeParam, "Blend Mode", ae::PopupDef::setup(|f| {
                f.set_options(&["Normal", "Add", "Screen"]);
                f.set_default(1);
            }))?;
            params.add(Params::MotionBlur, "Motion Blur", ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0); f.set_valid_max(1.0);
                f.set_slider_min(0.0); f.set_slider_max(1.0);
                f.set_default(0.2); f.set_precision(2);
            }))?;
            params.add(Params::DOFEnabled, "Depth of Field", ae::CheckBoxDef::setup(|f| {
                f.set_default(false); f.set_label("Enable");
            }))?;
            params.add(Params::DOFFocalDist, "DOF Focal Distance", ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0); f.set_valid_max(10000.0);
                f.set_slider_min(0.0); f.set_slider_max(1000.0);
                f.set_default(0.0); f.set_precision(1);
            }))?;
            params.add(Params::DOFAperture, "DOF Aperture", ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0); f.set_valid_max(100.0);
                f.set_slider_min(0.0); f.set_slider_max(50.0);
                f.set_default(5.0); f.set_precision(1);
            }))?;
            params.add(Params::SizeMultiplier, "Size Multiplier", ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.01); f.set_valid_max(10.0);
                f.set_slider_min(0.1); f.set_slider_max(5.0);
                f.set_default(1.15); f.set_precision(2);
            }))?;
            params.add(Params::CompositeOnOrig, "Composite on Original", ae::CheckBoxDef::setup(|f| {
                f.set_default(true); f.set_label("Enable");
            }))?;
            Ok(())
        })?;

        params.add_group(Params::ChildGroupStart, Params::ChildGroupEnd, "Child Particles", true, |params| {
            params.add(Params::ChildEnabled, "Child Particles", ae::CheckBoxDef::setup(|f| {
                f.set_default(false); f.set_label("Enable");
            }))?;
            params.add(Params::ChildCount, "Child Count", ae::SliderDef::setup(|f| {
                f.set_valid_min(0); f.set_valid_max(20);
                f.set_slider_min(0); f.set_slider_max(10);
                f.set_default(3);
            }))?;
            params.add(Params::ChildInheritVel, "Child Inherit Velocity", ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0); f.set_valid_max(1.0);
                f.set_slider_min(0.0); f.set_slider_max(1.0);
                f.set_default(0.65); f.set_precision(2);
            }))?;
            params.add(Params::ChildLifespan, "Child Lifespan", ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.01); f.set_valid_max(10.0);
                f.set_slider_min(0.1); f.set_slider_max(5.0);
                f.set_default(0.5); f.set_precision(2);
            }))?;
            params.add(Params::ChildSpeed, "Child Speed", ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0); f.set_valid_max(10000.0);
                f.set_slider_min(0.0); f.set_slider_max(2000.0);
                f.set_default(80.0); f.set_precision(1);
            }))?;
            params.add(Params::ChildSpread, "Child Spread (deg)", ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0); f.set_valid_max(180.0);
                f.set_slider_min(0.0); f.set_slider_max(180.0);
                f.set_default(110.0); f.set_precision(1);
            }))?;
            params.add(Params::ChildSizeScale, "Child Size Scale", ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.01); f.set_valid_max(5.0);
                f.set_slider_min(0.1); f.set_slider_max(2.0);
                f.set_default(0.4); f.set_precision(2);
            }))?;
            Ok(())
        })?;

        params.add_group(Params::SystemGroupStart, Params::SystemGroupEnd, "System", true, |params| {
            params.add(Params::Seed, "Random Seed", ae::SliderDef::setup(|f| {
                f.set_valid_min(0); f.set_valid_max(99999);
                f.set_slider_min(0); f.set_slider_max(99999);
                f.set_default(12345);
            }))?;
            Ok(())
        })?;

        // ---- Plexus ----
        params.add_with_flags(Params::PluginMode, "Mode", ae::PopupDef::setup(|f| {
            f.set_options(&["Particles", "Plexus", "Combined"]);
            f.set_default(1);
        }), ae::ParamFlag::SUPERVISE, ae::ParamUIFlags::empty())?;

        params.add_group(Params::PlexusGroupStart, Params::PlexusGroupEnd, "Plexus", true, |params| {
            // Point Group A
            params.add_group(Params::PointGroupAStart, Params::PointGroupAEnd, "Point Group A", false, |params| {
                params.add(Params::PointAEnabled, "Enable", ae::CheckBoxDef::setup(|f| {
                    f.set_default(true); f.set_label("Enable");
                }))?;
                params.add_with_flags(Params::PointASourceType, "Source Type", ae::PopupDef::setup(|f| {
                    f.set_options(&["Grid", "Layer", "OBJ File", "AE Lights", "Particles"]);
                    f.set_default(1);
                }), ae::ParamFlag::SUPERVISE, ae::ParamUIFlags::empty())?;
                params.add(Params::PointASourceLayer, "Source Layer", ae::LayerDef::new())?;
                params.add(Params::PointAGridResX, "Grid Res X", ae::SliderDef::setup(|f| {
                    f.set_valid_min(2); f.set_valid_max(200);
                    f.set_slider_min(2); f.set_slider_max(100);
                    f.set_default(10);
                }))?;
                params.add(Params::PointAGridResY, "Grid Res Y", ae::SliderDef::setup(|f| {
                    f.set_valid_min(2); f.set_valid_max(200);
                    f.set_slider_min(2); f.set_slider_max(100);
                    f.set_default(10);
                }))?;
                params.add(Params::PointAGridResZ, "Grid Res Z", ae::SliderDef::setup(|f| {
                    f.set_valid_min(1); f.set_valid_max(100);
                    f.set_slider_min(1); f.set_slider_max(50);
                    f.set_default(1);
                }))?;
                params.add(Params::PointAGridSpacing, "Grid Spacing", ae::FloatSliderDef::setup(|f| {
                    f.set_valid_min(1.0); f.set_valid_max(500.0);
                    f.set_slider_min(5.0); f.set_slider_max(200.0);
                    f.set_default(50.0); f.set_precision(1);
                }))?;
                params.add(Params::PointAMaxPoints, "Max Points", ae::SliderDef::setup(|f| {
                    f.set_valid_min(10); f.set_valid_max(10000);
                    f.set_slider_min(100); f.set_slider_max(10000);
                    f.set_default(5000);
                }))?;
                Ok(())
            })?;

            // Point Group B
            params.add_group(Params::PointGroupBStart, Params::PointGroupBEnd, "Point Group B", true, |params| {
                params.add(Params::PointBEnabled, "Enable", ae::CheckBoxDef::setup(|f| {
                    f.set_default(false); f.set_label("Enable");
                }))?;
                params.add_with_flags(Params::PointBSourceType, "Source Type", ae::PopupDef::setup(|f| {
                    f.set_options(&["Grid", "Layer", "OBJ File", "AE Lights"]);
                    f.set_default(1);
                }), ae::ParamFlag::SUPERVISE, ae::ParamUIFlags::empty())?;
                params.add(Params::PointBSourceLayer, "Source Layer", ae::LayerDef::new())?;
                params.add(Params::PointBGridResX, "Grid Res X", ae::SliderDef::setup(|f| {
                    f.set_valid_min(2); f.set_valid_max(200);
                    f.set_slider_min(2); f.set_slider_max(100);
                    f.set_default(10);
                }))?;
                params.add(Params::PointBGridResY, "Grid Res Y", ae::SliderDef::setup(|f| {
                    f.set_valid_min(2); f.set_valid_max(200);
                    f.set_slider_min(2); f.set_slider_max(100);
                    f.set_default(10);
                }))?;
                params.add(Params::PointBGridSpacing, "Grid Spacing", ae::FloatSliderDef::setup(|f| {
                    f.set_valid_min(1.0); f.set_valid_max(500.0);
                    f.set_slider_min(5.0); f.set_slider_max(200.0);
                    f.set_default(50.0); f.set_precision(1);
                }))?;
                Ok(())
            })?;

            // Noise
            params.add_group(Params::NoiseGroupStart, Params::NoiseGroupEnd, "Noise Displacement", true, |params| {
                params.add(Params::NoiseEnabled, "Enable", ae::CheckBoxDef::setup(|f| {
                    f.set_default(false); f.set_label("Enable");
                }))?;
                params.add(Params::NoiseAmplitude, "Amplitude", ae::FloatSliderDef::setup(|f| {
                    f.set_valid_min(0.0); f.set_valid_max(1000.0);
                    f.set_slider_min(0.0); f.set_slider_max(300.0);
                    f.set_default(50.0); f.set_precision(1);
                }))?;
                params.add(Params::NoiseFrequency, "Frequency", ae::FloatSliderDef::setup(|f| {
                    f.set_valid_min(0.001); f.set_valid_max(10.0);
                    f.set_slider_min(0.01); f.set_slider_max(2.0);
                    f.set_default(0.01); f.set_precision(3);
                }))?;
                params.add(Params::NoiseSpeed, "Speed", ae::FloatSliderDef::setup(|f| {
                    f.set_valid_min(0.0); f.set_valid_max(10.0);
                    f.set_slider_min(0.0); f.set_slider_max(5.0);
                    f.set_default(1.0); f.set_precision(2);
                }))?;
                params.add(Params::NoiseOctaves, "Octaves", ae::SliderDef::setup(|f| {
                    f.set_valid_min(1); f.set_valid_max(6);
                    f.set_slider_min(1); f.set_slider_max(6);
                    f.set_default(2);
                }))?;
                params.add(Params::NoiseAxisScale, "Mode", ae::PopupDef::setup(|f| {
                    f.set_options(&["Uniform", "Per-Axis"]);
                    f.set_default(1);
                }))?;
                Ok(())
            })?;

            // Lines
            params.add_group(Params::LinesGroupStart, Params::LinesGroupEnd, "Lines", true, |params| {
                params.add(Params::LinesEnabled, "Enable", ae::CheckBoxDef::setup(|f| {
                    f.set_default(true); f.set_label("Enable");
                }))?;
                params.add(Params::LinesMaxDistance, "Max Distance", ae::FloatSliderDef::setup(|f| {
                    f.set_valid_min(1.0); f.set_valid_max(2000.0);
                    f.set_slider_min(10.0); f.set_slider_max(500.0);
                    f.set_default(120.0); f.set_precision(1);
                }))?;
                params.add(Params::LinesWidth, "Line Width", ae::FloatSliderDef::setup(|f| {
                    f.set_valid_min(0.5); f.set_valid_max(20.0);
                    f.set_slider_min(0.5); f.set_slider_max(10.0);
                    f.set_default(1.0); f.set_precision(1);
                }))?;
                params.add(Params::LinesOpacityFalloff, "Opacity Falloff", ae::FloatSliderDef::setup(|f| {
                    f.set_valid_min(0.0); f.set_valid_max(1.0);
                    f.set_slider_min(0.0); f.set_slider_max(1.0);
                    f.set_default(0.8); f.set_precision(2);
                }))?;
                params.add(Params::LinesColor, "Line Color", ae::ColorDef::setup(|f| {
                    f.set_default(ae::Pixel8 { alpha: 255, red: 255, green: 255, blue: 255 });
                }))?;
                Ok(())
            })?;

            // Mesh
            params.add_group(Params::MeshGroupStart, Params::MeshGroupEnd, "Mesh", true, |params| {
                params.add(Params::MeshEnabled, "Enable", ae::CheckBoxDef::setup(|f| {
                    f.set_default(false); f.set_label("Enable");
                }))?;
                params.add(Params::MeshMaxEdge, "Max Edge Length", ae::FloatSliderDef::setup(|f| {
                    f.set_valid_min(1.0); f.set_valid_max(2000.0);
                    f.set_slider_min(10.0); f.set_slider_max(500.0);
                    f.set_default(150.0); f.set_precision(1);
                }))?;
                params.add(Params::MeshOpacity, "Mesh Opacity", ae::FloatSliderDef::setup(|f| {
                    f.set_valid_min(0.0); f.set_valid_max(100.0);
                    f.set_slider_min(0.0); f.set_slider_max(100.0);
                    f.set_default(30.0); f.set_precision(1);
                }))?;
                params.add(Params::MeshColor, "Mesh Color", ae::ColorDef::setup(|f| {
                    f.set_default(ae::Pixel8 { alpha: 255, red: 100, green: 150, blue: 255 });
                }))?;
                Ok(())
            })?;

            // Beams
            params.add_group(Params::BeamsGroupStart, Params::BeamsGroupEnd, "Beams", true, |params| {
                params.add(Params::BeamsEnabled, "Enable", ae::CheckBoxDef::setup(|f| {
                    f.set_default(false); f.set_label("Enable");
                }))?;
                params.add(Params::BeamsSourceGroup, "Direction", ae::PopupDef::setup(|f| {
                    f.set_options(&["A -> B", "B -> A", "A -> A", "B -> B"]);
                    f.set_default(1);
                }))?;
                params.add(Params::BeamsMaxDistance, "Max Distance", ae::FloatSliderDef::setup(|f| {
                    f.set_valid_min(1.0); f.set_valid_max(5000.0);
                    f.set_slider_min(10.0); f.set_slider_max(1000.0);
                    f.set_default(300.0); f.set_precision(1);
                }))?;
                params.add(Params::BeamsWidth, "Beam Width", ae::FloatSliderDef::setup(|f| {
                    f.set_valid_min(0.5); f.set_valid_max(30.0);
                    f.set_slider_min(0.5); f.set_slider_max(15.0);
                    f.set_default(2.0); f.set_precision(1);
                }))?;
                params.add(Params::BeamsColor, "Beam Color", ae::ColorDef::setup(|f| {
                    f.set_default(ae::Pixel8 { alpha: 255, red: 100, green: 200, blue: 255 });
                }))?;
                Ok(())
            })?;

            // Plexus Rendering
            params.add_group(Params::PlexusRenderGroupStart, Params::PlexusRenderGroupEnd, "Point Rendering", false, |params| {
                params.add(Params::PlexusPointSize, "Point Size", ae::FloatSliderDef::setup(|f| {
                    f.set_valid_min(0.5); f.set_valid_max(50.0);
                    f.set_slider_min(1.0); f.set_slider_max(20.0);
                    f.set_default(4.0); f.set_precision(1);
                }))?;
                params.add(Params::PlexusPointColor, "Point Color", ae::ColorDef::setup(|f| {
                    f.set_default(ae::Pixel8 { alpha: 255, red: 255, green: 255, blue: 255 });
                }))?;
                Ok(())
            })?;

            Ok(())
        })?;

        Ok(())
    }

    fn handle_command(
        &mut self,
        cmd: ae::Command,
        in_data: ae::InData,
        mut out_data: ae::OutData,
        params: &mut ae::Parameters<Params>,
    ) -> Result<(), ae::Error> {
        let cmd_name = command_name(&cmd);
        debug_info(format!(
            "{} begin current_time={} time_step={} time_scale={}",
            cmd_name,
            in_data.current_time(),
            in_data.time_step(),
            in_data.time_scale()
        ));

        let result = match panic::catch_unwind(AssertUnwindSafe(|| -> Result<(), ae::Error> {
            match cmd {
                ae::Command::About => {
                    out_data.set_return_msg(
                        "ONMK_ParticleLab v2.0\rParticle system plugin.\rEmitters, physics, child particles.\rWritten in Rust.",
                    );
                    Ok(())
                }

                ae::Command::GlobalSetup => {
                    out_data.set_out_flag(ae::OutFlags::IExpandBuffer, true);
                    out_data.set_out_flag(ae::OutFlags::SendUpdateParamsUi, true);
                    out_data.set_out_flag(ae::OutFlags::NonParamVary, true);
                    out_data.set_out_flag2(ae::OutFlags2::ParamGroupStartCollapsedFlag, true);
                    out_data.set_out_flag2(ae::OutFlags2::IUse3DCamera, true);
                    out_data.set_out_flag2(ae::OutFlags2::SupportsSmartRender, true);
                    Ok(())
                }

                ae::Command::Render { in_layer, mut out_layer } => {
                    render_particles(params, &in_data, &in_layer, &mut out_layer)
                }

                ae::Command::SmartPreRender { mut extra } => {
                    let req = extra.output_request();
                    let cb = extra.callbacks();
                    let in_result = cb.checkout_layer(
                        0, 0, &req,
                        in_data.current_time(), in_data.time_step(), in_data.time_scale(),
                    )?;

                    let in_rect: ae::Rect = in_result.result_rect.into();
                    let margin = estimate_render_margin(params)?;
                    let expanded_input = ae::Rect {
                        left:   in_rect.left.saturating_sub(margin),
                        top:    in_rect.top.saturating_sub(margin),
                        right:  in_rect.right.saturating_add(margin),
                        bottom: in_rect.bottom.saturating_add(margin),
                    };
                    let emitter_rect = estimated_emitter_bounds(params, margin)?;
                    let expanded = clamp_rect_to_pixel_budget(
                        union_rect(expanded_input, emitter_rect),
                        MAX_OUTPUT_PIXELS,
                    );
                    extra.set_result_rect(expanded);
                    extra.set_max_result_rect(expanded);
                    extra.set_returns_extra_pixels(true);

                    // Collect ALL data on the PreRender thread (safe for AE API calls).
                    // SmartRender will NOT call any AE param/camera APIs.
                    let mode = get_plugin_mode(params)?;
                    let (mut emitter, physics, appearance, child, mut render_cfg, seed) = extract_configs(params)?;
                    populate_layer_alpha_emitter(params, &in_data, &mut emitter)?;
                    populate_path_emitter(params, &in_data, &mut emitter)?;
                    populate_image_sprite(params, &in_data, &mut render_cfg)?;
                    let camera_projection = try_get_camera_projection(&in_data);
                    let t = current_time_sec(&in_data);
                    let dt = time_step_sec(&in_data);

                    let plexus_cfg = if mode == 2 || mode == 3 {
                        extract_plexus_configs(params)?
                    } else {
                        PlexusConfig::default()
                    };

                    let expected_output_w = (expanded.right - expanded.left).max(1) as usize;
                    let expected_output_h = (expanded.bottom - expanded.top).max(1) as usize;
                    let expected_origin_x = expanded.left;
                    let expected_origin_y = expanded.top;
                    extra.set_pre_render_data(SmartRenderData {
                        mode, emitter, physics, appearance, child, render_cfg, seed,
                        camera_projection, t, dt, plexus_cfg,
                        expected_output_w, expected_output_h,
                        expected_origin_x, expected_origin_y,
                    });

                    debug_info(format!(
                        "SmartPreRender in_rect left={} top={} right={} bottom={} expanded_margin={} expanded_rect=({}, {}, {}, {})",
                        in_rect.left, in_rect.top, in_rect.right, in_rect.bottom,
                        margin, expanded.left, expanded.top, expanded.right, expanded.bottom
                    ));
                    Ok(())
                }

                ae::Command::SmartRender { extra } => {
                    let render_start = Instant::now();
                    match smart_render_particles(render_start, &in_data, &extra) {
                        Ok(()) => Ok(()),
                        // Any error path — cancellation OR a real failure —
                        // must be surfaced to AE as `InterruptCancel`.
                        //
                        // Background: AE's SmartFX contract is "if you return
                        // Ok(()) you promised to have populated the output
                        // buffer." Whenever we bail before writing output
                        // (whether because the user cancelled, a checkout
                        // failed, the rect was too big, etc.) returning
                        // `Ok(())` causes AE to cache an uninitialized /
                        // partial buffer as that frame's rendered output,
                        // which poisons the frame cache. Returning
                        // `InterruptCancel` tells AE "this frame was not
                        // produced" so it will not cache the result and will
                        // simply re-request the frame. Non-cancel errors are
                        // still logged; `InterruptCancel` is suppressed by
                        // the framework (no error dialog shown to the user).
                        Err(ae::Error::InterruptCancel) => {
                            debug_info("SmartRender interrupted (cancel)");
                            Err(ae::Error::InterruptCancel)
                        }
                        Err(e) => {
                            debug_error(format!(
                                "SmartRender error: {:?} — signalling InterruptCancel to avoid cache poisoning",
                                e
                            ));
                            Err(ae::Error::InterruptCancel)
                        }
                    }
                }

                ae::Command::SmartRenderGpu { .. } => {
                    debug_error("SmartRenderGpu requested but GPU render path is not implemented");
                    Err(ae::Error::Generic)
                }

                ae::Command::UserChangedParam { param_index } => {
                    let changed = params.type_at(param_index);
                    if matches!(
                        changed,
                        Params::SavePreset | Params::LoadPreset | Params::DeletePreset | Params::OpenPresetFolder
                    ) {
                        handle_preset_command(changed, params, &mut out_data)?;
                    }
                    if matches!(changed, Params::EmitterSizeX | Params::EmitterSizeY | Params::EmitterSizeZ | Params::EmitterSizeLinked) {
                        sync_box_size_axes(params, changed)?;
                    }
                    if changed == Params::OpacityCurvePreset {
                        apply_opacity_preset(params)?;
                    }
                    if changed == Params::SizeCurvePreset {
                        apply_size_preset(params)?;
                    }
                    out_data.set_force_rerender();
                    if should_invalidate_source_cache(changed) {
                        invalidate_image_cache();
                    }
                    if changed == Params::RefreshImageCache {
                        out_data.set_return_msg("ParticleLab image cache refreshed.");
                        debug_info("RefreshImageCache button pressed");
                    }

                    if should_refresh_ui(changed) {
                        out_data.set_out_flag(ae::OutFlags::RefreshUi, true);
                    }
                    debug_info(format!("UserChangedParam index={}", param_index));
                    Ok(())
                }

                ae::Command::UpdateParamsUi => {
                    update_shape_dependent_ui(params)?;
                    Ok(())
                }

                ae::Command::Event { .. } => {
                    Ok(())
                }

                _ => Ok(()),
            }
        })) {
            Ok(result) => result,
            Err(payload) => {
                debug_error(format!("{} panicked: {}", cmd_name, panic_payload_message(payload)));
                Err(ae::Error::Generic)
            }
        };

        match &result {
            Ok(()) => debug_info(format!("{} end ok", cmd_name)),
            Err(err) => debug_error(format!("{} failed: {:?}", cmd_name, err)),
        }

        result
    }
}

// ---- Extract all parameters into configs ----

#[cfg(windows)]
fn preset_root_dir() -> Result<PathBuf, ae::Error> {
    let userprofile = std::env::var("USERPROFILE").map_err(|_| ae::Error::Generic)?;
    Ok(PathBuf::from(userprofile).join("Documents").join("ParticleLab").join("presets"))
}

#[cfg(not(windows))]
fn preset_root_dir() -> Result<std::path::PathBuf, ae::Error> {
    Err(ae::Error::Generic)
}

fn latest_preset_file() -> Result<Option<PathBuf>, ae::Error> {
    let dir = preset_root_dir()?;
    if !dir.is_dir() {
        return Ok(None);
    }
    let mut latest: Option<(PathBuf, std::time::SystemTime)> = None;
    let entries = fs::read_dir(&dir).map_err(|_| ae::Error::Generic)?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().map_or(false, |e| e == "json") {
            if let Ok(meta) = path.metadata() {
                if let Ok(modified) = meta.modified() {
                    if latest.as_ref().map_or(true, |(_, t)| modified > *t) {
                        latest = Some((path, modified));
                    }
                }
            }
        }
    }
    Ok(latest.map(|(p, _)| p))
}

fn generate_preset_path() -> Result<PathBuf, ae::Error> {
    let dir = preset_root_dir()?;
    fs::create_dir_all(&dir).map_err(|_| ae::Error::Generic)?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| ae::Error::Generic)?;
    let secs = now.as_secs();
    // Format as YYYYMMDD_HHMMSS using UTC
    let s = secs;
    let days = s / 86400;
    let time_of_day = s % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;
    // Simple date calculation from epoch days
    let (year, month, day) = epoch_days_to_date(days as i64);
    let name = format!("preset_{:04}{:02}{:02}_{:02}{:02}{:02}.json", year, month, day, hours, minutes, seconds);
    Ok(dir.join(name))
}

fn epoch_days_to_date(days: i64) -> (i64, u32, u32) {
    // Civil from days algorithm (Howard Hinnant)
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

fn read_latest_preset() -> Result<Option<PresetSnapshot>, ae::Error> {
    if let Some(path) = latest_preset_file()? {
        let contents = fs::read_to_string(&path).map_err(|_| ae::Error::Generic)?;
        let snapshot = serde_json::from_str::<PresetSnapshot>(&contents).map_err(|_| ae::Error::Generic)?;
        Ok(Some(snapshot))
    } else {
        Ok(None)
    }
}

fn write_preset(snapshot: &PresetSnapshot) -> Result<PathBuf, ae::Error> {
    let path = generate_preset_path()?;
    let json = serde_json::to_string_pretty(snapshot).map_err(|_| ae::Error::Generic)?;
    fs::write(&path, json).map_err(|_| ae::Error::Generic)?;
    Ok(path)
}

fn delete_latest_preset() -> Result<Option<PathBuf>, ae::Error> {
    if let Some(path) = latest_preset_file()? {
        fs::remove_file(&path).map_err(|_| ae::Error::Generic)?;
        Ok(Some(path))
    } else {
        Ok(None)
    }
}

#[cfg(windows)]
fn open_preset_folder() -> Result<(), ae::Error> {
    let dir = preset_root_dir()?;
    fs::create_dir_all(&dir).map_err(|_| ae::Error::Generic)?;
    std::process::Command::new("explorer.exe")
        .arg(dir.as_os_str())
        .spawn()
        .map_err(|_| ae::Error::Generic)?;
    Ok(())
}

#[cfg(not(windows))]
fn open_preset_folder() -> Result<(), ae::Error> {
    Err(ae::Error::Generic)
}

fn handle_preset_command(
    changed: Params,
    params: &ae::Parameters<Params>,
    out_data: &mut ae::OutData,
) -> Result<(), ae::Error> {
    match changed {
        Params::SavePreset => {
            let snapshot = capture_preset(params)?;
            let path = write_preset(&snapshot)?;
            out_data.set_return_msg(&format!(
                "ParticleLab preset saved: {}",
                path.file_name().unwrap_or_default().to_string_lossy()
            ));
        }
        Params::LoadPreset => {
            if let Some(snapshot) = read_latest_preset()? {
                apply_preset(params, &snapshot)?;
                invalidate_image_cache();
                let label = if snapshot.name.is_empty() { "latest".to_string() } else { snapshot.name.clone() };
                out_data.set_return_msg(&format!("ParticleLab preset loaded: {}", label));
            } else {
                out_data.set_return_msg("No preset files found in Documents/ParticleLab/presets/");
            }
        }
        Params::DeletePreset => {
            if let Some(path) = delete_latest_preset()? {
                out_data.set_return_msg(&format!(
                    "Deleted: {}",
                    path.file_name().unwrap_or_default().to_string_lossy()
                ));
            } else {
                out_data.set_return_msg("No preset files to delete.");
            }
        }
        Params::OpenPresetFolder => {
            open_preset_folder()?;
        }
        _ => {}
    }
    Ok(())
}

fn capture_preset(params: &ae::Parameters<Params>) -> Result<PresetSnapshot, ae::Error> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let s = now;
    let days = s / 86400;
    let time_of_day = s % 86400;
    let (year, month, day) = epoch_days_to_date(days as i64);
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;
    let name = format!("preset_{:04}{:02}{:02}_{:02}{:02}{:02}", year, month, day, hours, minutes, seconds);

    Ok(PresetSnapshot {
        version: PRESET_VERSION,
        name,
        emitter_type: params.get(Params::EmitterType)?.as_popup()?.value(),
        position_point: params.get(Params::PositionPoint)?.as_point()?.value(),
        position_z: params.get(Params::PositionZ)?.as_float_slider()?.value(),
        image_proxy_scale: params.get(Params::ImageProxyScale)?.as_popup()?.value(),
        path_sample_density: params.get(Params::PathSampleDensity)?.as_float_slider()?.value(),
        emitter_size_linked: params.get(Params::EmitterSizeLinked)?.as_checkbox()?.value(),
        emitter_size_x: params.get(Params::EmitterSizeX)?.as_float_slider()?.value(),
        emitter_size_y: params.get(Params::EmitterSizeY)?.as_float_slider()?.value(),
        emitter_size_z: params.get(Params::EmitterSizeZ)?.as_float_slider()?.value(),
        birth_rate: params.get(Params::BirthRate)?.as_float_slider()?.value(),
        lifespan: params.get(Params::Lifespan)?.as_float_slider()?.value(),
        lifespan_var: params.get(Params::LifespanVar)?.as_float_slider()?.value(),
        speed: params.get(Params::Speed)?.as_float_slider()?.value(),
        speed_var: params.get(Params::SpeedVar)?.as_float_slider()?.value(),
        direction_x: params.get(Params::DirectionX)?.as_float_slider()?.value(),
        direction_y: params.get(Params::DirectionY)?.as_float_slider()?.value(),
        direction_z: params.get(Params::DirectionZ)?.as_float_slider()?.value(),
        spread: params.get(Params::Spread)?.as_float_slider()?.value(),
        initial_size: params.get(Params::InitialSize)?.as_float_slider()?.value(),
        size_var: params.get(Params::SizeVar)?.as_float_slider()?.value(),
        rotation: params.get(Params::Rotation)?.as_float_slider()?.value(),
        rotation_speed: params.get(Params::RotationSpeed)?.as_float_slider()?.value(),
        gravity_strength: params.get(Params::GravityStrength)?.as_float_slider()?.value(),
        wind_x: params.get(Params::WindX)?.as_float_slider()?.value(),
        wind_y: params.get(Params::WindY)?.as_float_slider()?.value(),
        turb_strength: params.get(Params::TurbStrength)?.as_float_slider()?.value(),
        turb_scale: params.get(Params::TurbScale)?.as_float_slider()?.value(),
        turb_speed: params.get(Params::TurbSpeed)?.as_float_slider()?.value(),
        air_resistance: params.get(Params::AirResistance)?.as_float_slider()?.value(),
        bounce_enabled: params.get(Params::BounceEnabled)?.as_checkbox()?.value(),
        bounce_damping: params.get(Params::BounceDamping)?.as_float_slider()?.value(),
        color_mode: params.get(Params::ColorMode)?.as_popup()?.value(),
        color_start: params.get(Params::ColorStart)?.as_color()?.value().into(),
        color_end: params.get(Params::ColorEnd)?.as_color()?.value().into(),
        opacity_curve_preset: params.get(Params::OpacityCurvePreset)?.as_popup()?.value(),
        opacity_start: params.get(Params::OpacityStart)?.as_float_slider()?.value(),
        opacity_mid_a: params.get(Params::OpacityMidA)?.as_float_slider()?.value(),
        opacity_mid_b: params.get(Params::OpacityMidB)?.as_float_slider()?.value(),
        opacity_end: params.get(Params::OpacityEnd)?.as_float_slider()?.value(),
        size_curve_preset: params.get(Params::SizeCurvePreset)?.as_popup()?.value(),
        size_life_start: params.get(Params::SizeLifeStart)?.as_float_slider()?.value(),
        size_life_mid_a: params.get(Params::SizeLifeMidA)?.as_float_slider()?.value(),
        size_life_mid_b: params.get(Params::SizeLifeMidB)?.as_float_slider()?.value(),
        size_life_end: params.get(Params::SizeLifeEnd)?.as_float_slider()?.value(),
        shape: params.get(Params::Shape)?.as_popup()?.value(),
        image_color_mode: params.get(Params::ImageColorMode)?.as_popup()?.value(),
        image_fit_mode: params.get(Params::ImageFitMode)?.as_popup()?.value(),
        use_source_alpha: params.get(Params::UseSourceAlpha)?.as_checkbox()?.value(),
        source_premultiplied: params.get(Params::SourcePremultiplied)?.as_checkbox()?.value(),
        image_alpha_clip: params.get(Params::ImageAlphaClip)?.as_float_slider()?.value(),
        blend_mode: params.get(Params::BlendModeParam)?.as_popup()?.value(),
        motion_blur: params.get(Params::MotionBlur)?.as_float_slider()?.value(),
        edge_softness: params.get(Params::EdgeSoftness)?.as_float_slider()?.value(),
        dof_enabled: params.get(Params::DOFEnabled)?.as_checkbox()?.value(),
        dof_focal_dist: params.get(Params::DOFFocalDist)?.as_float_slider()?.value(),
        dof_aperture: params.get(Params::DOFAperture)?.as_float_slider()?.value(),
        size_multiplier: params.get(Params::SizeMultiplier)?.as_float_slider()?.value(),
        composite_on_orig: params.get(Params::CompositeOnOrig)?.as_checkbox()?.value(),
        child_enabled: params.get(Params::ChildEnabled)?.as_checkbox()?.value(),
        child_count: params.get(Params::ChildCount)?.as_slider()?.value(),
        child_inherit_vel: params.get(Params::ChildInheritVel)?.as_float_slider()?.value(),
        child_lifespan: params.get(Params::ChildLifespan)?.as_float_slider()?.value(),
        child_speed: params.get(Params::ChildSpeed)?.as_float_slider()?.value(),
        child_spread: params.get(Params::ChildSpread)?.as_float_slider()?.value(),
        child_size_scale: params.get(Params::ChildSizeScale)?.as_float_slider()?.value(),
        seed: params.get(Params::Seed)?.as_slider()?.value(),
        // ---- Plexus ----
        plugin_mode: params.get(Params::PluginMode)?.as_popup()?.value(),
        point_a_enabled: params.get(Params::PointAEnabled)?.as_checkbox()?.value(),
        point_a_source_type: params.get(Params::PointASourceType)?.as_popup()?.value(),
        point_a_grid_res_x: params.get(Params::PointAGridResX)?.as_slider()?.value(),
        point_a_grid_res_y: params.get(Params::PointAGridResY)?.as_slider()?.value(),
        point_a_grid_res_z: params.get(Params::PointAGridResZ)?.as_slider()?.value(),
        point_a_grid_spacing: params.get(Params::PointAGridSpacing)?.as_float_slider()?.value(),
        point_a_max_points: params.get(Params::PointAMaxPoints)?.as_slider()?.value(),
        point_b_enabled: params.get(Params::PointBEnabled)?.as_checkbox()?.value(),
        point_b_source_type: params.get(Params::PointBSourceType)?.as_popup()?.value(),
        point_b_grid_res_x: params.get(Params::PointBGridResX)?.as_slider()?.value(),
        point_b_grid_res_y: params.get(Params::PointBGridResY)?.as_slider()?.value(),
        point_b_grid_spacing: params.get(Params::PointBGridSpacing)?.as_float_slider()?.value(),
        noise_enabled: params.get(Params::NoiseEnabled)?.as_checkbox()?.value(),
        noise_amplitude: params.get(Params::NoiseAmplitude)?.as_float_slider()?.value(),
        noise_frequency: params.get(Params::NoiseFrequency)?.as_float_slider()?.value(),
        noise_speed: params.get(Params::NoiseSpeed)?.as_float_slider()?.value(),
        noise_octaves: params.get(Params::NoiseOctaves)?.as_slider()?.value(),
        noise_axis_scale: params.get(Params::NoiseAxisScale)?.as_popup()?.value(),
        lines_enabled: params.get(Params::LinesEnabled)?.as_checkbox()?.value(),
        lines_max_distance: params.get(Params::LinesMaxDistance)?.as_float_slider()?.value(),
        lines_width: params.get(Params::LinesWidth)?.as_float_slider()?.value(),
        lines_opacity_falloff: params.get(Params::LinesOpacityFalloff)?.as_float_slider()?.value(),
        lines_color: params.get(Params::LinesColor)?.as_color()?.value().into(),
        mesh_enabled: params.get(Params::MeshEnabled)?.as_checkbox()?.value(),
        mesh_max_edge: params.get(Params::MeshMaxEdge)?.as_float_slider()?.value(),
        mesh_opacity: params.get(Params::MeshOpacity)?.as_float_slider()?.value(),
        mesh_color: params.get(Params::MeshColor)?.as_color()?.value().into(),
        beams_enabled: params.get(Params::BeamsEnabled)?.as_checkbox()?.value(),
        beams_source_group: params.get(Params::BeamsSourceGroup)?.as_popup()?.value(),
        beams_max_distance: params.get(Params::BeamsMaxDistance)?.as_float_slider()?.value(),
        beams_width: params.get(Params::BeamsWidth)?.as_float_slider()?.value(),
        beams_color: params.get(Params::BeamsColor)?.as_color()?.value().into(),
        plexus_point_size: params.get(Params::PlexusPointSize)?.as_float_slider()?.value(),
        plexus_point_color: params.get(Params::PlexusPointColor)?.as_color()?.value().into(),
    })
}

fn set_popup_param(params: &mut ae::Parameters<Params>, id: Params, value: i32) -> Result<(), ae::Error> {
    let mut param = params.get_mut(id)?;
    param.as_popup_mut()?.set_value(value);
    param.set_change_flag(ae::ChangeFlag::CHANGED_VALUE, true);
    Ok(())
}

fn set_float_param(params: &mut ae::Parameters<Params>, id: Params, value: f64) -> Result<(), ae::Error> {
    let mut param = params.get_mut(id)?;
    param.as_float_slider_mut()?.set_value(value);
    param.set_change_flag(ae::ChangeFlag::CHANGED_VALUE, true);
    Ok(())
}

fn set_slider_param(params: &mut ae::Parameters<Params>, id: Params, value: i32) -> Result<(), ae::Error> {
    let mut param = params.get_mut(id)?;
    param.as_slider_mut()?.set_value(value);
    param.set_change_flag(ae::ChangeFlag::CHANGED_VALUE, true);
    Ok(())
}

fn set_checkbox_param(params: &mut ae::Parameters<Params>, id: Params, value: bool) -> Result<(), ae::Error> {
    let mut param = params.get_mut(id)?;
    param.as_checkbox_mut()?.set_value(value);
    param.set_change_flag(ae::ChangeFlag::CHANGED_VALUE, true);
    Ok(())
}

fn set_color_param(params: &mut ae::Parameters<Params>, id: Params, value: PresetColor) -> Result<(), ae::Error> {
    let mut param = params.get_mut(id)?;
    param.as_color_mut()?.set_value(value.into());
    param.set_change_flag(ae::ChangeFlag::CHANGED_VALUE, true);
    Ok(())
}

fn set_point_param(params: &mut ae::Parameters<Params>, id: Params, value: (f32, f32)) -> Result<(), ae::Error> {
    let mut param = params.get_mut(id)?;
    param.as_point_mut()?.set_value(value);
    param.set_change_flag(ae::ChangeFlag::CHANGED_VALUE, true);
    Ok(())
}

fn apply_preset(params: &ae::Parameters<Params>, preset: &PresetSnapshot) -> Result<(), ae::Error> {
    let mut params_copy = params.cloned();

    set_popup_param(&mut params_copy, Params::EmitterType, preset.emitter_type)?;
    set_point_param(&mut params_copy, Params::PositionPoint, preset.position_point)?;
    set_float_param(&mut params_copy, Params::PositionZ, preset.position_z)?;
    set_popup_param(&mut params_copy, Params::ImageProxyScale, preset.image_proxy_scale)?;
    set_checkbox_param(&mut params_copy, Params::EmitterSizeLinked, preset.emitter_size_linked)?;
    set_float_param(&mut params_copy, Params::EmitterSizeX, preset.emitter_size_x)?;
    set_float_param(&mut params_copy, Params::EmitterSizeY, preset.emitter_size_y)?;
    set_float_param(&mut params_copy, Params::EmitterSizeZ, preset.emitter_size_z)?;
    set_float_param(&mut params_copy, Params::BirthRate, preset.birth_rate)?;
    set_float_param(&mut params_copy, Params::Lifespan, preset.lifespan)?;
    set_float_param(&mut params_copy, Params::LifespanVar, preset.lifespan_var)?;
    set_float_param(&mut params_copy, Params::Speed, preset.speed)?;
    set_float_param(&mut params_copy, Params::SpeedVar, preset.speed_var)?;
    set_float_param(&mut params_copy, Params::DirectionX, preset.direction_x)?;
    set_float_param(&mut params_copy, Params::DirectionY, preset.direction_y)?;
    set_float_param(&mut params_copy, Params::DirectionZ, preset.direction_z)?;
    set_float_param(&mut params_copy, Params::Spread, preset.spread)?;
    set_float_param(&mut params_copy, Params::InitialSize, preset.initial_size)?;
    set_float_param(&mut params_copy, Params::SizeVar, preset.size_var)?;
    set_float_param(&mut params_copy, Params::Rotation, preset.rotation)?;
    set_float_param(&mut params_copy, Params::RotationSpeed, preset.rotation_speed)?;
    set_float_param(&mut params_copy, Params::GravityStrength, preset.gravity_strength)?;
    set_float_param(&mut params_copy, Params::WindX, preset.wind_x)?;
    set_float_param(&mut params_copy, Params::WindY, preset.wind_y)?;
    set_float_param(&mut params_copy, Params::TurbStrength, preset.turb_strength)?;
    set_float_param(&mut params_copy, Params::TurbScale, preset.turb_scale)?;
    set_float_param(&mut params_copy, Params::TurbSpeed, preset.turb_speed)?;
    set_float_param(&mut params_copy, Params::AirResistance, preset.air_resistance)?;
    set_checkbox_param(&mut params_copy, Params::BounceEnabled, preset.bounce_enabled)?;
    set_float_param(&mut params_copy, Params::BounceDamping, preset.bounce_damping)?;
    set_popup_param(&mut params_copy, Params::ColorMode, preset.color_mode)?;
    set_color_param(&mut params_copy, Params::ColorStart, preset.color_start)?;
    set_color_param(&mut params_copy, Params::ColorEnd, preset.color_end)?;
    set_popup_param(&mut params_copy, Params::OpacityCurvePreset, preset.opacity_curve_preset)?;
    set_float_param(&mut params_copy, Params::OpacityStart, preset.opacity_start)?;
    set_float_param(&mut params_copy, Params::OpacityMidA, preset.opacity_mid_a)?;
    set_float_param(&mut params_copy, Params::OpacityMidB, preset.opacity_mid_b)?;
    set_float_param(&mut params_copy, Params::OpacityEnd, preset.opacity_end)?;
    set_popup_param(&mut params_copy, Params::SizeCurvePreset, preset.size_curve_preset)?;
    set_float_param(&mut params_copy, Params::SizeLifeStart, preset.size_life_start)?;
    set_float_param(&mut params_copy, Params::SizeLifeMidA, preset.size_life_mid_a)?;
    set_float_param(&mut params_copy, Params::SizeLifeMidB, preset.size_life_mid_b)?;
    set_float_param(&mut params_copy, Params::SizeLifeEnd, preset.size_life_end)?;
    set_popup_param(&mut params_copy, Params::Shape, preset.shape)?;
    set_popup_param(&mut params_copy, Params::ImageColorMode, preset.image_color_mode)?;
    set_popup_param(&mut params_copy, Params::ImageFitMode, preset.image_fit_mode)?;
    set_checkbox_param(&mut params_copy, Params::UseSourceAlpha, preset.use_source_alpha)?;
    set_checkbox_param(&mut params_copy, Params::SourcePremultiplied, preset.source_premultiplied)?;
    set_float_param(&mut params_copy, Params::ImageAlphaClip, preset.image_alpha_clip)?;
    set_popup_param(&mut params_copy, Params::BlendModeParam, preset.blend_mode)?;
    set_float_param(&mut params_copy, Params::MotionBlur, preset.motion_blur)?;
    set_float_param(&mut params_copy, Params::EdgeSoftness, preset.edge_softness)?;
    set_checkbox_param(&mut params_copy, Params::DOFEnabled, preset.dof_enabled)?;
    set_float_param(&mut params_copy, Params::DOFFocalDist, preset.dof_focal_dist)?;
    set_float_param(&mut params_copy, Params::DOFAperture, preset.dof_aperture)?;
    set_float_param(&mut params_copy, Params::SizeMultiplier, preset.size_multiplier)?;
    set_checkbox_param(&mut params_copy, Params::CompositeOnOrig, preset.composite_on_orig)?;
    set_checkbox_param(&mut params_copy, Params::ChildEnabled, preset.child_enabled)?;
    set_slider_param(&mut params_copy, Params::ChildCount, preset.child_count)?;
    set_float_param(&mut params_copy, Params::ChildInheritVel, preset.child_inherit_vel)?;
    set_float_param(&mut params_copy, Params::ChildLifespan, preset.child_lifespan)?;
    set_float_param(&mut params_copy, Params::ChildSpeed, preset.child_speed)?;
    set_float_param(&mut params_copy, Params::ChildSpread, preset.child_spread)?;
    set_float_param(&mut params_copy, Params::ChildSizeScale, preset.child_size_scale)?;
    set_slider_param(&mut params_copy, Params::Seed, preset.seed)?;
    set_float_param(&mut params_copy, Params::PathSampleDensity, preset.path_sample_density)?;

    // ---- Plexus ----
    set_popup_param(&mut params_copy, Params::PluginMode, preset.plugin_mode)?;
    set_checkbox_param(&mut params_copy, Params::PointAEnabled, preset.point_a_enabled)?;
    set_popup_param(&mut params_copy, Params::PointASourceType, preset.point_a_source_type)?;
    set_slider_param(&mut params_copy, Params::PointAGridResX, preset.point_a_grid_res_x)?;
    set_slider_param(&mut params_copy, Params::PointAGridResY, preset.point_a_grid_res_y)?;
    set_slider_param(&mut params_copy, Params::PointAGridResZ, preset.point_a_grid_res_z)?;
    set_float_param(&mut params_copy, Params::PointAGridSpacing, preset.point_a_grid_spacing)?;
    set_slider_param(&mut params_copy, Params::PointAMaxPoints, preset.point_a_max_points)?;
    set_checkbox_param(&mut params_copy, Params::PointBEnabled, preset.point_b_enabled)?;
    set_popup_param(&mut params_copy, Params::PointBSourceType, preset.point_b_source_type)?;
    set_slider_param(&mut params_copy, Params::PointBGridResX, preset.point_b_grid_res_x)?;
    set_slider_param(&mut params_copy, Params::PointBGridResY, preset.point_b_grid_res_y)?;
    set_float_param(&mut params_copy, Params::PointBGridSpacing, preset.point_b_grid_spacing)?;
    set_checkbox_param(&mut params_copy, Params::NoiseEnabled, preset.noise_enabled)?;
    set_float_param(&mut params_copy, Params::NoiseAmplitude, preset.noise_amplitude)?;
    set_float_param(&mut params_copy, Params::NoiseFrequency, preset.noise_frequency)?;
    set_float_param(&mut params_copy, Params::NoiseSpeed, preset.noise_speed)?;
    set_slider_param(&mut params_copy, Params::NoiseOctaves, preset.noise_octaves)?;
    set_popup_param(&mut params_copy, Params::NoiseAxisScale, preset.noise_axis_scale)?;
    set_checkbox_param(&mut params_copy, Params::LinesEnabled, preset.lines_enabled)?;
    set_float_param(&mut params_copy, Params::LinesMaxDistance, preset.lines_max_distance)?;
    set_float_param(&mut params_copy, Params::LinesWidth, preset.lines_width)?;
    set_float_param(&mut params_copy, Params::LinesOpacityFalloff, preset.lines_opacity_falloff)?;
    set_color_param(&mut params_copy, Params::LinesColor, preset.lines_color)?;
    set_checkbox_param(&mut params_copy, Params::MeshEnabled, preset.mesh_enabled)?;
    set_float_param(&mut params_copy, Params::MeshMaxEdge, preset.mesh_max_edge)?;
    set_float_param(&mut params_copy, Params::MeshOpacity, preset.mesh_opacity)?;
    set_color_param(&mut params_copy, Params::MeshColor, preset.mesh_color)?;
    set_checkbox_param(&mut params_copy, Params::BeamsEnabled, preset.beams_enabled)?;
    set_popup_param(&mut params_copy, Params::BeamsSourceGroup, preset.beams_source_group)?;
    set_float_param(&mut params_copy, Params::BeamsMaxDistance, preset.beams_max_distance)?;
    set_float_param(&mut params_copy, Params::BeamsWidth, preset.beams_width)?;
    set_color_param(&mut params_copy, Params::BeamsColor, preset.beams_color)?;
    set_float_param(&mut params_copy, Params::PlexusPointSize, preset.plexus_point_size)?;
    set_color_param(&mut params_copy, Params::PlexusPointColor, preset.plexus_point_color)?;

    update_shape_dependent_ui(&params_copy)?;
    Ok(())
}

fn extract_configs(params: &ae::Parameters<Params>) -> Result<(EmitterConfig, PhysicsConfig, AppearanceConfig, ChildConfig, RenderConfig, u64), ae::Error> {
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
    let size_linked = params.get(Params::EmitterSizeLinked)?.as_checkbox()?.value();
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
        rotation_speed: params.get(Params::RotationSpeed)?.as_float_slider()?.value() as f32,
    };

    let physics = PhysicsConfig {
        gravity: glam::Vec3::new(0.0, params.get(Params::GravityStrength)?.as_float_slider()?.value() as f32, 0.0),
        wind: glam::Vec3::new(
            params.get(Params::WindX)?.as_float_slider()?.value() as f32,
            params.get(Params::WindY)?.as_float_slider()?.value() as f32,
            0.0,
        ),
        air_resistance: params.get(Params::AirResistance)?.as_float_slider()?.value() as f32,
        turbulence_strength: params.get(Params::TurbStrength)?.as_float_slider()?.value() as f32,
        turbulence_scale: params.get(Params::TurbScale)?.as_float_slider()?.value() as f32,
        turbulence_speed: params.get(Params::TurbSpeed)?.as_float_slider()?.value() as f32,
        bounce_floor_y: 10000.0, // effectively off unless positioned
        bounce_enabled: params.get(Params::BounceEnabled)?.as_checkbox()?.value(),
        bounce_damping: params.get(Params::BounceDamping)?.as_float_slider()?.value() as f32,
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
    let size_life_start = params.get(Params::SizeLifeStart)?.as_float_slider()?.value() as f32;
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
        size_over_life: [size_life_start, size_life_mid_a, size_life_mid_b, size_life_end],
        opacity_over_life: [opacity_start, opacity_mid_a, opacity_mid_b, opacity_end],
    };

    let child = ChildConfig {
        enabled: params.get(Params::ChildEnabled)?.as_checkbox()?.value(),
        count: params.get(Params::ChildCount)?.as_slider()?.value() as u32,
        inherit_velocity: params.get(Params::ChildInheritVel)?.as_float_slider()?.value() as f32,
        lifespan: params.get(Params::ChildLifespan)?.as_float_slider()?.value() as f32,
        initial_speed: params.get(Params::ChildSpeed)?.as_float_slider()?.value() as f32,
        spread: params.get(Params::ChildSpread)?.as_float_slider()?.value().to_radians() as f32,
        size_scale: params.get(Params::ChildSizeScale)?.as_float_slider()?.value() as f32,
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
        width: 0,  // filled at render time
        height: 0,
        row_stride: 0, // filled at render time
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
        size_multiplier: params.get(Params::SizeMultiplier)?.as_float_slider()?.value() as f32,
        sprite_image: None,
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
            source_premultiplied: params.get(Params::SourcePremultiplied)?.as_checkbox()?.value(),
            alpha_clip: params.get(Params::ImageAlphaClip)?.as_float_slider()?.value() as f32,
        },
        camera_projection: None,
    };

    let seed = params.get(Params::Seed)?.as_slider()?.value() as u64;

    Ok((emitter, physics, appearance, child, render, seed))
}

fn get_plugin_mode(params: &ae::Parameters<Params>) -> Result<i32, ae::Error> {
    Ok(params.get(Params::PluginMode)?.as_popup()?.value())
}

fn extract_plexus_configs(params: &ae::Parameters<Params>) -> Result<PlexusConfig, ae::Error> {
    let point_a_source_val = params.get(Params::PointASourceType)?.as_popup()?.value();
    let point_a = PointGroupConfig {
        enabled: params.get(Params::PointAEnabled)?.as_checkbox()?.value(),
        source_type: match point_a_source_val {
            1 => PointSourceType::Grid,
            2 => PointSourceType::Layer,
            3 => PointSourceType::ObjFile,
            4 => PointSourceType::AELights,
            5 => PointSourceType::Particles,
            _ => PointSourceType::Grid,
        },
        grid_res_x: params.get(Params::PointAGridResX)?.as_slider()?.value() as u32,
        grid_res_y: params.get(Params::PointAGridResY)?.as_slider()?.value() as u32,
        grid_res_z: params.get(Params::PointAGridResZ)?.as_slider()?.value() as u32,
        grid_spacing: params.get(Params::PointAGridSpacing)?.as_float_slider()?.value() as f32,
        max_points: params.get(Params::PointAMaxPoints)?.as_slider()?.value() as usize,
        source_points: None,
    };

    let point_b_source_val = params.get(Params::PointBSourceType)?.as_popup()?.value();
    let point_b = PointGroupConfig {
        enabled: params.get(Params::PointBEnabled)?.as_checkbox()?.value(),
        source_type: match point_b_source_val {
            1 => PointSourceType::Grid,
            2 => PointSourceType::Layer,
            3 => PointSourceType::ObjFile,
            4 => PointSourceType::AELights,
            _ => PointSourceType::Grid,
        },
        grid_res_x: params.get(Params::PointBGridResX)?.as_slider()?.value() as u32,
        grid_res_y: params.get(Params::PointBGridResY)?.as_slider()?.value() as u32,
        grid_res_z: 1,
        grid_spacing: params.get(Params::PointBGridSpacing)?.as_float_slider()?.value() as f32,
        max_points: 5000,
        source_points: None,
    };

    let point_color_pix = params.get(Params::PlexusPointColor)?.as_color()?.value();

    Ok(PlexusConfig {
        point_a,
        point_b,
        point_size: params.get(Params::PlexusPointSize)?.as_float_slider()?.value() as f32,
        point_color: [
            point_color_pix.red as f32 / 255.0,
            point_color_pix.green as f32 / 255.0,
            point_color_pix.blue as f32 / 255.0,
            1.0,
        ],
    })
}

// ---- Compute current time in seconds ----

fn current_time_sec(in_data: &ae::InData) -> f32 {
    let time = in_data.current_time() as f32;
    let scale = in_data.time_scale() as f32;
    if scale > 0.0 { time / scale } else { 0.0 }
}

fn time_step_sec(in_data: &ae::InData) -> f32 {
    let step = in_data.time_step() as f32;
    let scale = in_data.time_scale() as f32;
    if scale > 0.0 { step / scale } else { 1.0 / 30.0 }
}

fn checked_rgba_len(width: usize, height: usize) -> Result<usize, ae::Error> {
    let len = width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or(ae::Error::OutOfMemory)?;
    if len > MAX_RENDER_BYTES {
        debug_error(format!(
            "Refusing oversized render buffer: {}x{} ({} bytes)",
            width, height, len
        ));
        return Err(ae::Error::OutOfMemory);
    }
    Ok(len)
}


fn estimate_render_margin(params: &ae::Parameters<Params>) -> Result<i32, ae::Error> {
    let lifespan = params.get(Params::Lifespan)?.as_float_slider()?.value() as f32;
    let lifespan_var = params.get(Params::LifespanVar)?.as_float_slider()?.value() as f32;
    let speed = params.get(Params::Speed)?.as_float_slider()?.value() as f32;
    let speed_var = params.get(Params::SpeedVar)?.as_float_slider()?.value() as f32;
    let gravity = params.get(Params::GravityStrength)?.as_float_slider()?.value().abs() as f32;
    let wind_x = params.get(Params::WindX)?.as_float_slider()?.value().abs() as f32;
    let wind_y = params.get(Params::WindY)?.as_float_slider()?.value().abs() as f32;
    let turb = params.get(Params::TurbStrength)?.as_float_slider()?.value().abs() as f32;
    let size = params.get(Params::InitialSize)?.as_float_slider()?.value() as f32;
    let size_var = params.get(Params::SizeVar)?.as_float_slider()?.value() as f32;
    let size_multiplier = params.get(Params::SizeMultiplier)?.as_float_slider()?.value() as f32;
    let motion_blur = params.get(Params::MotionBlur)?.as_float_slider()?.value() as f32;
    let child_enabled = params.get(Params::ChildEnabled)?.as_checkbox()?.value();
    let child_lifespan = if child_enabled {
        params.get(Params::ChildLifespan)?.as_float_slider()?.value() as f32
    } else {
        0.0
    };

    let air_res = params.get(Params::AirResistance)?.as_float_slider()?.value().abs() as f32;

    let max_life = (lifespan * (1.0 + lifespan_var.clamp(0.0, 1.0)))
        .max(child_lifespan)
        .max(0.01);
    let peak_speed = speed.max(0.0) * (1.0 + speed_var.clamp(0.0, 1.0));
    let accel = gravity.max(wind_x.max(wind_y)) + turb;
    // Air resistance reduces effective travel distance
    let drag_factor = if air_res > 0.01 { (1.0 / air_res).min(max_life) } else { max_life };
    let travel = peak_speed * drag_factor + 0.5 * accel * max_life * max_life;
    let radius = size.max(0.0)
        * (1.0 + size_var.clamp(0.0, 1.0))
        * size_multiplier.max(0.01)
        * (1.0 + motion_blur.clamp(0.0, 1.0) * 0.5);

    Ok((travel + radius + 64.0)
        .ceil()
        .clamp(
            MIN_SMART_PRE_RENDER_MARGIN as f32,
            MAX_SMART_PRE_RENDER_MARGIN as f32,
        ) as i32)
}

fn estimated_emitter_bounds(params: &ae::Parameters<Params>, margin: i32) -> Result<ae::Rect, ae::Error> {
    let (pos_x, pos_y) = params.get(Params::PositionPoint)?.as_point()?.value();
    let emitter_type = params.get(Params::EmitterType)?.as_popup()?.value();
    let size_linked = params.get(Params::EmitterSizeLinked)?.as_checkbox()?.value();
    let size_x = params.get(Params::EmitterSizeX)?.as_float_slider()?.value() as f32;
    let size_y = if size_linked {
        size_x
    } else {
        params.get(Params::EmitterSizeY)?.as_float_slider()?.value() as f32
    };
    let max_emitter_half = MAX_SMART_PRE_RENDER_MARGIN as f32;
    let half_w = if matches!(emitter_type, 2 | 3 | 4) {
        (size_x * 0.5).min(max_emitter_half).ceil() as i32
    } else {
        0
    };
    let half_h = if matches!(emitter_type, 2 | 3 | 4) {
        (size_y * 0.5).min(max_emitter_half).ceil() as i32
    } else {
        0
    };
    let center_x = pos_x.round() as i32;
    let center_y = pos_y.round() as i32;

    Ok(ae::Rect {
        left: center_x.saturating_sub(half_w).saturating_sub(margin),
        top: center_y.saturating_sub(half_h).saturating_sub(margin),
        right: center_x.saturating_add(half_w).saturating_add(margin),
        bottom: center_y.saturating_add(half_h).saturating_add(margin),
    })
}

fn union_rect(a: ae::Rect, b: ae::Rect) -> ae::Rect {
    ae::Rect {
        left: a.left.min(b.left),
        top: a.top.min(b.top),
        right: a.right.max(b.right),
        bottom: a.bottom.max(b.bottom),
    }
}

fn clamp_rect_to_pixel_budget(rect: ae::Rect, budget: i64) -> ae::Rect {
    let w = (rect.right - rect.left) as i64;
    let h = (rect.bottom - rect.top) as i64;
    if w <= 0 || h <= 0 || w * h <= budget {
        return rect;
    }
    let scale = (budget as f64 / (w * h) as f64).sqrt();
    let cx = (rect.left + rect.right) / 2;
    let cy = (rect.top + rect.bottom) / 2;
    let hw = ((w as f64 * scale) * 0.5).ceil() as i32;
    let hh = ((h as f64 * scale) * 0.5).ceil() as i32;
    ae::Rect {
        left: cx - hw,
        top: cy - hh,
        right: cx + hw,
        bottom: cy + hh,
    }
}


// ---- Helpers: flat buffer I/O (same as MedianPro) ----

fn layer_to_flat(layer: &ae::Layer) -> (Vec<u8>, usize, usize) {
    let w = layer.width() as usize;
    let h = layer.height() as usize;
    let depth = layer.bit_depth();
    let stride = layer.buffer_stride();
    let buf = layer.buffer();
    let mut flat = vec![0u8; w * h * 4];
    match depth {
        16 => {
            for y in 0..h {
                let src_row = y * stride;
                let dst_row = y * w * 4;
                for x in 0..w {
                    let si = src_row + x * 8;
                    let di = dst_row + x * 4;
                    if si + 7 < buf.len() && di + 3 < flat.len() {
                        for ch in 0..4usize {
                            let v16 = u16::from_ne_bytes([buf[si + ch * 2], buf[si + ch * 2 + 1]]);
                            flat[di + ch] = ((v16 as u32 * 255 + 16384) / 32768).min(255) as u8;
                        }
                    }
                }
            }
        }
        32 => {
            for y in 0..h {
                let src_row = y * stride;
                let dst_row = y * w * 4;
                for x in 0..w {
                    let si = src_row + x * 16;
                    let di = dst_row + x * 4;
                    if si + 15 < buf.len() && di + 3 < flat.len() {
                        for ch in 0..4usize {
                            let v = f32::from_ne_bytes([
                                buf[si + ch * 4], buf[si + ch * 4 + 1],
                                buf[si + ch * 4 + 2], buf[si + ch * 4 + 3],
                            ]);
                            flat[di + ch] = (v * 255.0).clamp(0.0, 255.0) as u8;
                        }
                    }
                }
            }
        }
        _ => {
            for y in 0..h {
                let src_off = y * stride;
                let dst_off = y * w * 4;
                let row_len = w * 4;
                if src_off + row_len <= buf.len() && dst_off + row_len <= flat.len() {
                    flat[dst_off..dst_off + row_len].copy_from_slice(&buf[src_off..src_off + row_len]);
                }
            }
        }
    }
    (flat, w, h)
}

fn flat_to_layer(flat: &[u8], layer: &mut ae::Layer, w: usize, h: usize) {
    let depth = layer.bit_depth();
    let stride = layer.buffer_stride();
    let buf = layer.buffer_mut();
    match depth {
        16 => {
            // 8-bit ARGB → 16-bit ARGB (AE 16bpc: 0-32768 range, where 32768 = 1.0)
            for y in 0..h {
                let src_row = y * w * 4;
                let dst_row = y * stride;
                for x in 0..w {
                    let si = src_row + x * 4;
                    let di = dst_row + x * 8; // 4 channels × 2 bytes
                    if si + 3 < flat.len() && di + 7 < buf.len() {
                        for ch in 0..4usize {
                            let v8 = flat[si + ch] as u16;
                            // AE 16bpc: max value is 32768 (not 32767). 255 → 32768.
                            let v16 = ((v8 as u32 * 32768 + 127) / 255) as u16;
                            let bytes = v16.to_ne_bytes();
                            buf[di + ch * 2] = bytes[0];
                            buf[di + ch * 2 + 1] = bytes[1];
                        }
                    }
                }
            }
        }
        32 => {
            // 8-bit ARGB → 32-bit float ARGB (0.0 - 1.0)
            for y in 0..h {
                let src_row = y * w * 4;
                let dst_row = y * stride;
                for x in 0..w {
                    let si = src_row + x * 4;
                    let di = dst_row + x * 16; // 4 channels × 4 bytes
                    if si + 3 < flat.len() && di + 15 < buf.len() {
                        for ch in 0..4usize {
                            let v = flat[si + ch] as f32 / 255.0;
                            let bytes = v.to_ne_bytes();
                            buf[di + ch * 4] = bytes[0];
                            buf[di + ch * 4 + 1] = bytes[1];
                            buf[di + ch * 4 + 2] = bytes[2];
                            buf[di + ch * 4 + 3] = bytes[3];
                        }
                    }
                }
            }
        }
        _ => {
            // 8bpc: direct copy
            for y in 0..h {
                let src_off = y * w * 4;
                let dst_off = y * stride;
                let row_len = w * 4;
                if src_off + row_len <= flat.len() && dst_off + row_len <= buf.len() {
                    buf[dst_off..dst_off + row_len].copy_from_slice(&flat[src_off..src_off + row_len]);
                }
            }
        }
    }
}

fn blit_flat_into(
    src: &[u8],
    src_w: usize,
    src_h: usize,
    src_origin_x: i32,
    src_origin_y: i32,
    dst: &mut [u8],
    dst_w: usize,
    dst_h: usize,
    dst_origin_x: i32,
    dst_origin_y: i32,
) {
    // Compute overlap region in global coords, then copy row by row
    let g_left = src_origin_x.max(dst_origin_x);
    let g_top = src_origin_y.max(dst_origin_y);
    let g_right = (src_origin_x + src_w as i32).min(dst_origin_x + dst_w as i32);
    let g_bottom = (src_origin_y + src_h as i32).min(dst_origin_y + dst_h as i32);
    if g_left >= g_right || g_top >= g_bottom {
        return;
    }
    let copy_w = (g_right - g_left) as usize;
    for gy in g_top..g_bottom {
        let sy = (gy - src_origin_y) as usize;
        let dy = (gy - dst_origin_y) as usize;
        let sx = (g_left - src_origin_x) as usize;
        let dx = (g_left - dst_origin_x) as usize;
        let src_off = (sy * src_w + sx) * 4;
        let dst_off = (dy * dst_w + dx) * 4;
        let row_bytes = copy_w * 4;
        if src_off + row_bytes <= src.len() && dst_off + row_bytes <= dst.len() {
            dst[dst_off..dst_off + row_bytes].copy_from_slice(&src[src_off..src_off + row_bytes]);
        }
    }
}

fn image_cache() -> &'static RwLock<SpriteCacheMap> {
    IMAGE_CACHE.get_or_init(|| RwLock::new(SpriteCacheMap::new()))
}

fn emitter_cache() -> &'static RwLock<EmitterPointCacheMap> {
    EMITTER_CACHE.get_or_init(|| RwLock::new(EmitterPointCacheMap::new()))
}

fn invalidate_image_cache() {
    IMAGE_CACHE_GENERATION.fetch_add(1, Ordering::Relaxed);
    if let Ok(mut cache) = image_cache().write() {
        cache.clear();
    }
    if let Ok(mut cache) = emitter_cache().write() {
        cache.clear();
    }
}

fn proxy_divisor(params: &ae::Parameters<Params>) -> Result<u8, ae::Error> {
    let divisor = match params.get(Params::ImageProxyScale)?.as_popup()?.value() {
        1 => 1,
        2 => 2,
        3 => 4,
        4 => 8,
        _ => 4,
    };
    Ok(divisor)
}

fn layer_signature(layer: &ae::Layer) -> u64 {
    let width = layer.width() as usize;
    let height = layer.height() as usize;
    let stride = layer.buffer_stride();
    let buf = layer.buffer();
    if width == 0 || height == 0 || buf.is_empty() {
        return 0;
    }

    let sample_cols = width.min(8);
    let sample_rows = height.min(8);
    let mut hash = 0xcbf2_9ce4_8422_2325u64;

    for sy in 0..sample_rows {
        let y = sy * height / sample_rows;
        for sx in 0..sample_cols {
            let x = sx * width / sample_cols;
            let idx = y.saturating_mul(stride).saturating_add(x.saturating_mul(4));
            if idx + 3 >= buf.len() {
                continue;
            }

            let sample = [buf[idx], buf[idx + 1], buf[idx + 2], buf[idx + 3]];
            for byte in sample {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(0x100_0000_01b3);
            }
        }
    }

    hash
}

fn source_cache_key(
    in_data: &ae::InData,
    proxy_divisor: u8,
    source_layer: &ae::Layer,
) -> ImageCacheKey {
    let origin = source_layer.origin();
    ImageCacheKey {
        instance_id: in_data.effect().filter_instance_id().unwrap_or_default(),
        time: in_data.current_time(),
        time_step: in_data.time_step(),
        time_scale: in_data.time_scale(),
        proxy_divisor,
        source_width: source_layer.width() as u32,
        source_height: source_layer.height() as u32,
        source_origin_x: origin.h,
        source_origin_y: origin.v,
        source_signature: layer_signature(source_layer),
        generation: IMAGE_CACHE_GENERATION.load(Ordering::Relaxed),
    }
}

fn update_shape_dependent_ui(params: &ae::Parameters<Params>) -> Result<(), ae::Error> {
    let is_image_shape = params.get(Params::Shape)?.as_popup()?.value() == 6;
    let emitter_type_val = params.get(Params::EmitterType)?.as_popup()?.value();
    let is_layer_alpha_emitter = emitter_type_val == 5;
    let is_path_emitter = emitter_type_val == 6;
    let uses_emitter_volume = matches!(emitter_type_val, 2 | 3 | 4);
    let emitter_source_enabled = is_layer_alpha_emitter || is_path_emitter;
    let any_image_in_use = emitter_source_enabled || is_image_shape;
    let use_source_alpha = params.get(Params::UseSourceAlpha)?.as_checkbox()?.value();
    let size_linked = params.get(Params::EmitterSizeLinked)?.as_checkbox()?.value();
    let mut params_copy = params.cloned();

    // Emitter source layer control (Layer Alpha or Path emitter)
    {
        let mut param = params_copy.get_mut(Params::ImageSourceLayer)?;
        param.set_ui_flag(ae::ParamUIFlags::DISABLED, !emitter_source_enabled);
        param.update_param_ui()?;
    }

    // Refresh Image Cache + Image Proxy — active if either emitter source OR
    // sprite image is in use (the proxy divisor applies to both).
    for param_id in [Params::ImageProxyScale, Params::RefreshImageCache] {
        let mut param = params_copy.get_mut(param_id)?;
        param.set_ui_flag(ae::ParamUIFlags::DISABLED, !any_image_in_use);
        param.update_param_ui()?;
    }

    // Path density (Path emitter only)
    {
        let mut param = params_copy.get_mut(Params::PathSampleDensity)?;
        param.set_ui_flag(ae::ParamUIFlags::DISABLED, !is_path_emitter);
        param.update_param_ui()?;
    }

    // Sprite source layer (Image shape only)
    {
        let mut param = params_copy.get_mut(Params::SpriteSourceLayer)?;
        param.set_ui_flag(ae::ParamUIFlags::DISABLED, !is_image_shape);
        param.update_param_ui()?;
    }

    for param_id in [
        Params::ImageColorMode,
        Params::ImageFitMode,
        Params::UseSourceAlpha,
    ] {
        let mut param = params_copy.get_mut(param_id)?;
        param.set_ui_flag(ae::ParamUIFlags::DISABLED, !is_image_shape);
        param.update_param_ui()?;
    }

    for param_id in [
        Params::SourcePremultiplied,
        Params::ImageAlphaClip,
    ] {
        let mut param = params_copy.get_mut(param_id)?;
        param.set_ui_flag(ae::ParamUIFlags::DISABLED, !is_image_shape || !use_source_alpha);
        param.update_param_ui()?;
    }

    for param_id in [Params::EmitterSizeX, Params::EmitterSizeLinked] {
        let mut param = params_copy.get_mut(param_id)?;
        param.set_ui_flag(ae::ParamUIFlags::DISABLED, !uses_emitter_volume);
        param.update_param_ui()?;
    }

    for param_id in [Params::EmitterSizeY, Params::EmitterSizeZ] {
        let mut param = params_copy.get_mut(param_id)?;
        param.set_ui_flag(
            ae::ParamUIFlags::DISABLED,
            !uses_emitter_volume || size_linked,
        );
        param.update_param_ui()?;
    }

    // Disable individual curve sliders when a non-Custom preset is selected
    let opacity_custom = params.get(Params::OpacityCurvePreset)?.as_popup()?.value() == 1;
    for param_id in [Params::OpacityStart, Params::OpacityMidA, Params::OpacityMidB, Params::OpacityEnd] {
        let mut param = params_copy.get_mut(param_id)?;
        param.set_ui_flag(ae::ParamUIFlags::DISABLED, !opacity_custom);
        param.update_param_ui()?;
    }

    let size_custom = params.get(Params::SizeCurvePreset)?.as_popup()?.value() == 1;
    for param_id in [Params::SizeLifeStart, Params::SizeLifeMidA, Params::SizeLifeMidB, Params::SizeLifeEnd] {
        let mut param = params_copy.get_mut(param_id)?;
        param.set_ui_flag(ae::ParamUIFlags::DISABLED, !size_custom);
        param.update_param_ui()?;
    }

    Ok(())
}

fn sync_box_size_axes(params: &ae::Parameters<Params>, changed: Params) -> Result<(), ae::Error> {
    let emitter_type = params.get(Params::EmitterType)?.as_popup()?.value();
    let size_linked = params.get(Params::EmitterSizeLinked)?.as_checkbox()?.value();
    if emitter_type != 2 || !size_linked {
        return Ok(());
    }

    let source_value = match changed {
        Params::EmitterSizeX => params.get(Params::EmitterSizeX)?.as_float_slider()?.value(),
        Params::EmitterSizeY => params.get(Params::EmitterSizeY)?.as_float_slider()?.value(),
        Params::EmitterSizeZ => params.get(Params::EmitterSizeZ)?.as_float_slider()?.value(),
        Params::EmitterSizeLinked => params.get(Params::EmitterSizeX)?.as_float_slider()?.value(),
        _ => return Ok(()),
    };

    let mut params_copy = params.cloned();
    for param_id in [Params::EmitterSizeX, Params::EmitterSizeY, Params::EmitterSizeZ] {
        let mut param = params_copy.get_mut(param_id)?;
        param.as_float_slider_mut()?.set_value(source_value);
        param.set_change_flag(ae::ChangeFlag::CHANGED_VALUE, true);
        param.update_param_ui()?;
    }

    Ok(())
}

fn apply_opacity_preset(params: &ae::Parameters<Params>) -> Result<(), ae::Error> {
    let preset = params.get(Params::OpacityCurvePreset)?.as_popup()?.value();
    // 1=Custom (no change), 2=Constant, 3=Fade Out, 4=Fade In-Out, 5=Ease Out, 6=Quick Fade
    let values: [f64; 4] = match preset {
        2 => [100.0, 100.0, 100.0, 100.0],     // Constant
        3 => [100.0, 80.0, 40.0, 0.0],          // Fade Out (linear-ish)
        4 => [0.0, 100.0, 100.0, 0.0],          // Fade In-Out
        5 => [100.0, 95.0, 70.0, 0.0],          // Ease Out (slow start, fast end)
        6 => [100.0, 30.0, 5.0, 0.0],           // Quick Fade
        _ => return Ok(()),                       // Custom: don't change
    };
    let param_ids = [Params::OpacityStart, Params::OpacityMidA, Params::OpacityMidB, Params::OpacityEnd];
    let mut params_copy = params.cloned();
    for (i, &pid) in param_ids.iter().enumerate() {
        let mut param = params_copy.get_mut(pid)?;
        param.as_float_slider_mut()?.set_value(values[i]);
        param.set_change_flag(ae::ChangeFlag::CHANGED_VALUE, true);
        param.update_param_ui()?;
    }
    Ok(())
}

fn apply_size_preset(params: &ae::Parameters<Params>) -> Result<(), ae::Error> {
    let preset = params.get(Params::SizeCurvePreset)?.as_popup()?.value();
    // 1=Custom, 2=Constant, 3=Shrink, 4=Grow-Shrink, 5=Grow, 6=Pop-Shrink
    let values: [f64; 4] = match preset {
        2 => [1.0, 1.0, 1.0, 1.0],             // Constant
        3 => [1.0, 0.8, 0.4, 0.0],              // Shrink
        4 => [0.0, 1.0, 1.0, 0.0],              // Grow-Shrink
        5 => [0.0, 0.4, 0.8, 1.0],              // Grow
        6 => [1.2, 0.9, 0.4, 0.0],              // Pop-Shrink (starts big)
        _ => return Ok(()),                       // Custom: don't change
    };
    let param_ids = [Params::SizeLifeStart, Params::SizeLifeMidA, Params::SizeLifeMidB, Params::SizeLifeEnd];
    let mut params_copy = params.cloned();
    for (i, &pid) in param_ids.iter().enumerate() {
        let mut param = params_copy.get_mut(pid)?;
        param.as_float_slider_mut()?.set_value(values[i]);
        param.set_change_flag(ae::ChangeFlag::CHANGED_VALUE, true);
        param.update_param_ui()?;
    }
    Ok(())
}

fn populate_layer_alpha_emitter(
    params: &ae::Parameters<Params>,
    in_data: &ae::InData,
    emitter: &mut EmitterConfig,
) -> Result<(), ae::Error> {
    if !matches!(emitter.emitter_type, EmitterType::LayerAlpha) {
        return Ok(());
    }

    let checked_out = params.checkout(Params::ImageSourceLayer)?;
    let Some(source_layer) = checked_out.as_layer()?.value() else {
        return Ok(());
    };

    let key = source_cache_key(in_data, proxy_divisor(params)?, &source_layer);

    if let Ok(cache) = emitter_cache().read() {
        if let Some(points) = cache.get(&key) {
            emitter.source_points = Some(points.clone());
            return Ok(());
        }
    }

    let points = Arc::new(build_emitter_points(&source_layer, key.proxy_divisor));
    if let Ok(mut cache) = emitter_cache().write() {
        cache.insert(key, points.clone());
    }
    emitter.source_points = Some(points);
    Ok(())
}

fn build_emitter_points(layer: &ae::Layer, proxy_divisor: u8) -> Vec<glam::Vec3> {
    let (flat, width, height) = layer_to_flat(layer);
    if width == 0 || height == 0 || flat.is_empty() {
        return vec![glam::Vec3::ZERO];
    }

    let step = proxy_divisor.max(1) as usize;
    let half_w = width as f32 * 0.5;
    let half_h = height as f32 * 0.5;
    let mut points = Vec::new();

    for oy in (0..height).step_by(step) {
        for ox in (0..width).step_by(step) {
            let sample_x = (ox + step / 2).min(width - 1);
            let sample_y = (oy + step / 2).min(height - 1);
            let idx = (sample_y * width + sample_x) * 4;
            if idx + 3 >= flat.len() {
                continue;
            }

            let alpha = flat[idx] as f32 / 255.0;
            if alpha < 0.05 {
                continue;
            }

            let repeats = if alpha > 0.8 {
                3
            } else if alpha > 0.45 {
                2
            } else {
                1
            };
            let point = glam::Vec3::new(sample_x as f32 - half_w, sample_y as f32 - half_h, 0.0);
            for _ in 0..repeats {
                points.push(point);
            }
        }
    }

    if points.is_empty() {
        points.push(glam::Vec3::ZERO);
    } else if points.len() > 32_000 {
        let stride = (points.len() / 32_000).max(2);
        points = points.into_iter().step_by(stride).collect();
    }

    points
}

fn populate_path_emitter(
    params: &ae::Parameters<Params>,
    in_data: &ae::InData,
    emitter: &mut EmitterConfig,
) -> Result<(), ae::Error> {
    if !matches!(emitter.emitter_type, EmitterType::Path) {
        return Ok(());
    }

    let density = params.get(Params::PathSampleDensity)?.as_float_slider()?.value() as f32;
    let samples_per_seg = (density as usize).max(1);

    let path_query = match ae::pf::suites::PathQuery::new() {
        Ok(s) => s,
        Err(_) => return Ok(()),
    };

    let effect_ref = in_data.effect_ref();
    let num_paths = path_query.num_paths(&effect_ref)?;
    if num_paths <= 0 {
        return Ok(());
    }

    let mut points = Vec::new();
    let time = in_data.current_time();
    let step = in_data.time_step();
    let scale = in_data.time_scale();

    for path_idx in 0..num_paths {
        let path_id = match path_query.path_info(&effect_ref, path_idx) {
            Ok(id) => id,
            Err(_) => continue,
        };
        let path_outline = match path_query.checkout_path(&effect_ref, path_id, time, step, scale) {
            Ok(Some(p)) => p,
            _ => continue,
        };

        let num_segs = match path_outline.num_segments() {
            Ok(n) => n,
            Err(_) => continue,
        };
        if num_segs <= 0 {
            continue;
        }

        // Collect vertices for cubic bezier sampling
        let mut vertices = Vec::with_capacity((num_segs + 1) as usize);
        for vi in 0..=num_segs {
            if let Ok(v) = path_outline.vertex(vi) {
                vertices.push(v);
            }
        }

        // Sample each segment
        for seg in 0..(vertices.len().saturating_sub(1)) {
            let v0 = &vertices[seg];
            let v1 = &vertices[seg + 1];
            // Cubic bezier: P0, P0+tangent_out, P1+tangent_in, P1
            let p0 = glam::Vec2::new(v0.x as f32, v0.y as f32);
            let p1 = glam::Vec2::new((v0.x + v0.tan_out_x) as f32, (v0.y + v0.tan_out_y) as f32);
            let p2 = glam::Vec2::new((v1.x + v1.tan_in_x) as f32, (v1.y + v1.tan_in_y) as f32);
            let p3 = glam::Vec2::new(v1.x as f32, v1.y as f32);

            for si in 0..samples_per_seg {
                let t = si as f32 / samples_per_seg as f32;
                let pt = cubic_bezier(p0, p1, p2, p3, t);
                points.push(glam::Vec3::new(pt.x, pt.y, 0.0));
            }
        }
        // Add last point
        if let Some(last) = vertices.last() {
            points.push(glam::Vec3::new(last.x as f32, last.y as f32, 0.0));
        }
    }

    // Center around origin (like Layer Alpha does)
    if !points.is_empty() {
        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut max_x = f32::MIN;
        let mut max_y = f32::MIN;
        for p in &points {
            min_x = min_x.min(p.x);
            min_y = min_y.min(p.y);
            max_x = max_x.max(p.x);
            max_y = max_y.max(p.y);
        }
        let cx = (min_x + max_x) * 0.5;
        let cy = (min_y + max_y) * 0.5;
        for p in &mut points {
            p.x -= cx;
            p.y -= cy;
        }
    }

    if points.is_empty() {
        points.push(glam::Vec3::ZERO);
    } else if points.len() > 32_000 {
        let stride = (points.len() / 32_000).max(2);
        points = points.into_iter().step_by(stride).collect();
    }

    emitter.source_points = Some(Arc::new(points));
    Ok(())
}

fn cubic_bezier(p0: glam::Vec2, p1: glam::Vec2, p2: glam::Vec2, p3: glam::Vec2, t: f32) -> glam::Vec2 {
    let u = 1.0 - t;
    let uu = u * u;
    let tt = t * t;
    p0 * (uu * u) + p1 * (3.0 * uu * t) + p2 * (3.0 * u * tt) + p3 * (tt * t)
}

fn populate_image_sprite(
    params: &ae::Parameters<Params>,
    in_data: &ae::InData,
    render_cfg: &mut RenderConfig,
) -> Result<(), ae::Error> {
    if !matches!(render_cfg.shape, ParticleShape::Image) {
        return Ok(());
    }

    let checked_out = params.checkout(Params::SpriteSourceLayer)?;
    let Some(source_layer) = checked_out.as_layer()?.value() else {
        return Ok(());
    };

    let key = source_cache_key(in_data, proxy_divisor(params)?, &source_layer);

    if let Ok(cache) = image_cache().read() {
        if let Some(sprite) = cache.get(&key) {
            render_cfg.sprite_image = Some((**sprite).clone());
            return Ok(());
        }
    }

    let sprite = Arc::new(build_sprite_image(&source_layer, key.proxy_divisor));
    if let Ok(mut cache) = image_cache().write() {
        cache.insert(key, sprite.clone());
    }
    render_cfg.sprite_image = Some((*sprite).clone());
    Ok(())
}

fn build_sprite_image(layer: &ae::Layer, proxy_divisor: u8) -> SpriteImage {
    let (flat, width, height) = layer_to_flat(layer);
    if width == 0 || height == 0 || flat.is_empty() {
        return SpriteImage {
            width: 1,
            height: 1,
            pixels: Arc::new(vec![255, 255, 255, 255]),
        };
    }

    let step = proxy_divisor.max(1) as usize;
    let out_w = width.div_ceil(step).max(1);
    let out_h = height.div_ceil(step).max(1);
    let mut pixels = vec![0u8; out_w * out_h * 4];

    for oy in 0..out_h {
        for ox in 0..out_w {
            let sample_x = (ox * step + step / 2).min(width - 1);
            let sample_y = (oy * step + step / 2).min(height - 1);
            let src_idx = (sample_y * width + sample_x) * 4;
            let dst_idx = (oy * out_w + ox) * 4;
            if src_idx + 3 < flat.len() && dst_idx + 3 < pixels.len() {
                pixels[dst_idx..dst_idx + 4].copy_from_slice(&flat[src_idx..src_idx + 4]);
            }
        }
    }

    SpriteImage {
        width: out_w,
        height: out_h,
        pixels: Arc::new(pixels),
    }
}

// ---- Camera matrix helper ----

fn camera_diag_should_emit() -> bool {
    static LAST_LOG_MS: AtomicU64 = AtomicU64::new(0);
    static START: OnceLock<Instant> = OnceLock::new();
    let start = START.get_or_init(Instant::now);
    let now_ms = start.elapsed().as_millis() as u64;
    let last = LAST_LOG_MS.load(Ordering::Relaxed);
    if now_ms.saturating_sub(last) < 1000 {
        return false;
    }
    LAST_LOG_MS.store(now_ms, Ordering::Relaxed);
    true
}

fn fmt_mat16(flat: &[f64; 16]) -> String {
    flat.iter().map(|v| format!("{:.3}", v)).collect::<Vec<_>>().join(",")
}

fn fmt_mat44(m: &[[f64; 4]; 4]) -> String {
    let flat: [f64; 16] = [
        m[0][0], m[0][1], m[0][2], m[0][3],
        m[1][0], m[1][1], m[1][2], m[1][3],
        m[2][0], m[2][1], m[2][2], m[2][3],
        m[3][0], m[3][1], m[3][2], m[3][3],
    ];
    fmt_mat16(&flat)
}

fn log_camera_diag(raw_flat: &[f64; 16], projection: &CameraProjection) {
    if !camera_diag_should_emit() {
        return;
    }
    let inv_str = match invert_camera_matrix(projection.matrix) {
        Some(inv) => fmt_mat44(&inv),
        None => "invert_failed".to_string(),
    };
    let cw = projection.image_plane_width;
    let ch = projection.image_plane_height;
    let test_points: [(&str, glam::Vec3); 5] = [
        ("topleft_z0",   glam::Vec3::new(0.0,      0.0,      0.0)),
        ("center_z0",    glam::Vec3::new(cw * 0.5, ch * 0.5, 0.0)),
        ("center_zp500", glam::Vec3::new(cw * 0.5, ch * 0.5, 500.0)),
        ("center_zn500", glam::Vec3::new(cw * 0.5, ch * 0.5, -500.0)),
        ("right_z0",     glam::Vec3::new(cw,       ch * 0.5, 0.0)),
    ];
    let proj_str = test_points.iter().map(|(name, p)| {
        match project_point_3d(*p, projection) {
            Some((sx, sy, d)) => format!("{}=({:.1},{:.1},d{:.1})", name, sx, sy, d),
            None => format!("{}=cull", name),
        }
    }).collect::<Vec<_>>().join(" ");

    debug_info(format!(
        "[CAMERA-DIAG] dist={:.2} img={}x{} invert={} | raw=[{}] | trans=[{}] | inv=[{}] | proj: {}",
        projection.image_plane_dist,
        cw, ch,
        projection.invert_matrix,
        fmt_mat16(raw_flat),
        fmt_mat44(&projection.matrix),
        inv_str,
        proj_str,
    ));
}

fn try_get_camera_projection(in_data: &ae::InData) -> Option<CameraProjection> {
    let time = ae::Time { value: in_data.current_time(), scale: in_data.time_scale() };
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        in_data.effect().camera_matrix(time)
    }));
    let (matrix, image_plane_dist, image_w, image_h) = match result {
        Ok(Ok(v)) => v,
        Ok(Err(e)) => {
            if camera_diag_should_emit() {
                debug_info(format!("[CAMERA-DIAG] camera_matrix err: {:?}", e));
            }
            return None;
        }
        Err(_) => {
            if camera_diag_should_emit() {
                debug_info("[CAMERA-DIAG] camera_matrix panic");
            }
            return None;
        }
    };
    let flat: [f64; 16] = matrix.into();

    if image_plane_dist <= 0.0 || image_w <= 0 || image_h <= 0 {
        if camera_diag_should_emit() {
            debug_info(format!(
                "[CAMERA-DIAG] invalid metrics: dist={} w={} h={}",
                image_plane_dist, image_w, image_h
            ));
        }
        return None;
    }
    // Reject all-zero matrix
    if flat.iter().all(|v| v.abs() < 1e-10) {
        if camera_diag_should_emit() {
            debug_info("[CAMERA-DIAG] all-zero matrix");
        }
        return None;
    }

    // AE uses row-vector convention (v * M): translation in bottom row.
    // Transpose to column-vector convention (M * v): translation in rightmost column.
    let transposed = [
        [flat[0], flat[4], flat[8],  flat[12]],
        [flat[1], flat[5], flat[9],  flat[13]],
        [flat[2], flat[6], flat[10], flat[14]],
        [flat[3], flat[7], flat[11], flat[15]],
    ];

    let projection = CameraProjection {
        matrix: transposed,
        invert_matrix: true,
        image_plane_dist,
        image_plane_width: image_w.max(1) as f32,
        image_plane_height: image_h.max(1) as f32,
    };

    log_camera_diag(&flat, &projection);

    Some(projection)
}

// ---- Legacy render ----

fn render_particles(
    params: &ae::Parameters<Params>,
    in_data: &ae::InData,
    in_layer: &ae::Layer,
    out_layer: &mut ae::Layer,
) -> Result<(), ae::Error> {
    let mode = get_plugin_mode(params)?;
    let (mut emitter, physics, appearance, child, mut render_cfg, seed) = extract_configs(params)?;
    populate_layer_alpha_emitter(params, in_data, &mut emitter)?;
    populate_path_emitter(params, in_data, &mut emitter)?;
    populate_image_sprite(params, in_data, &mut render_cfg)?;

    let out_w = out_layer.width() as usize;
    let out_h = out_layer.height() as usize;
    let out_origin = out_layer.origin();
    render_cfg.width = out_w;
    render_cfg.height = out_h;
    render_cfg.row_stride = 0;
    render_cfg.origin_x = out_origin.h as f32;
    render_cfg.origin_y = out_origin.v as f32;

    let t = current_time_sec(in_data);
    let dt = time_step_sec(in_data);
    render_cfg.frame_dt = dt.max(1.0 / 240.0);
    render_cfg.camera_projection = try_get_camera_projection(in_data);

    let output_len = checked_rgba_len(out_w, out_h)?;
    let mut output = vec![0u8; output_len];
    if render_cfg.composite_on_original {
        let (original, in_w, in_h) = layer_to_flat(in_layer);
        let in_origin = in_layer.origin();
        blit_flat_into(
            &original, in_w, in_h, in_origin.h, in_origin.v,
            &mut output, out_w, out_h, out_origin.h, out_origin.v,
        );
    }

    let mut particle_count = 0;
    if mode == 1 || mode == 3 {
        let mut system = ParticleSystem::new(emitter, physics, appearance, child, seed);
        system.simulate_to_time(t, dt);
        particle_count = system.get_particles().len();
        renderer::render_particles_8bit(system.get_particles(), &render_cfg, &mut output, None);
    }

    if mode == 2 || mode == 3 {
        let plexus_cfg = extract_plexus_configs(params)?;
        let center_x = out_w as f32 * 0.5 + render_cfg.origin_x;
        let center_y = out_h as f32 * 0.5 + render_cfg.origin_y;
        let cloud = plexus::generate_points(&plexus_cfg, center_x, center_y);

        let plexus_render_cfg = plexus_render::PlexusRenderConfig {
            width: out_w,
            height: out_h,
            row_stride: 0,
            origin_x: render_cfg.origin_x,
            origin_y: render_cfg.origin_y,
            point_size: plexus_cfg.point_size,
            point_color: plexus_cfg.point_color,
            blend_mode: render_cfg.blend_mode,
            camera_projection: render_cfg.camera_projection,
        };

        for group in &cloud.groups {
            if !group.is_empty() {
                plexus_render::render_plexus_points_8bit(group, &plexus_render_cfg, &mut output);
            }
        }
    }

    flat_to_layer(&output, out_layer, out_w, out_h);

    debug_info(format!(
        "Render frame time={:.3}s dt={:.3}s size={}x{} particles={} mode={} composite={} origin=({}, {})",
        t, dt, out_w, out_h, particle_count, mode,
        render_cfg.composite_on_original, render_cfg.origin_x, render_cfg.origin_y
    ));

    Ok(())
}

// ---- SmartFX render ----

/// Check if AE has requested us to abort.
///
/// IMPORTANT: when this returns an error, propagate it unchanged (do NOT
/// convert to `Ok(())`). AE's SmartFX contract requires that a render which
/// does not write to the checked-out output buffer must return
/// `PF_Interrupt_CANCEL` (==`Error::InterruptCancel`). Returning `Ok(())`
/// without populating the output buffer causes AE to cache the (blank /
/// uninitialized) buffer as the rendered frame, which poisons the frame
/// cache until it is manually purged.
fn check_render_abort(in_data: &ae::InData) -> Result<(), ae::Error> {
    in_data.interact().abort()
}

fn smart_render_particles(
    render_start: Instant,
    in_data: &ae::InData,
    extra: &ae::pf::SmartRenderExtra,
) -> Result<(), ae::Error> {
    // Retrieve data collected in SmartPreRender — no AE API calls on render thread.
    let data = extra.pre_render_data::<SmartRenderData>()
        .ok_or(ae::Error::Generic)?;
    let mode = data.mode;
    let emitter = data.emitter.clone();
    let physics = data.physics;
    let appearance = data.appearance;
    let child = data.child;
    let mut render_cfg = data.render_cfg.clone();
    let seed = data.seed;
    let t = data.t;
    let dt = data.dt;
    let plexus_cfg = data.plexus_cfg.clone();
    render_cfg.camera_projection = data.camera_projection;

    let cb = extra.callbacks();

    // === Phase 1: Read input pixels into local memory ===
    // Keep input checked out so AE's "input before output" requirement is met for Phase 3.
    let input_world = cb.checkout_layer_pixels(0)?
        .ok_or(ae::Error::Generic)?;
    let input_origin = input_world.origin();
    let (original, input_w, input_h) = if render_cfg.composite_on_original {
        layer_to_flat(&input_world)
    } else {
        (Vec::new(), 0, 0)
    };
    // NOTE: input stays checked out — required for checkout_output in Phase 3.

    // Use expected dimensions from SmartPreRender (no checkout_output needed yet)
    let output_w = data.expected_output_w;
    let output_h = data.expected_output_h;

    debug_info(format!(
        "SmartRender V9 START output={}x{} elapsed={}ms",
        output_w, output_h, render_start.elapsed().as_millis()
    ));

    render_cfg.row_stride = 0; // flat: stride == width*4
    render_cfg.width = output_w;
    render_cfg.height = output_h;
    render_cfg.origin_x = data.expected_origin_x as f32;
    render_cfg.origin_y = data.expected_origin_y as f32;
    render_cfg.frame_dt = dt.max(1.0 / 240.0);

    let output_len = checked_rgba_len(output_w, output_h)?;
    let origin_x = data.expected_origin_x;
    let origin_y = data.expected_origin_y;
    debug_info(format!(
        "SmartRender V9 alloc {}MB ({}x{}) elapsed={}ms",
        output_len / (1024*1024), output_w, output_h,
        render_start.elapsed().as_millis()
    ));
    let mut output = vec![0u8; output_len];
    if !original.is_empty() {
        blit_flat_into(
            &original, input_w, input_h, input_origin.h, input_origin.v,
            &mut output, output_w, output_h, origin_x, origin_y,
        );
    }
    drop(original); // Free input copy immediately
    debug_info(format!("SmartRender V9 STEP:blit elapsed={}ms", render_start.elapsed().as_millis()));

    // Particles
    let mut particle_count = 0;
    if mode == 1 || mode == 3 {
        check_render_abort(in_data)?;
        let mut system = ParticleSystem::new(emitter, physics, appearance, child, seed);
        system.simulate_to_time(t, dt);
        let particles = system.get_particles();
        particle_count = particles.len();
        debug_info(format!("SmartRender V9 STEP:sim particles={} elapsed={}ms", particle_count, render_start.elapsed().as_millis()));
        check_render_abort(in_data)?;
        let deadline = Instant::checked_add(&render_start, std::time::Duration::from_millis(RENDER_TIME_BUDGET_MS as u64));
        renderer::render_particles_8bit(particles, &render_cfg, &mut output, deadline);
        debug_info(format!("SmartRender V9 STEP:draw elapsed={}ms", render_start.elapsed().as_millis()));
    }

    // Plexus
    if mode == 2 || mode == 3 {
        check_render_abort(in_data)?;
        let center_x = output_w as f32 * 0.5 + render_cfg.origin_x;
        let center_y = output_h as f32 * 0.5 + render_cfg.origin_y;
        let cloud = plexus::generate_points(&plexus_cfg, center_x, center_y);

        let plexus_render_cfg = plexus_render::PlexusRenderConfig {
            width: output_w,
            height: output_h,
            row_stride: 0,
            origin_x: render_cfg.origin_x,
            origin_y: render_cfg.origin_y,
            point_size: plexus_cfg.point_size,
            point_color: plexus_cfg.point_color,
            blend_mode: render_cfg.blend_mode,
            camera_projection: render_cfg.camera_projection,
        };

        for group in &cloud.groups {
            if !group.is_empty() {
                plexus_render::render_plexus_points_8bit(group, &plexus_render_cfg, &mut output);
            }
        }
        debug_info(format!("SmartRender V9 STEP:plexus elapsed={}ms", render_start.elapsed().as_millis()));
    }

    // Final abort check before writing. If AE has asked us to stop, propagate
    // `Error::InterruptCancel` — NEVER return `Ok(())` without writing the
    // output buffer, or AE will cache an uninitialized frame.
    check_render_abort(in_data)?;

    debug_info(format!("SmartRender V9 STEP:pre_write elapsed={}ms", render_start.elapsed().as_millis()));

    // === Phase 3: Checkout output and write result — hold AE buffer as briefly as possible ===
    let mut output_world = cb.checkout_output()?
        .ok_or(ae::Error::Generic)?;
    let bit_depth = output_world.bit_depth();
    flat_to_layer(&output, &mut output_world, output_w, output_h);
    // output_world and _input_world2 drop here

    debug_info(format!(
        "SmartRender V9 {}bpc time={:.3}s output={}x{} particles={} mode={} cam={}",
        bit_depth, t, output_w, output_h, particle_count, mode,
        render_cfg.camera_projection.is_some()
    ));
    Ok(())
}

