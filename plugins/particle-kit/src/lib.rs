use after_effects as ae;
#[cfg(windows)]
use std::fs;
#[cfg(windows)]
use std::fs::OpenOptions;
#[cfg(windows)]
use std::io::Write;
use std::panic::{self, AssertUnwindSafe};
#[cfg(windows)]
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock, RwLock};
use std::time::Instant;

// ---- File dialog via PowerShell (reliable from AE plugin context) ----
#[cfg(windows)]
mod file_dialog {
    use std::path::PathBuf;
    use std::process::Command;

    pub fn open_file_dialog(initial_dir: &str, title: &str) -> Option<PathBuf> {
        let script = format!(
            "Add-Type -AssemblyName System.Windows.Forms; \
             $d = New-Object System.Windows.Forms.OpenFileDialog; \
             $d.InitialDirectory = '{}'; \
             $d.Filter = 'JSON (*.json)|*.json|All Files|*.*'; \
             $d.Title = '{}'; \
             if ($d.ShowDialog() -eq 'OK') {{ Write-Output $d.FileName }}",
            initial_dir.replace('\'', "''"),
            title.replace('\'', "''")
        );
        let output = Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .output()
            .ok()?;
        let path_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if path_str.is_empty() {
            None
        } else {
            Some(PathBuf::from(path_str))
        }
    }

    pub fn save_file_dialog(initial_dir: &str, title: &str, default_name: &str) -> Option<PathBuf> {
        let script = format!(
            "Add-Type -AssemblyName System.Windows.Forms; \
             $d = New-Object System.Windows.Forms.SaveFileDialog; \
             $d.InitialDirectory = '{}'; \
             $d.Filter = 'JSON (*.json)|*.json'; \
             $d.Title = '{}'; \
             $d.FileName = '{}'; \
             $d.DefaultExt = 'json'; \
             if ($d.ShowDialog() -eq 'OK') {{ Write-Output $d.FileName }}",
            initial_dir.replace('\'', "''"),
            title.replace('\'', "''"),
            default_name.replace('\'', "''")
        );
        let output = Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .output()
            .ok()?;
        let path_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if path_str.is_empty() {
            None
        } else {
            Some(PathBuf::from(path_str))
        }
    }
}

mod classic_params;
mod engine;
mod graph;
mod node_graph_core;
mod particle;
mod preset;
mod project_state;
mod render_core;
mod renderer;

use classic_params::extract_engine_config;
use engine::{
    render_particle_engine_8bit, ParticleEngineConfig, ParticleRenderPlan, ParticleRuntimeInputs,
};
use graph::{GraphPublishedValue, GraphPublishedValueOverride};
use particle::{EmitterConfig, EmitterType};
use preset::{load_preset_snapshot, PresetColor, PresetSnapshot, PRESET_VERSION};
use project_state::{
    EngineConfigSource, NodeUiGraphStateSnapshot, ParticleLabProjectState, PROJECT_STATE_VERSION,
    PUBLISHED_HOST_FLOAT_SLOT_COUNT,
};
use render_core::RenderSurface;
use renderer::{
    invert_camera_matrix, project_point_3d, ApplyMode, CameraProjection, ParticleShape,
    RenderConfig, SpriteImage, TimeSamplingMode,
};

const NODE_UI_SHELL_INDEX_HTML: &str = include_str!("../tools/node-ui-shell/index.html");
const NODE_UI_SHELL_APP_JS: &str = include_str!("../tools/node-ui-shell/app.js");
const NODE_UI_SHELL_STYLES_CSS: &str = include_str!("../tools/node-ui-shell/styles.css");
const NODE_UI_SHELL_STARTUP_JS: &str = include_str!("../tools/node-ui-shell/startup-payload.js");

// ---- Parameter IDs ----

#[repr(u16)]
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
    // ---- Retired Plexus ABI slots ----
    //
    // These parameters are no longer used by the particle renderer, but they
    // must stay registered forever. The after-effects wrapper derives stable
    // host parameter IDs from the enum variant names, and AE projects also
    // rely on setup order. Removing or renaming these would shift saved project
    // streams when the effect is updated.
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
    SpriteTimeSampling,
    SpriteFrameCount,
    GridResX,
    GridResY,
    GridResZ,
    EmitMode,
    RotationVar,
    ApplyMode,
    OpacityVar,
    PublishedControlsGroupStart,
    PublishedFloat1,
    PublishedFloat2,
    PublishedFloat3,
    PublishedFloat4,
    PublishedControlsGroupEnd,
    NodeGraphGroupStart,
    ExportNodeGraphState,
    ImportNodeGraphState,
    SeedNodeGraphFromParams,
    DisableNodeGraph,
    NodeGraphGroupEnd,
    NodeUiSidecarGroupStart,
    OpenNodeUiShell,
    NodeUiSidecarGroupEnd,
}

// ---- Plugin ----

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

#[derive(Default)]
struct Plugin;

ae::define_effect!(Plugin, ParticleLabProjectState, Params);

const LEGACY_PLEXUS_PARAMS: &[Params] = &[
    Params::PluginMode,
    Params::PlexusGroupStart,
    Params::PointGroupAStart,
    Params::PointAEnabled,
    Params::PointASourceType,
    Params::PointASourceLayer,
    Params::PointAGridResX,
    Params::PointAGridResY,
    Params::PointAGridResZ,
    Params::PointAGridSpacing,
    Params::PointAMaxPoints,
    Params::PointGroupAEnd,
    Params::PointGroupBStart,
    Params::PointBEnabled,
    Params::PointBSourceType,
    Params::PointBSourceLayer,
    Params::PointBGridResX,
    Params::PointBGridResY,
    Params::PointBGridSpacing,
    Params::PointGroupBEnd,
    Params::NoiseGroupStart,
    Params::NoiseEnabled,
    Params::NoiseAmplitude,
    Params::NoiseFrequency,
    Params::NoiseSpeed,
    Params::NoiseOctaves,
    Params::NoiseAxisScale,
    Params::NoiseGroupEnd,
    Params::LinesGroupStart,
    Params::LinesEnabled,
    Params::LinesMaxDistance,
    Params::LinesWidth,
    Params::LinesOpacityFalloff,
    Params::LinesColor,
    Params::LinesGroupEnd,
    Params::MeshGroupStart,
    Params::MeshEnabled,
    Params::MeshMaxEdge,
    Params::MeshOpacity,
    Params::MeshColor,
    Params::MeshGroupEnd,
    Params::BeamsGroupStart,
    Params::BeamsEnabled,
    Params::BeamsSourceGroup,
    Params::BeamsMaxDistance,
    Params::BeamsWidth,
    Params::BeamsColor,
    Params::BeamsGroupEnd,
    Params::PlexusRenderGroupStart,
    Params::PlexusPointSize,
    Params::PlexusPointColor,
    Params::PlexusRenderGroupEnd,
    Params::PlexusGroupEnd,
];

fn legacy_param_ui_flags() -> ae::ParamUIFlags {
    ae::ParamUIFlags::NO_ECW_UI | ae::ParamUIFlags::INVISIBLE
}

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

type SpriteCacheMap = rustc_hash::FxHashMap<ImageCacheKey, Arc<Vec<SpriteImage>>>;
type EmitterPointCacheMap = rustc_hash::FxHashMap<ImageCacheKey, Arc<Vec<glam::Vec3>>>;

static IMAGE_CACHE: OnceLock<RwLock<SpriteCacheMap>> = OnceLock::new();
static EMITTER_CACHE: OnceLock<RwLock<EmitterPointCacheMap>> = OnceLock::new();
static IMAGE_CACHE_GENERATION: AtomicU64 = AtomicU64::new(1);
static IMAGE_CACHE_POPULATED: AtomicBool = AtomicBool::new(false);
static EMITTER_CACHE_POPULATED: AtomicBool = AtomicBool::new(false);
/// Data collected in SmartPreRender (main thread) and passed to SmartRender (render thread).
/// This avoids calling AE param/camera APIs from render threads.
struct SmartRenderData {
    engine: ParticleEngineConfig,
    plan: ParticleRenderPlan,
    source: EngineConfigSource,
}

const DEBUG_MODULE: &str = "ParticleKit";
const MAX_RENDER_BYTES: usize = 256 * 1024 * 1024;
const MAX_OUTPUT_PIXELS: i64 = 20_000_000; // ~4472x4472 max
const MIN_SMART_PRE_RENDER_MARGIN: i32 = 256;
const MAX_SMART_PRE_RENDER_MARGIN: i32 = 4096;
const RENDER_TIME_BUDGET_MS: u128 = 10_000;
const AE_PARAM_DYNAMIC_NAME_MAX_CHARS: usize = 31;

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

    if let Ok(mut pipe) = OpenOptions::new()
        .write(true)
        .open(r"\\.\pipe\AEExternalDebug")
    {
        let _ = pipe.write_all(line.as_bytes());
        let _ = pipe.flush();
    }

    // Also write to file for post-mortem debugging
    if let Ok(profile) = std::env::var("USERPROFILE") {
        let log_path = std::path::Path::new(&profile)
            .join("Documents")
            .join("Particle Kit")
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

fn command_is_sequence_owned(cmd: &ae::Command) -> bool {
    matches!(
        cmd,
        ae::Command::Render { .. }
            | ae::Command::SmartPreRender { .. }
            | ae::Command::SmartRender { .. }
            | ae::Command::SmartRenderGpu { .. }
            | ae::Command::UserChangedParam { .. }
            | ae::Command::UpdateParamsUi
    )
}

fn should_refresh_ui(param: Params) -> bool {
    matches!(
        param,
        Params::Shape
            | Params::EmitterType
            | Params::EmitMode
            | Params::UseSourceAlpha
            | Params::EmitterSizeLinked
            | Params::RefreshImageCache
            | Params::OpacityCurvePreset
            | Params::SizeCurvePreset
    )
}

fn should_invalidate_source_cache(param: Params) -> bool {
    matches!(
        param,
        Params::ImageSourceLayer | Params::ImageProxyScale | Params::RefreshImageCache
    )
}

fn add_legacy_checkbox(
    params: &mut ae::Parameters<Params>,
    id: Params,
    name: &str,
    default: bool,
) -> Result<(), ae::Error> {
    params.add_with_flags(
        id,
        name,
        ae::CheckBoxDef::setup(|f| {
            f.set_default(default);
            f.set_value(default);
            f.set_label("Enable");
        }),
        ae::ParamFlag::empty(),
        legacy_param_ui_flags(),
    )
}

fn add_legacy_popup(
    params: &mut ae::Parameters<Params>,
    id: Params,
    name: &str,
    options: &[&str],
    default: i32,
) -> Result<(), ae::Error> {
    params.add_with_flags(
        id,
        name,
        ae::PopupDef::setup(|f| {
            f.set_options(options);
            f.set_default(default);
            f.set_value(default);
        }),
        ae::ParamFlag::empty(),
        legacy_param_ui_flags(),
    )
}

fn add_legacy_slider(
    params: &mut ae::Parameters<Params>,
    id: Params,
    name: &str,
    min: i32,
    max: i32,
    slider_min: i32,
    slider_max: i32,
    default: i32,
) -> Result<(), ae::Error> {
    params.add_with_flags(
        id,
        name,
        ae::SliderDef::setup(|f| {
            f.set_valid_min(min);
            f.set_valid_max(max);
            f.set_slider_min(slider_min);
            f.set_slider_max(slider_max);
            f.set_default(default);
            f.set_value(default);
        }),
        ae::ParamFlag::empty(),
        legacy_param_ui_flags(),
    )
}

fn add_legacy_float(
    params: &mut ae::Parameters<Params>,
    id: Params,
    name: &str,
    min: f64,
    max: f64,
    slider_min: f64,
    slider_max: f64,
    default: f64,
    precision: i16,
) -> Result<(), ae::Error> {
    params.add_with_flags(
        id,
        name,
        ae::FloatSliderDef::setup(|f| {
            f.set_valid_min(min as f32);
            f.set_valid_max(max as f32);
            f.set_slider_min(slider_min as f32);
            f.set_slider_max(slider_max as f32);
            f.set_default(default);
            f.set_value(default);
            f.set_precision(precision);
        }),
        ae::ParamFlag::empty(),
        legacy_param_ui_flags(),
    )
}

fn add_legacy_color(
    params: &mut ae::Parameters<Params>,
    id: Params,
    name: &str,
    default: ae::Pixel8,
) -> Result<(), ae::Error> {
    params.add_with_flags(
        id,
        name,
        ae::ColorDef::setup(|f| {
            f.set_default(default);
            f.set_value(default);
        }),
        ae::ParamFlag::empty(),
        legacy_param_ui_flags(),
    )
}

fn add_legacy_layer(
    params: &mut ae::Parameters<Params>,
    id: Params,
    name: &str,
) -> Result<(), ae::Error> {
    params.add_with_flags(
        id,
        name,
        ae::LayerDef::new(),
        ae::ParamFlag::empty(),
        legacy_param_ui_flags(),
    )
}

fn add_legacy_plexus_params(params: &mut ae::Parameters<Params>) -> Result<(), ae::Error> {
    add_legacy_popup(
        params,
        Params::PluginMode,
        "Mode",
        &["Particles", "Plexus", "Combined"],
        1,
    )?;

    params.add_group(
        Params::PlexusGroupStart,
        Params::PlexusGroupEnd,
        "Plexus",
        true,
        |params| {
            params.add_group(
                Params::PointGroupAStart,
                Params::PointGroupAEnd,
                "Point Group A",
                false,
                |params| {
                    add_legacy_checkbox(params, Params::PointAEnabled, "Enable", true)?;
                    add_legacy_popup(
                        params,
                        Params::PointASourceType,
                        "Source Type",
                        &["Grid", "Layer", "OBJ File", "AE Lights", "Particles"],
                        1,
                    )?;
                    add_legacy_layer(params, Params::PointASourceLayer, "Source Layer")?;
                    add_legacy_slider(
                        params,
                        Params::PointAGridResX,
                        "Grid Res X",
                        2,
                        200,
                        2,
                        100,
                        10,
                    )?;
                    add_legacy_slider(
                        params,
                        Params::PointAGridResY,
                        "Grid Res Y",
                        2,
                        200,
                        2,
                        100,
                        10,
                    )?;
                    add_legacy_slider(
                        params,
                        Params::PointAGridResZ,
                        "Grid Res Z",
                        1,
                        100,
                        1,
                        50,
                        1,
                    )?;
                    add_legacy_float(
                        params,
                        Params::PointAGridSpacing,
                        "Grid Spacing",
                        1.0,
                        500.0,
                        5.0,
                        200.0,
                        50.0,
                        1,
                    )?;
                    add_legacy_slider(
                        params,
                        Params::PointAMaxPoints,
                        "Max Points",
                        10,
                        10000,
                        100,
                        10000,
                        5000,
                    )?;
                    Ok(())
                },
            )?;

            params.add_group(
                Params::PointGroupBStart,
                Params::PointGroupBEnd,
                "Point Group B",
                true,
                |params| {
                    add_legacy_checkbox(params, Params::PointBEnabled, "Enable", false)?;
                    add_legacy_popup(
                        params,
                        Params::PointBSourceType,
                        "Source Type",
                        &["Grid", "Layer", "OBJ File", "AE Lights"],
                        1,
                    )?;
                    add_legacy_layer(params, Params::PointBSourceLayer, "Source Layer")?;
                    add_legacy_slider(
                        params,
                        Params::PointBGridResX,
                        "Grid Res X",
                        2,
                        200,
                        2,
                        100,
                        10,
                    )?;
                    add_legacy_slider(
                        params,
                        Params::PointBGridResY,
                        "Grid Res Y",
                        2,
                        200,
                        2,
                        100,
                        10,
                    )?;
                    add_legacy_float(
                        params,
                        Params::PointBGridSpacing,
                        "Grid Spacing",
                        1.0,
                        500.0,
                        5.0,
                        200.0,
                        50.0,
                        1,
                    )?;
                    Ok(())
                },
            )?;

            params.add_group(
                Params::NoiseGroupStart,
                Params::NoiseGroupEnd,
                "Noise Displacement",
                true,
                |params| {
                    add_legacy_checkbox(params, Params::NoiseEnabled, "Enable Noise", false)?;
                    add_legacy_float(
                        params,
                        Params::NoiseAmplitude,
                        "Amplitude",
                        0.0,
                        1000.0,
                        0.0,
                        200.0,
                        50.0,
                        1,
                    )?;
                    add_legacy_float(
                        params,
                        Params::NoiseFrequency,
                        "Frequency",
                        0.001,
                        1.0,
                        0.001,
                        0.1,
                        0.01,
                        3,
                    )?;
                    add_legacy_float(
                        params,
                        Params::NoiseSpeed,
                        "Speed",
                        0.0,
                        10.0,
                        0.0,
                        5.0,
                        1.0,
                        2,
                    )?;
                    add_legacy_slider(params, Params::NoiseOctaves, "Octaves", 1, 8, 1, 5, 2)?;
                    add_legacy_popup(
                        params,
                        Params::NoiseAxisScale,
                        "Axis Scale",
                        &["Uniform", "XY Only", "Z Only"],
                        1,
                    )?;
                    Ok(())
                },
            )?;

            params.add_group(
                Params::LinesGroupStart,
                Params::LinesGroupEnd,
                "Lines",
                true,
                |params| {
                    add_legacy_checkbox(params, Params::LinesEnabled, "Enable Lines", true)?;
                    add_legacy_float(
                        params,
                        Params::LinesMaxDistance,
                        "Max Distance",
                        0.0,
                        1000.0,
                        0.0,
                        500.0,
                        120.0,
                        1,
                    )?;
                    add_legacy_float(
                        params,
                        Params::LinesWidth,
                        "Width",
                        0.1,
                        20.0,
                        0.1,
                        10.0,
                        1.0,
                        2,
                    )?;
                    add_legacy_float(
                        params,
                        Params::LinesOpacityFalloff,
                        "Opacity Falloff",
                        0.0,
                        2.0,
                        0.0,
                        1.0,
                        0.8,
                        2,
                    )?;
                    add_legacy_color(
                        params,
                        Params::LinesColor,
                        "Line Color",
                        ae::Pixel8 {
                            alpha: 255,
                            red: 255,
                            green: 255,
                            blue: 255,
                        },
                    )?;
                    Ok(())
                },
            )?;

            params.add_group(
                Params::MeshGroupStart,
                Params::MeshGroupEnd,
                "Mesh",
                true,
                |params| {
                    add_legacy_checkbox(params, Params::MeshEnabled, "Enable Mesh", false)?;
                    add_legacy_float(
                        params,
                        Params::MeshMaxEdge,
                        "Max Edge Length",
                        0.0,
                        1000.0,
                        0.0,
                        500.0,
                        150.0,
                        1,
                    )?;
                    add_legacy_float(
                        params,
                        Params::MeshOpacity,
                        "Mesh Opacity",
                        0.0,
                        100.0,
                        0.0,
                        100.0,
                        30.0,
                        1,
                    )?;
                    add_legacy_color(
                        params,
                        Params::MeshColor,
                        "Mesh Color",
                        ae::Pixel8 {
                            alpha: 255,
                            red: 100,
                            green: 150,
                            blue: 255,
                        },
                    )?;
                    Ok(())
                },
            )?;

            params.add_group(
                Params::BeamsGroupStart,
                Params::BeamsGroupEnd,
                "Beams",
                true,
                |params| {
                    add_legacy_checkbox(params, Params::BeamsEnabled, "Enable Beams", false)?;
                    add_legacy_popup(
                        params,
                        Params::BeamsSourceGroup,
                        "Source Group",
                        &["A", "B"],
                        1,
                    )?;
                    add_legacy_float(
                        params,
                        Params::BeamsMaxDistance,
                        "Max Distance",
                        0.0,
                        2000.0,
                        0.0,
                        1000.0,
                        300.0,
                        1,
                    )?;
                    add_legacy_float(
                        params,
                        Params::BeamsWidth,
                        "Width",
                        0.1,
                        50.0,
                        0.1,
                        20.0,
                        2.0,
                        2,
                    )?;
                    add_legacy_color(
                        params,
                        Params::BeamsColor,
                        "Beam Color",
                        ae::Pixel8 {
                            alpha: 255,
                            red: 100,
                            green: 200,
                            blue: 255,
                        },
                    )?;
                    Ok(())
                },
            )?;

            params.add_group(
                Params::PlexusRenderGroupStart,
                Params::PlexusRenderGroupEnd,
                "Render",
                false,
                |params| {
                    add_legacy_float(
                        params,
                        Params::PlexusPointSize,
                        "Point Size",
                        0.1,
                        50.0,
                        1.0,
                        20.0,
                        4.0,
                        1,
                    )?;
                    add_legacy_color(
                        params,
                        Params::PlexusPointColor,
                        "Point Color",
                        ae::Pixel8 {
                            alpha: 255,
                            red: 255,
                            green: 255,
                            blue: 255,
                        },
                    )?;
                    Ok(())
                },
            )?;

            Ok(())
        },
    )?;

    Ok(())
}

fn published_host_float_param(slot: u8) -> Option<Params> {
    if slot == 0 || slot > PUBLISHED_HOST_FLOAT_SLOT_COUNT {
        return None;
    }
    match slot {
        1 => Some(Params::PublishedFloat1),
        2 => Some(Params::PublishedFloat2),
        3 => Some(Params::PublishedFloat3),
        4 => Some(Params::PublishedFloat4),
        _ => None,
    }
}

fn add_published_host_float_param(
    params: &mut ae::Parameters<Params>,
    id: Params,
    name: &str,
) -> Result<(), ae::Error> {
    params.add(
        id,
        name,
        ae::FloatSliderDef::setup(|f| {
            f.set_valid_min(-1_000_000.0);
            f.set_valid_max(1_000_000.0);
            f.set_slider_min(-100.0);
            f.set_slider_max(100.0);
            f.set_default(0.0);
            f.set_precision(2);
        }),
    )
}

fn add_published_host_params(params: &mut ae::Parameters<Params>) -> Result<(), ae::Error> {
    params.add_group(
        Params::PublishedControlsGroupStart,
        Params::PublishedControlsGroupEnd,
        "Published Graph Controls",
        true,
        |params| {
            add_published_host_float_param(params, Params::PublishedFloat1, "Published Float 1")?;
            add_published_host_float_param(params, Params::PublishedFloat2, "Published Float 2")?;
            add_published_host_float_param(params, Params::PublishedFloat3, "Published Float 3")?;
            add_published_host_float_param(params, Params::PublishedFloat4, "Published Float 4")?;
            Ok(())
        },
    )
}

fn add_node_graph_tool_params(params: &mut ae::Parameters<Params>) -> Result<(), ae::Error> {
    params.add_group(
        Params::NodeGraphGroupStart,
        Params::NodeGraphGroupEnd,
        "Node Graph",
        true,
        |params| {
            params.add(
                Params::ExportNodeGraphState,
                "Export Node Graph",
                ae::ButtonDef::setup(|f| {
                    f.set_label("Export");
                }),
            )?;
            params.add(
                Params::ImportNodeGraphState,
                "Import Node Graph",
                ae::ButtonDef::setup(|f| {
                    f.set_label("Import");
                }),
            )?;
            params.add(
                Params::SeedNodeGraphFromParams,
                "Seed From Current Params",
                ae::ButtonDef::setup(|f| {
                    f.set_label("Seed");
                }),
            )?;
            params.add(
                Params::DisableNodeGraph,
                "Disable Node Graph",
                ae::ButtonDef::setup(|f| {
                    f.set_label("Disable");
                }),
            )?;
            Ok(())
        },
    )
}

fn add_node_ui_sidecar_params(params: &mut ae::Parameters<Params>) -> Result<(), ae::Error> {
    params.add_group(
        Params::NodeUiSidecarGroupStart,
        Params::NodeUiSidecarGroupEnd,
        "Node UI Sidecar",
        true,
        |params| {
            params.add(
                Params::OpenNodeUiShell,
                "Open Node UI Shell",
                ae::ButtonDef::setup(|f| {
                    f.set_label("Open Shell");
                }),
            )?;
            Ok(())
        },
    )
}

impl AdobePluginGlobal for Plugin {
    fn params_setup(
        &self,
        params: &mut ae::Parameters<Params>,
        _in_data: ae::InData,
        _out_data: ae::OutData,
    ) -> Result<(), ae::Error> {
        params.add_group(
            Params::PresetGroupStart,
            Params::PresetGroupEnd,
            "Presets",
            true,
            |params| {
                params.add(
                    Params::SavePreset,
                    "Save Preset",
                    ae::ButtonDef::setup(|f| {
                        f.set_label("Save");
                    }),
                )?;
                params.add(
                    Params::LoadPreset,
                    "Load Preset",
                    ae::ButtonDef::setup(|f| {
                        f.set_label("Load");
                    }),
                )?;
                params.add(
                    Params::DeletePreset,
                    "Delete Preset",
                    ae::ButtonDef::setup(|f| {
                        f.set_label("Delete");
                    }),
                )?;
                params.add(
                    Params::OpenPresetFolder,
                    "Open Folder",
                    ae::ButtonDef::setup(|f| {
                        f.set_label("Open Folder");
                    }),
                )?;
                Ok(())
            },
        )?;

        params.add_group(
            Params::EmitterGroupStart,
            Params::EmitterGroupEnd,
            "Emitter",
            false,
            |params| {
                params.add_with_flags(
                    Params::EmitterType,
                    "Emitter Type",
                    ae::PopupDef::setup(|f| {
                        f.set_options(&["Point", "Box", "Sphere", "Grid", "Layer Alpha", "Path"]);
                        f.set_default(1);
                    }),
                    ae::ParamFlag::SUPERVISE,
                    ae::ParamUIFlags::empty(),
                )?;
                params.add(
                    Params::EmitMode,
                    "Emit Mode",
                    ae::PopupDef::setup(|f| {
                        f.set_options(&["Continuous", "All at Start"]);
                        f.set_default(1);
                    }),
                )?;
                params.add(
                    Params::PositionPoint,
                    "Position",
                    ae::PointDef::setup(|f| {
                        f.set_default((50.0, 50.0));
                        f.set_restrict_bounds(false);
                    }),
                )?;
                params.add(
                    Params::PositionZ,
                    "Position Z",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(-10000.0);
                        f.set_valid_max(10000.0);
                        f.set_slider_min(-2000.0);
                        f.set_slider_max(2000.0);
                        f.set_default(0.0);
                        f.set_precision(1);
                    }),
                )?;
                params.add(
                    Params::ImageSourceLayer,
                    "Emitter Source Layer",
                    ae::LayerDef::new(),
                )?;
                params.add(
                    Params::RefreshImageCache,
                    "Refresh Image Cache",
                    ae::ButtonDef::setup(|f| {
                        f.set_label("Refresh");
                    }),
                )?;
                params.add(
                    Params::PathSampleDensity,
                    "Path Density",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(1.0);
                        f.set_valid_max(100.0);
                        f.set_slider_min(1.0);
                        f.set_slider_max(50.0);
                        f.set_default(10.0);
                        f.set_precision(0);
                    }),
                )?;
                params.add(
                    Params::EmitterSizeLinked,
                    "Uniform Box Size",
                    ae::CheckBoxDef::setup(|f| {
                        f.set_default(true);
                        f.set_label("Enable");
                    }),
                )?;
                params.add(
                    Params::EmitterSizeX,
                    "Emitter Size X",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.0);
                        f.set_valid_max(10000.0);
                        f.set_slider_min(0.0);
                        f.set_slider_max(2000.0);
                        f.set_default(0.0);
                        f.set_precision(1);
                    }),
                )?;
                params.add(
                    Params::EmitterSizeY,
                    "Emitter Size Y",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.0);
                        f.set_valid_max(10000.0);
                        f.set_slider_min(0.0);
                        f.set_slider_max(2000.0);
                        f.set_default(0.0);
                        f.set_precision(1);
                    }),
                )?;
                params.add(
                    Params::EmitterSizeZ,
                    "Emitter Size Z",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.0);
                        f.set_valid_max(10000.0);
                        f.set_slider_min(0.0);
                        f.set_slider_max(2000.0);
                        f.set_default(0.0);
                        f.set_precision(1);
                    }),
                )?;
                params.add(
                    Params::GridResX,
                    "Grid Res X",
                    ae::SliderDef::setup(|f| {
                        f.set_valid_min(1);
                        f.set_valid_max(50);
                        f.set_slider_min(1);
                        f.set_slider_max(50);
                        f.set_default(8);
                    }),
                )?;
                params.add(
                    Params::GridResY,
                    "Grid Res Y",
                    ae::SliderDef::setup(|f| {
                        f.set_valid_min(1);
                        f.set_valid_max(50);
                        f.set_slider_min(1);
                        f.set_slider_max(50);
                        f.set_default(8);
                    }),
                )?;
                params.add(
                    Params::GridResZ,
                    "Grid Res Z",
                    ae::SliderDef::setup(|f| {
                        f.set_valid_min(1);
                        f.set_valid_max(10);
                        f.set_slider_min(1);
                        f.set_slider_max(10);
                        f.set_default(1);
                    }),
                )?;
                params.add(
                    Params::BirthRate,
                    "Birth Rate",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.0);
                        f.set_valid_max(10000.0);
                        f.set_slider_min(0.0);
                        f.set_slider_max(1000.0);
                        f.set_default(180.0);
                        f.set_precision(1);
                    }),
                )?;
                params.add(
                    Params::Lifespan,
                    "Lifespan (sec)",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.01);
                        f.set_valid_max(120.0);
                        f.set_slider_min(0.1);
                        f.set_slider_max(30.0);
                        f.set_default(1.6);
                        f.set_precision(2);
                    }),
                )?;
                params.add(
                    Params::LifespanVar,
                    "Lifespan Variation",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.0);
                        f.set_valid_max(1.0);
                        f.set_slider_min(0.0);
                        f.set_slider_max(1.0);
                        f.set_default(0.15);
                        f.set_precision(2);
                    }),
                )?;
                Ok(())
            },
        )?;

        params.add_group(
            Params::MotionGroupStart,
            Params::MotionGroupEnd,
            "Motion",
            false,
            |params| {
                params.add(
                    Params::Speed,
                    "Speed",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.0);
                        f.set_valid_max(5000.0);
                        f.set_slider_min(0.0);
                        f.set_slider_max(500.0);
                        f.set_default(240.0);
                        f.set_precision(1);
                    }),
                )?;
                params.add(
                    Params::SpeedVar,
                    "Speed Variation",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.0);
                        f.set_valid_max(1.0);
                        f.set_slider_min(0.0);
                        f.set_slider_max(1.0);
                        f.set_default(0.0);
                        f.set_precision(2);
                    }),
                )?;
                params.add(
                    Params::DirectionX,
                    "Direction X",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(-1.0);
                        f.set_valid_max(1.0);
                        f.set_slider_min(-1.0);
                        f.set_slider_max(1.0);
                        f.set_default(0.0);
                        f.set_precision(2);
                    }),
                )?;
                params.add(
                    Params::DirectionY,
                    "Direction Y",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(-1.0);
                        f.set_valid_max(1.0);
                        f.set_slider_min(-1.0);
                        f.set_slider_max(1.0);
                        f.set_default(-1.0);
                        f.set_precision(2);
                    }),
                )?;
                params.add(
                    Params::DirectionZ,
                    "Direction Z",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(-1.0);
                        f.set_valid_max(1.0);
                        f.set_slider_min(-1.0);
                        f.set_slider_max(1.0);
                        f.set_default(0.0);
                        f.set_precision(2);
                    }),
                )?;
                params.add(
                    Params::Spread,
                    "Spread (deg)",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.0);
                        f.set_valid_max(180.0);
                        f.set_slider_min(0.0);
                        f.set_slider_max(180.0);
                        f.set_default(18.0);
                        f.set_precision(1);
                    }),
                )?;
                params.add(
                    Params::InitialSize,
                    "Particle Size",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.1);
                        f.set_valid_max(500.0);
                        f.set_slider_min(0.5);
                        f.set_slider_max(100.0);
                        f.set_default(9.0);
                        f.set_precision(1);
                    }),
                )?;
                params.add(
                    Params::SizeVar,
                    "Size Variation",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.0);
                        f.set_valid_max(1.0);
                        f.set_slider_min(0.0);
                        f.set_slider_max(1.0);
                        f.set_default(0.2);
                        f.set_precision(2);
                    }),
                )?;
                params.add(
                    Params::Rotation,
                    "Initial Rotation",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.0);
                        f.set_valid_max(360.0);
                        f.set_slider_min(0.0);
                        f.set_slider_max(360.0);
                        f.set_default(0.0);
                        f.set_precision(1);
                    }),
                )?;
                params.add(
                    Params::RotationSpeed,
                    "Rotation Speed",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(-1000.0);
                        f.set_valid_max(1000.0);
                        f.set_slider_min(-360.0);
                        f.set_slider_max(360.0);
                        f.set_default(0.0);
                        f.set_precision(1);
                    }),
                )?;
                params.add(
                    Params::RotationVar,
                    "Rotation Variation",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.0);
                        f.set_valid_max(360.0);
                        f.set_slider_min(0.0);
                        f.set_slider_max(180.0);
                        f.set_default(0.0);
                        f.set_precision(1);
                    }),
                )?;
                Ok(())
            },
        )?;

        params.add_group(
            Params::PhysicsGroupStart,
            Params::PhysicsGroupEnd,
            "Physics",
            false,
            |params| {
                params.add_with_flags(
                    Params::GravityStrength,
                    "Gravity",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(-2000.0);
                        f.set_valid_max(2000.0);
                        f.set_slider_min(-500.0);
                        f.set_slider_max(500.0);
                        f.set_default(160.0);
                        f.set_precision(1);
                    }),
                    ae::ParamFlag::CANNOT_TIME_VARY,
                    ae::ParamUIFlags::empty(),
                )?;
                params.add(
                    Params::WindX,
                    "Wind X",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(-2000.0);
                        f.set_valid_max(2000.0);
                        f.set_slider_min(-500.0);
                        f.set_slider_max(500.0);
                        f.set_default(0.0);
                        f.set_precision(1);
                    }),
                )?;
                params.add(
                    Params::WindY,
                    "Wind Y",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(-2000.0);
                        f.set_valid_max(2000.0);
                        f.set_slider_min(-500.0);
                        f.set_slider_max(500.0);
                        f.set_default(0.0);
                        f.set_precision(1);
                    }),
                )?;
                params.add(
                    Params::TurbStrength,
                    "Turbulence",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.0);
                        f.set_valid_max(1000.0);
                        f.set_slider_min(0.0);
                        f.set_slider_max(200.0);
                        f.set_default(12.0);
                        f.set_precision(1);
                    }),
                )?;
                params.add(
                    Params::TurbScale,
                    "Turbulence Scale",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.01);
                        f.set_valid_max(100.0);
                        f.set_slider_min(0.1);
                        f.set_slider_max(10.0);
                        f.set_default(0.75);
                        f.set_precision(2);
                    }),
                )?;
                params.add(
                    Params::TurbSpeed,
                    "Turbulence Speed",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.0);
                        f.set_valid_max(10.0);
                        f.set_slider_min(0.0);
                        f.set_slider_max(5.0);
                        f.set_default(1.0);
                        f.set_precision(2);
                    }),
                )?;
                params.add(
                    Params::AirResistance,
                    "Air Resistance",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.0);
                        f.set_valid_max(20.0);
                        f.set_slider_min(0.0);
                        f.set_slider_max(5.0);
                        f.set_default(0.3);
                        f.set_precision(2);
                    }),
                )?;
                params.add(
                    Params::BounceEnabled,
                    "Bounce",
                    ae::CheckBoxDef::setup(|f| {
                        f.set_default(false);
                        f.set_label("Enable");
                    }),
                )?;
                params.add(
                    Params::BounceDamping,
                    "Bounce Damping",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.0);
                        f.set_valid_max(1.0);
                        f.set_slider_min(0.0);
                        f.set_slider_max(1.0);
                        f.set_default(0.5);
                        f.set_precision(2);
                    }),
                )?;
                Ok(())
            },
        )?;

        params.add_group(
            Params::AppearanceGroupStart,
            Params::AppearanceGroupEnd,
            "Appearance",
            false,
            |params| {
                params.add(
                    Params::ColorMode,
                    "Color Mode",
                    ae::PopupDef::setup(|f| {
                        f.set_options(&["Single", "Gradient"]);
                        f.set_default(2);
                    }),
                )?;
                params.add(
                    Params::ColorStart,
                    "Color Start",
                    ae::ColorDef::setup(|f| {
                        f.set_default(ae::Pixel8 {
                            alpha: 255,
                            red: 255,
                            green: 255,
                            blue: 255,
                        });
                    }),
                )?;
                params.add(
                    Params::ColorEnd,
                    "Color End",
                    ae::ColorDef::setup(|f| {
                        f.set_default(ae::Pixel8 {
                            alpha: 255,
                            red: 255,
                            green: 255,
                            blue: 255,
                        });
                    }),
                )?;
                params.add(
                    Params::OpacityVar,
                    "Opacity Variation",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.0);
                        f.set_valid_max(1.0);
                        f.set_slider_min(0.0);
                        f.set_slider_max(1.0);
                        f.set_default(0.0);
                        f.set_precision(2);
                    }),
                )?;
                params.add_with_flags(
                    Params::OpacityCurvePreset,
                    "Opacity Curve",
                    ae::PopupDef::setup(|f| {
                        f.set_options(&[
                            "Custom",
                            "Constant",
                            "Fade Out",
                            "Fade In-Out",
                            "Ease Out",
                            "Quick Fade",
                        ]);
                        f.set_default(3); // Fade Out
                    }),
                    ae::ParamFlag::SUPERVISE,
                    ae::ParamUIFlags::empty(),
                )?;
                params.add(
                    Params::OpacityStart,
                    "Opacity Start",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.0);
                        f.set_valid_max(100.0);
                        f.set_slider_min(0.0);
                        f.set_slider_max(100.0);
                        f.set_default(100.0);
                        f.set_precision(1);
                    }),
                )?;
                params.add(
                    Params::OpacityMidA,
                    "Opacity 33%",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.0);
                        f.set_valid_max(100.0);
                        f.set_slider_min(0.0);
                        f.set_slider_max(100.0);
                        f.set_default(90.0);
                        f.set_precision(1);
                    }),
                )?;
                params.add(
                    Params::OpacityMidB,
                    "Opacity 66%",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.0);
                        f.set_valid_max(100.0);
                        f.set_slider_min(0.0);
                        f.set_slider_max(100.0);
                        f.set_default(45.0);
                        f.set_precision(1);
                    }),
                )?;
                params.add(
                    Params::OpacityEnd,
                    "Opacity End",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.0);
                        f.set_valid_max(100.0);
                        f.set_slider_min(0.0);
                        f.set_slider_max(100.0);
                        f.set_default(0.0);
                        f.set_precision(1);
                    }),
                )?;
                params.add_with_flags(
                    Params::SizeCurvePreset,
                    "Size Curve",
                    ae::PopupDef::setup(|f| {
                        f.set_options(&[
                            "Custom",
                            "Constant",
                            "Shrink",
                            "Grow-Shrink",
                            "Grow",
                            "Pop-Shrink",
                        ]);
                        f.set_default(3); // Shrink
                    }),
                    ae::ParamFlag::SUPERVISE,
                    ae::ParamUIFlags::empty(),
                )?;
                params.add(
                    Params::SizeLifeStart,
                    "Size 0%",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.0);
                        f.set_valid_max(5.0);
                        f.set_slider_min(0.0);
                        f.set_slider_max(3.0);
                        f.set_default(1.0);
                        f.set_precision(2);
                    }),
                )?;
                params.add(
                    Params::SizeLifeMidA,
                    "Size 33%",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.0);
                        f.set_valid_max(5.0);
                        f.set_slider_min(0.0);
                        f.set_slider_max(3.0);
                        f.set_default(1.0);
                        f.set_precision(2);
                    }),
                )?;
                params.add(
                    Params::SizeLifeMidB,
                    "Size 66%",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.0);
                        f.set_valid_max(5.0);
                        f.set_slider_min(0.0);
                        f.set_slider_max(3.0);
                        f.set_default(0.65);
                        f.set_precision(2);
                    }),
                )?;
                params.add(
                    Params::SizeLifeEnd,
                    "Size 100%",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.0);
                        f.set_valid_max(5.0);
                        f.set_slider_min(0.0);
                        f.set_slider_max(3.0);
                        f.set_default(0.35);
                        f.set_precision(2);
                    }),
                )?;
                Ok(())
            },
        )?;

        params.add_group(
            Params::RenderingGroupStart,
            Params::RenderingGroupEnd,
            "Rendering",
            false,
            |params| {
                params.add_with_flags(
                    Params::Shape,
                    "Shape",
                    ae::PopupDef::setup(|f| {
                        f.set_options(&["Circle", "Square", "Triangle", "Star", "Line", "Image"]);
                        f.set_default(1);
                    }),
                    ae::ParamFlag::SUPERVISE,
                    ae::ParamUIFlags::empty(),
                )?;
                params.add(
                    Params::SpriteSourceLayer,
                    "Sprite Source",
                    ae::LayerDef::new(),
                )?;
                params.add(
                    Params::SpriteTimeSampling,
                    "Time Sampling",
                    ae::PopupDef::setup(|f| {
                        f.set_options(&[
                            "Current Time",
                            "Birth Time",
                            "Random - Still",
                            "Random - Play",
                            "Cycle",
                        ]);
                        f.set_default(1);
                    }),
                )?;
                params.add(
                    Params::SpriteFrameCount,
                    "Frame Count",
                    ae::SliderDef::setup(|f| {
                        f.set_valid_min(1);
                        f.set_valid_max(100);
                        f.set_slider_min(1);
                        f.set_slider_max(30);
                        f.set_default(1);
                    }),
                )?;
                params.add(
                    Params::ImageProxyScale,
                    "Image Proxy",
                    ae::PopupDef::setup(|f| {
                        f.set_options(&["Full", "/2", "/4", "/8"]);
                        f.set_default(3);
                    }),
                )?;
                params.add(
                    Params::EdgeSoftness,
                    "Edge Softness",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.0);
                        f.set_valid_max(1.0);
                        f.set_slider_min(0.0);
                        f.set_slider_max(1.0);
                        f.set_default(0.0);
                        f.set_precision(2);
                    }),
                )?;
                params.add(
                    Params::ImageColorMode,
                    "Image Color",
                    ae::PopupDef::setup(|f| {
                        f.set_options(&["Tint", "Source"]);
                        f.set_default(1);
                    }),
                )?;
                params.add(
                    Params::ImageFitMode,
                    "Image Fit",
                    ae::PopupDef::setup(|f| {
                        f.set_options(&["Contain", "Stretch"]);
                        f.set_default(1);
                    }),
                )?;
                params.add(
                    Params::UseSourceAlpha,
                    "Use Source Alpha",
                    ae::CheckBoxDef::setup(|f| {
                        f.set_default(true);
                        f.set_label("Enable");
                    }),
                )?;
                params.add(
                    Params::SourcePremultiplied,
                    "Source Premultiplied",
                    ae::CheckBoxDef::setup(|f| {
                        f.set_default(true);
                        f.set_label("Enable");
                    }),
                )?;
                params.add(
                    Params::ImageAlphaClip,
                    "Alpha Clip",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.0);
                        f.set_valid_max(1.0);
                        f.set_slider_min(0.0);
                        f.set_slider_max(1.0);
                        f.set_default(0.01);
                        f.set_precision(2);
                    }),
                )?;
                params.add(
                    Params::BlendModeParam,
                    "Blend Mode",
                    ae::PopupDef::setup(|f| {
                        f.set_options(&["Normal", "Add", "Screen"]);
                        f.set_default(1);
                    }),
                )?;
                params.add(
                    Params::MotionBlur,
                    "Motion Blur",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.0);
                        f.set_valid_max(1.0);
                        f.set_slider_min(0.0);
                        f.set_slider_max(1.0);
                        f.set_default(0.2);
                        f.set_precision(2);
                    }),
                )?;
                params.add(
                    Params::DOFEnabled,
                    "Depth of Field",
                    ae::CheckBoxDef::setup(|f| {
                        f.set_default(false);
                        f.set_label("Enable");
                    }),
                )?;
                params.add(
                    Params::DOFFocalDist,
                    "DOF Focal Distance",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.0);
                        f.set_valid_max(10000.0);
                        f.set_slider_min(0.0);
                        f.set_slider_max(1000.0);
                        f.set_default(0.0);
                        f.set_precision(1);
                    }),
                )?;
                params.add(
                    Params::DOFAperture,
                    "DOF Aperture",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.0);
                        f.set_valid_max(100.0);
                        f.set_slider_min(0.0);
                        f.set_slider_max(50.0);
                        f.set_default(5.0);
                        f.set_precision(1);
                    }),
                )?;
                params.add(
                    Params::SizeMultiplier,
                    "Size Multiplier",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.01);
                        f.set_valid_max(10.0);
                        f.set_slider_min(0.1);
                        f.set_slider_max(5.0);
                        f.set_default(1.15);
                        f.set_precision(2);
                    }),
                )?;
                params.add(
                    Params::CompositeOnOrig,
                    "Composite on Original",
                    ae::CheckBoxDef::setup(|f| {
                        f.set_default(true);
                        f.set_label("Enable");
                    }),
                )?;
                params.add(
                    Params::ApplyMode,
                    "Apply Mode",
                    ae::PopupDef::setup(|f| {
                        f.set_options(&["On Transparent", "Normal", "Add", "Screen"]);
                        f.set_default(2);
                    }),
                )?;
                Ok(())
            },
        )?;

        params.add_group(
            Params::ChildGroupStart,
            Params::ChildGroupEnd,
            "Child Particles",
            true,
            |params| {
                params.add(
                    Params::ChildEnabled,
                    "Child Particles",
                    ae::CheckBoxDef::setup(|f| {
                        f.set_default(false);
                        f.set_label("Enable");
                    }),
                )?;
                params.add(
                    Params::ChildCount,
                    "Child Count",
                    ae::SliderDef::setup(|f| {
                        f.set_valid_min(0);
                        f.set_valid_max(20);
                        f.set_slider_min(0);
                        f.set_slider_max(10);
                        f.set_default(3);
                    }),
                )?;
                params.add(
                    Params::ChildInheritVel,
                    "Child Inherit Velocity",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.0);
                        f.set_valid_max(1.0);
                        f.set_slider_min(0.0);
                        f.set_slider_max(1.0);
                        f.set_default(0.65);
                        f.set_precision(2);
                    }),
                )?;
                params.add(
                    Params::ChildLifespan,
                    "Child Lifespan",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.01);
                        f.set_valid_max(10.0);
                        f.set_slider_min(0.1);
                        f.set_slider_max(5.0);
                        f.set_default(0.5);
                        f.set_precision(2);
                    }),
                )?;
                params.add(
                    Params::ChildSpeed,
                    "Child Speed",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.0);
                        f.set_valid_max(10000.0);
                        f.set_slider_min(0.0);
                        f.set_slider_max(2000.0);
                        f.set_default(80.0);
                        f.set_precision(1);
                    }),
                )?;
                params.add(
                    Params::ChildSpread,
                    "Child Spread (deg)",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.0);
                        f.set_valid_max(180.0);
                        f.set_slider_min(0.0);
                        f.set_slider_max(180.0);
                        f.set_default(110.0);
                        f.set_precision(1);
                    }),
                )?;
                params.add(
                    Params::ChildSizeScale,
                    "Child Size Scale",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.01);
                        f.set_valid_max(5.0);
                        f.set_slider_min(0.1);
                        f.set_slider_max(2.0);
                        f.set_default(0.4);
                        f.set_precision(2);
                    }),
                )?;
                Ok(())
            },
        )?;

        params.add_group(
            Params::SystemGroupStart,
            Params::SystemGroupEnd,
            "System",
            true,
            |params| {
                params.add(
                    Params::Seed,
                    "Random Seed",
                    ae::SliderDef::setup(|f| {
                        f.set_valid_min(0);
                        f.set_valid_max(99999);
                        f.set_slider_min(0);
                        f.set_slider_max(99999);
                        f.set_default(12345);
                    }),
                )?;
                Ok(())
            },
        )?;

        add_legacy_plexus_params(params)?;
        add_published_host_params(params)?;
        add_node_graph_tool_params(params)?;
        add_node_ui_sidecar_params(params)?;

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
            if command_is_sequence_owned(&cmd) {
                return Ok(());
            }

            match cmd {
                ae::Command::About => {
                    out_data.set_return_msg(
                        "Particle Kit v1.0\rOpen-source particle effect plugin for After Effects.\rEmitters, physics, child particles.\rWritten in Rust.",
                    );
                    Ok(())
                }

                ae::Command::GlobalSetup => {
                    out_data.set_out_flag(ae::OutFlags::IExpandBuffer, true);
                    out_data.set_out_flag(ae::OutFlags::SendUpdateParamsUi, true);
                    out_data.set_out_flag(ae::OutFlags::NonParamVary, true);
                    out_data.set_out_flag(ae::OutFlags::SequenceDataNeedsFlattening, true);
                    out_data.set_out_flag2(ae::OutFlags2::ParamGroupStartCollapsedFlag, true);
                    out_data.set_out_flag2(ae::OutFlags2::IUse3DCamera, true);
                    out_data.set_out_flag2(ae::OutFlags2::SupportsSmartRender, true);
                    out_data.set_out_flag2(ae::OutFlags2::SupportsGetFlattenedSequenceData, true);
                    Ok(())
                }

                ae::Command::Render { .. } => Ok(()),

                ae::Command::SmartPreRender { .. } => Ok(()),

                ae::Command::SmartRender { extra } => {
                    let render_start = Instant::now();
                    match smart_render_particles(render_start, &in_data, &extra) {
                        Ok(()) => Ok(()),
                        // Any error path  Ecancellation OR a real failure  E                        // must be surfaced to AE as `InterruptCancel`.
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
                                "SmartRender error: {:?}  Esignalling InterruptCancel to avoid cache poisoning",
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
                        Params::SavePreset
                            | Params::LoadPreset
                            | Params::DeletePreset
                            | Params::OpenPresetFolder
                    ) {
                        handle_preset_command(changed, params, &mut out_data)?;
                    }
                    if matches!(
                        changed,
                        Params::EmitterSizeX
                            | Params::EmitterSizeY
                            | Params::EmitterSizeZ
                            | Params::EmitterSizeLinked
                    ) {
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
                        out_data.set_return_msg("Particle Kit image cache refreshed.");
                        debug_info("RefreshImageCache button pressed");
                    }

                    if should_refresh_ui(changed) {
                        out_data.set_out_flag(ae::OutFlags::RefreshUi, true);
                    }
                    debug_info(format!("UserChangedParam index={}", param_index));
                    Ok(())
                }

                ae::Command::UpdateParamsUi => {
                    update_shape_dependent_ui(params, None)?;
                    Ok(())
                }

                ae::Command::Event { .. } => Ok(()),

                _ => Ok(()),
            }
        })) {
            Ok(result) => result,
            Err(payload) => {
                debug_error(format!(
                    "{} panicked: {}",
                    cmd_name,
                    panic_payload_message(payload)
                ));
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

impl AdobePluginInstance for ParticleLabProjectState {
    fn flatten(&self) -> Result<(u16, Vec<u8>), ae::Error> {
        self.flatten_bytes()
            .map(|bytes| (PROJECT_STATE_VERSION, bytes))
            .map_err(|_| ae::Error::Generic)
    }

    fn unflatten(version: u16, serialized: &[u8]) -> Result<Self, ae::Error> {
        ParticleLabProjectState::unflatten_bytes(version, serialized)
            .map_err(|_| ae::Error::Generic)
    }

    fn render(
        &self,
        plugin: &mut PluginState,
        in_layer: &ae::Layer,
        out_layer: &mut ae::Layer,
    ) -> Result<(), ae::Error> {
        render_particles(plugin.params, self, &plugin.in_data, in_layer, out_layer)
    }

    fn handle_command(
        &mut self,
        plugin: &mut PluginState,
        command: ae::Command,
    ) -> Result<(), ae::Error> {
        match command {
            ae::Command::SmartPreRender { mut extra } => {
                smart_pre_render_particles(plugin.params, self, &plugin.in_data, &mut extra)
            }
            ae::Command::SmartRender { extra } => {
                let render_start = Instant::now();
                match smart_render_particles(render_start, &plugin.in_data, &extra) {
                    Ok(()) => Ok(()),
                    Err(ae::Error::InterruptCancel) => {
                        debug_info("SmartRender interrupted (cancel)");
                        Err(ae::Error::InterruptCancel)
                    }
                    Err(err) => {
                        debug_error(format!(
                            "SmartRender error: {:?} - signalling InterruptCancel to avoid cache poisoning",
                            err
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
                handle_user_changed_param(param_index, plugin.params, self, &mut plugin.out_data)
            }
            ae::Command::UpdateParamsUi => {
                update_shape_dependent_ui(plugin.params, Some(self))?;
                Ok(())
            }
            _ => Ok(()),
        }
    }
}

// ---- Extract all parameters into configs ----

#[cfg(windows)]
fn preset_root_dir() -> Result<PathBuf, ae::Error> {
    let userprofile = std::env::var("USERPROFILE").map_err(|_| ae::Error::Generic)?;
    Ok(PathBuf::from(userprofile)
        .join("Documents")
        .join("Particle Kit")
        .join("presets"))
}

#[cfg(not(windows))]
fn preset_root_dir() -> Result<std::path::PathBuf, ae::Error> {
    Err(ae::Error::Generic)
}

#[cfg(windows)]
fn node_graph_state_root_dir() -> Result<PathBuf, ae::Error> {
    let userprofile = std::env::var("USERPROFILE").map_err(|_| ae::Error::Generic)?;
    Ok(PathBuf::from(userprofile)
        .join("Documents")
        .join("Particle Kit")
        .join("node-graphs"))
}

#[cfg(not(windows))]
fn node_graph_state_root_dir() -> Result<std::path::PathBuf, ae::Error> {
    Err(ae::Error::Generic)
}

fn node_ui_shell_assets() -> [(&'static str, &'static str); 4] {
    [
        ("index.html", NODE_UI_SHELL_INDEX_HTML),
        ("app.js", NODE_UI_SHELL_APP_JS),
        ("styles.css", NODE_UI_SHELL_STYLES_CSS),
        ("startup-payload.js", NODE_UI_SHELL_STARTUP_JS),
    ]
}

fn node_ui_shell_startup_payload_js(
    project_state: &ParticleLabProjectState,
) -> Result<String, ae::Error> {
    let payload = project_state.node_ui_bootstrap_payload();
    let json = serde_json::to_string_pretty(&payload).map_err(|_| ae::Error::Generic)?;
    Ok(format!(
        "window.PARTICLELAB_NODE_UI_BOOTSTRAP = {};\nwindow.PARTICLELAB_NODE_UI_BOOTSTRAP_SOURCE = \"AE sidecar startup payload\";\n",
        json
    ))
}

#[cfg(windows)]
fn node_ui_shell_root_dir() -> Result<PathBuf, ae::Error> {
    let userprofile = std::env::var("USERPROFILE").map_err(|_| ae::Error::Generic)?;
    Ok(PathBuf::from(userprofile)
        .join("Documents")
        .join("Particle Kit")
        .join("node-ui-shell"))
}

#[cfg(not(windows))]
fn node_ui_shell_root_dir() -> Result<std::path::PathBuf, ae::Error> {
    Err(ae::Error::Generic)
}

#[cfg(windows)]
fn install_node_ui_shell_assets(
    project_state: &ParticleLabProjectState,
) -> Result<PathBuf, ae::Error> {
    let dir = node_ui_shell_root_dir()?;
    fs::create_dir_all(&dir).map_err(|_| ae::Error::Generic)?;
    for (filename, contents) in node_ui_shell_assets() {
        fs::write(dir.join(filename), contents).map_err(|_| ae::Error::Generic)?;
    }
    fs::write(
        dir.join("startup-payload.js"),
        node_ui_shell_startup_payload_js(project_state)?,
    )
    .map_err(|_| ae::Error::Generic)?;
    Ok(dir.join("index.html"))
}

#[cfg(not(windows))]
fn install_node_ui_shell_assets(
    _project_state: &ParticleLabProjectState,
) -> Result<std::path::PathBuf, ae::Error> {
    Err(ae::Error::Generic)
}

#[cfg(windows)]
fn open_node_ui_shell_sidecar(
    project_state: &ParticleLabProjectState,
) -> Result<PathBuf, ae::Error> {
    let index_path = install_node_ui_shell_assets(project_state)?;
    std::process::Command::new("explorer.exe")
        .arg(index_path.as_os_str())
        .spawn()
        .map_err(|_| ae::Error::Generic)?;
    Ok(index_path)
}

#[cfg(not(windows))]
fn open_node_ui_shell_sidecar(
    _project_state: &ParticleLabProjectState,
) -> Result<std::path::PathBuf, ae::Error> {
    Err(ae::Error::Generic)
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

fn timestamped_json_name(prefix: &str) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let days = now / 86400;
    let tod = now % 86400;
    let (y, m, d) = epoch_days_to_date(days as i64);
    format!(
        "{}_{:04}{:02}{:02}_{:02}{:02}{:02}.json",
        prefix,
        y,
        m,
        d,
        tod / 3600,
        (tod % 3600) / 60,
        tod % 60
    )
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
            let dir = preset_root_dir()?;
            fs::create_dir_all(&dir).map_err(|_| ae::Error::Generic)?;
            let dir_str = dir.to_string_lossy().to_string();

            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs());
            let days = now / 86400;
            let tod = now % 86400;
            let (y, m, d) = epoch_days_to_date(days as i64);
            let default_name = format!(
                "preset_{:04}{:02}{:02}_{:02}{:02}{:02}.json",
                y,
                m,
                d,
                tod / 3600,
                (tod % 3600) / 60,
                tod % 60
            );

            if let Some(path) =
                file_dialog::save_file_dialog(&dir_str, "Save Preset", &default_name)
            {
                let mut snapshot = capture_preset(params)?;
                snapshot.name = path
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                let json =
                    serde_json::to_string_pretty(&snapshot).map_err(|_| ae::Error::Generic)?;
                fs::write(&path, json).map_err(|_| ae::Error::Generic)?;
                out_data.set_return_msg(&format!(
                    "Preset saved: {}",
                    path.file_name().unwrap_or_default().to_string_lossy()
                ));
            }
        }
        Params::LoadPreset => {
            let dir = preset_root_dir()?;
            fs::create_dir_all(&dir).map_err(|_| ae::Error::Generic)?;
            let dir_str = dir.to_string_lossy().to_string();

            if let Some(path) = file_dialog::open_file_dialog(&dir_str, "Load Preset") {
                let contents = fs::read_to_string(&path).map_err(|_| ae::Error::Generic)?;
                let snapshot = load_preset_snapshot(&contents).map_err(|_| ae::Error::Generic)?;
                let _ = snapshot.to_engine_config();
                apply_preset(params, &snapshot)?;
                invalidate_image_cache();
                let label = path
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                out_data.set_return_msg(&format!("Preset loaded: {}", label));
            }
        }
        Params::DeletePreset => {
            let dir = preset_root_dir()?;
            fs::create_dir_all(&dir).map_err(|_| ae::Error::Generic)?;
            let dir_str = dir.to_string_lossy().to_string();

            if let Some(path) = file_dialog::open_file_dialog(&dir_str, "Delete Preset") {
                let name = path
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                fs::remove_file(&path).map_err(|_| ae::Error::Generic)?;
                out_data.set_return_msg(&format!("Deleted: {}", name));
            }
        }
        Params::OpenPresetFolder => {
            open_preset_folder()?;
        }
        _ => {}
    }
    Ok(())
}

fn handle_preset_command_with_project_state(
    changed: Params,
    params: &ae::Parameters<Params>,
    project_state: &mut ParticleLabProjectState,
    out_data: &mut ae::OutData,
) -> Result<(), ae::Error> {
    match changed {
        Params::SavePreset => {
            let dir = preset_root_dir()?;
            fs::create_dir_all(&dir).map_err(|_| ae::Error::Generic)?;
            let dir_str = dir.to_string_lossy().to_string();

            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs());
            let days = now / 86400;
            let tod = now % 86400;
            let (y, m, d) = epoch_days_to_date(days as i64);
            let default_name = format!(
                "preset_{:04}{:02}{:02}_{:02}{:02}{:02}.json",
                y,
                m,
                d,
                tod / 3600,
                (tod % 3600) / 60,
                tod % 60
            );

            if let Some(path) =
                file_dialog::save_file_dialog(&dir_str, "Save Preset", &default_name)
            {
                let mut snapshot = capture_preset(params)?;
                snapshot.name = path
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                snapshot.graph_document = project_state.graph.document.clone();
                if snapshot.graph_document.is_some() {
                    snapshot.graph_published_values = project_state.graph.published_values.clone();
                }
                let json =
                    serde_json::to_string_pretty(&snapshot).map_err(|_| ae::Error::Generic)?;
                fs::write(&path, json).map_err(|_| ae::Error::Generic)?;
                out_data.set_return_msg(&format!(
                    "Preset saved: {}",
                    path.file_name().unwrap_or_default().to_string_lossy()
                ));
            }
        }
        Params::LoadPreset => {
            let dir = preset_root_dir()?;
            fs::create_dir_all(&dir).map_err(|_| ae::Error::Generic)?;
            let dir_str = dir.to_string_lossy().to_string();

            if let Some(path) = file_dialog::open_file_dialog(&dir_str, "Load Preset") {
                let contents = fs::read_to_string(&path).map_err(|_| ae::Error::Generic)?;
                let snapshot = load_preset_snapshot(&contents).map_err(|_| ae::Error::Generic)?;
                let _ = snapshot.to_engine_config();
                apply_preset(params, &snapshot)?;
                project_state.replace_graph_document(snapshot.graph_document.clone());
                if snapshot.graph_document.is_some() {
                    project_state.graph.published_values = snapshot.graph_published_values.clone();
                }
                invalidate_image_cache();
                let label = path
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                out_data.set_return_msg(&format!("Preset loaded: {}", label));
            }
        }
        Params::DeletePreset | Params::OpenPresetFolder => {
            handle_preset_command(changed, params, out_data)?;
        }
        _ => {}
    }
    Ok(())
}

fn handle_node_graph_tool_command(
    changed: Params,
    params: &ae::Parameters<Params>,
    project_state: &mut ParticleLabProjectState,
    out_data: &mut ae::OutData,
) -> Result<(), ae::Error> {
    match changed {
        Params::ExportNodeGraphState => {
            let dir = node_graph_state_root_dir()?;
            fs::create_dir_all(&dir).map_err(|_| ae::Error::Generic)?;
            let dir_str = dir.to_string_lossy().to_string();
            let default_name = timestamped_json_name("node_graph");
            if let Some(path) =
                file_dialog::save_file_dialog(&dir_str, "Export Node Graph", &default_name)
            {
                let payload = project_state.node_ui_bootstrap_payload();
                let json =
                    serde_json::to_string_pretty(&payload).map_err(|_| ae::Error::Generic)?;
                fs::write(&path, json).map_err(|_| ae::Error::Generic)?;
                out_data.set_return_msg(&format!(
                    "Node graph exported: {}",
                    path.file_name().unwrap_or_default().to_string_lossy()
                ));
            }
        }
        Params::ImportNodeGraphState => {
            let dir = node_graph_state_root_dir()?;
            fs::create_dir_all(&dir).map_err(|_| ae::Error::Generic)?;
            let dir_str = dir.to_string_lossy().to_string();
            if let Some(path) = file_dialog::open_file_dialog(&dir_str, "Import Node Graph") {
                let contents = fs::read_to_string(&path).map_err(|_| ae::Error::Generic)?;
                let snapshot = match NodeUiGraphStateSnapshot::from_node_ui_json(&contents) {
                    Ok(snapshot) => snapshot,
                    Err(err) => {
                        debug_error(format!("Node graph import JSON error: {:?}", err));
                        out_data.set_return_msg("Node graph import failed: invalid JSON.");
                        return Ok(());
                    }
                };
                if let Err(err) = project_state.commit_node_ui_graph_state_snapshot(snapshot) {
                    debug_error(format!("Node graph import rejected: {:?}", err));
                    out_data.set_return_msg("Node graph import failed: incompatible graph.");
                    return Ok(());
                }
                let label = path
                    .file_stem()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .to_string();
                update_shape_dependent_ui(params, Some(project_state))?;
                out_data.set_return_msg(&format!("Node graph imported: {}", label));
                out_data.set_out_flag(ae::OutFlags::RefreshUi, true);
                invalidate_image_cache();
            }
        }
        Params::SeedNodeGraphFromParams => {
            let config = extract_engine_config(params)?.normalized_for_render();
            project_state.replace_graph_from_engine_config(&config);
            update_shape_dependent_ui(params, Some(project_state))?;
            out_data.set_return_msg("Node graph seeded from current ParticleLab params.");
            out_data.set_out_flag(ae::OutFlags::RefreshUi, true);
            invalidate_image_cache();
        }
        Params::DisableNodeGraph => {
            project_state.set_graph_enabled(false);
            update_shape_dependent_ui(params, Some(project_state))?;
            out_data.set_return_msg("Node graph disabled; classic ParticleLab params are active.");
            out_data.set_out_flag(ae::OutFlags::RefreshUi, true);
            invalidate_image_cache();
        }
        _ => {}
    }
    Ok(())
}

fn handle_node_ui_sidecar_command(
    changed: Params,
    project_state: &ParticleLabProjectState,
    out_data: &mut ae::OutData,
) -> Result<(), ae::Error> {
    match changed {
        Params::OpenNodeUiShell => match open_node_ui_shell_sidecar(project_state) {
            Ok(path) => {
                out_data.set_return_msg(&format!(
                    "Node UI shell opened: {}",
                    path.file_name().unwrap_or_default().to_string_lossy()
                ));
            }
            Err(err) => {
                debug_error(format!("Node UI shell open failed: {:?}", err));
                out_data.set_return_msg("Node UI shell open failed.");
                return Err(err);
            }
        },
        _ => {}
    }
    Ok(())
}

fn handle_user_changed_param(
    param_index: usize,
    params: &ae::Parameters<Params>,
    project_state: &mut ParticleLabProjectState,
    out_data: &mut ae::OutData,
) -> Result<(), ae::Error> {
    let changed = params.type_at(param_index);
    if matches!(
        changed,
        Params::SavePreset | Params::LoadPreset | Params::DeletePreset | Params::OpenPresetFolder
    ) {
        handle_preset_command_with_project_state(changed, params, project_state, out_data)?;
        if changed == Params::LoadPreset {
            update_shape_dependent_ui(params, Some(project_state))?;
        }
    }
    if matches!(
        changed,
        Params::ExportNodeGraphState
            | Params::ImportNodeGraphState
            | Params::SeedNodeGraphFromParams
            | Params::DisableNodeGraph
    ) {
        handle_node_graph_tool_command(changed, params, project_state, out_data)?;
    }
    if matches!(changed, Params::OpenNodeUiShell) {
        handle_node_ui_sidecar_command(changed, project_state, out_data)?;
    }
    if matches!(
        changed,
        Params::EmitterSizeX
            | Params::EmitterSizeY
            | Params::EmitterSizeZ
            | Params::EmitterSizeLinked
    ) {
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
        out_data.set_return_msg("Particle Kit image cache refreshed.");
        debug_info("RefreshImageCache button pressed");
    }

    if should_refresh_ui(changed) {
        out_data.set_out_flag(ae::OutFlags::RefreshUi, true);
    }
    debug_info(format!("UserChangedParam index={}", param_index));
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
    let name = format!(
        "preset_{:04}{:02}{:02}_{:02}{:02}{:02}",
        year, month, day, hours, minutes, seconds
    );

    Ok(PresetSnapshot {
        version: PRESET_VERSION,
        name,
        graph_document: None,
        graph_published_values: Vec::new(),
        emitter_type: params.get(Params::EmitterType)?.as_popup()?.value(),
        emit_mode: params.get(Params::EmitMode)?.as_popup()?.value(),
        position_point: params.get(Params::PositionPoint)?.as_point()?.value(),
        position_z: params.get(Params::PositionZ)?.as_float_slider()?.value(),
        image_proxy_scale: params.get(Params::ImageProxyScale)?.as_popup()?.value(),
        path_sample_density: params
            .get(Params::PathSampleDensity)?
            .as_float_slider()?
            .value(),
        grid_res_x: params.get(Params::GridResX)?.as_slider()?.value(),
        grid_res_y: params.get(Params::GridResY)?.as_slider()?.value(),
        grid_res_z: params.get(Params::GridResZ)?.as_slider()?.value(),
        emitter_size_linked: params
            .get(Params::EmitterSizeLinked)?
            .as_checkbox()?
            .value(),
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
        rotation_speed: params
            .get(Params::RotationSpeed)?
            .as_float_slider()?
            .value(),
        gravity_strength: params
            .get(Params::GravityStrength)?
            .as_float_slider()?
            .value(),
        wind_x: params.get(Params::WindX)?.as_float_slider()?.value(),
        wind_y: params.get(Params::WindY)?.as_float_slider()?.value(),
        turb_strength: params.get(Params::TurbStrength)?.as_float_slider()?.value(),
        turb_scale: params.get(Params::TurbScale)?.as_float_slider()?.value(),
        turb_speed: params.get(Params::TurbSpeed)?.as_float_slider()?.value(),
        air_resistance: params
            .get(Params::AirResistance)?
            .as_float_slider()?
            .value(),
        bounce_enabled: params.get(Params::BounceEnabled)?.as_checkbox()?.value(),
        bounce_damping: params
            .get(Params::BounceDamping)?
            .as_float_slider()?
            .value(),
        color_mode: params.get(Params::ColorMode)?.as_popup()?.value(),
        color_start: params.get(Params::ColorStart)?.as_color()?.value().into(),
        color_end: params.get(Params::ColorEnd)?.as_color()?.value().into(),
        opacity_curve_preset: params.get(Params::OpacityCurvePreset)?.as_popup()?.value(),
        opacity_start: params.get(Params::OpacityStart)?.as_float_slider()?.value(),
        opacity_mid_a: params.get(Params::OpacityMidA)?.as_float_slider()?.value(),
        opacity_mid_b: params.get(Params::OpacityMidB)?.as_float_slider()?.value(),
        opacity_end: params.get(Params::OpacityEnd)?.as_float_slider()?.value(),
        size_curve_preset: params.get(Params::SizeCurvePreset)?.as_popup()?.value(),
        size_life_start: params
            .get(Params::SizeLifeStart)?
            .as_float_slider()?
            .value(),
        size_life_mid_a: params.get(Params::SizeLifeMidA)?.as_float_slider()?.value(),
        size_life_mid_b: params.get(Params::SizeLifeMidB)?.as_float_slider()?.value(),
        size_life_end: params.get(Params::SizeLifeEnd)?.as_float_slider()?.value(),
        shape: params.get(Params::Shape)?.as_popup()?.value(),
        sprite_time_sampling: params.get(Params::SpriteTimeSampling)?.as_popup()?.value(),
        sprite_frame_count: params.get(Params::SpriteFrameCount)?.as_slider()?.value(),
        image_color_mode: params.get(Params::ImageColorMode)?.as_popup()?.value(),
        image_fit_mode: params.get(Params::ImageFitMode)?.as_popup()?.value(),
        use_source_alpha: params.get(Params::UseSourceAlpha)?.as_checkbox()?.value(),
        source_premultiplied: params
            .get(Params::SourcePremultiplied)?
            .as_checkbox()?
            .value(),
        image_alpha_clip: params
            .get(Params::ImageAlphaClip)?
            .as_float_slider()?
            .value(),
        blend_mode: params.get(Params::BlendModeParam)?.as_popup()?.value(),
        motion_blur: params.get(Params::MotionBlur)?.as_float_slider()?.value(),
        edge_softness: params.get(Params::EdgeSoftness)?.as_float_slider()?.value(),
        dof_enabled: params.get(Params::DOFEnabled)?.as_checkbox()?.value(),
        dof_focal_dist: params.get(Params::DOFFocalDist)?.as_float_slider()?.value(),
        dof_aperture: params.get(Params::DOFAperture)?.as_float_slider()?.value(),
        size_multiplier: params
            .get(Params::SizeMultiplier)?
            .as_float_slider()?
            .value(),
        composite_on_orig: params.get(Params::CompositeOnOrig)?.as_checkbox()?.value(),
        apply_mode: params.get(Params::ApplyMode)?.as_popup()?.value(),
        rotation_variation: params.get(Params::RotationVar)?.as_float_slider()?.value(),
        opacity_variation: params.get(Params::OpacityVar)?.as_float_slider()?.value(),
        child_enabled: params.get(Params::ChildEnabled)?.as_checkbox()?.value(),
        child_count: params.get(Params::ChildCount)?.as_slider()?.value(),
        child_inherit_vel: params
            .get(Params::ChildInheritVel)?
            .as_float_slider()?
            .value(),
        child_lifespan: params
            .get(Params::ChildLifespan)?
            .as_float_slider()?
            .value(),
        child_speed: params.get(Params::ChildSpeed)?.as_float_slider()?.value(),
        child_spread: params.get(Params::ChildSpread)?.as_float_slider()?.value(),
        child_size_scale: params
            .get(Params::ChildSizeScale)?
            .as_float_slider()?
            .value(),
        seed: params.get(Params::Seed)?.as_slider()?.value(),
    })
}

fn set_popup_param(
    params: &mut ae::Parameters<Params>,
    id: Params,
    value: i32,
) -> Result<(), ae::Error> {
    let mut param = params.get_mut(id)?;
    param.as_popup_mut()?.set_value(value);
    param.set_change_flag(ae::ChangeFlag::CHANGED_VALUE, true);
    Ok(())
}

fn set_float_param(
    params: &mut ae::Parameters<Params>,
    id: Params,
    value: f64,
) -> Result<(), ae::Error> {
    let mut param = params.get_mut(id)?;
    param.as_float_slider_mut()?.set_value(value);
    param.set_change_flag(ae::ChangeFlag::CHANGED_VALUE, true);
    Ok(())
}

fn set_slider_param(
    params: &mut ae::Parameters<Params>,
    id: Params,
    value: i32,
) -> Result<(), ae::Error> {
    let mut param = params.get_mut(id)?;
    param.as_slider_mut()?.set_value(value);
    param.set_change_flag(ae::ChangeFlag::CHANGED_VALUE, true);
    Ok(())
}

fn set_checkbox_param(
    params: &mut ae::Parameters<Params>,
    id: Params,
    value: bool,
) -> Result<(), ae::Error> {
    let mut param = params.get_mut(id)?;
    param.as_checkbox_mut()?.set_value(value);
    param.set_change_flag(ae::ChangeFlag::CHANGED_VALUE, true);
    Ok(())
}

fn set_color_param(
    params: &mut ae::Parameters<Params>,
    id: Params,
    value: PresetColor,
) -> Result<(), ae::Error> {
    let mut param = params.get_mut(id)?;
    param.as_color_mut()?.set_value(value.into());
    param.set_change_flag(ae::ChangeFlag::CHANGED_VALUE, true);
    Ok(())
}

fn set_point_param(
    params: &mut ae::Parameters<Params>,
    id: Params,
    value: (f32, f32),
) -> Result<(), ae::Error> {
    let mut param = params.get_mut(id)?;
    param.as_point_mut()?.set_value(value);
    param.set_change_flag(ae::ChangeFlag::CHANGED_VALUE, true);
    Ok(())
}

fn apply_preset(params: &ae::Parameters<Params>, preset: &PresetSnapshot) -> Result<(), ae::Error> {
    let mut params_copy = params.cloned();

    set_popup_param(&mut params_copy, Params::EmitterType, preset.emitter_type)?;
    set_popup_param(&mut params_copy, Params::EmitMode, preset.emit_mode)?;
    set_point_param(
        &mut params_copy,
        Params::PositionPoint,
        preset.position_point,
    )?;
    set_float_param(&mut params_copy, Params::PositionZ, preset.position_z)?;
    set_popup_param(
        &mut params_copy,
        Params::ImageProxyScale,
        preset.image_proxy_scale,
    )?;
    set_checkbox_param(
        &mut params_copy,
        Params::EmitterSizeLinked,
        preset.emitter_size_linked,
    )?;
    set_float_param(
        &mut params_copy,
        Params::EmitterSizeX,
        preset.emitter_size_x,
    )?;
    set_float_param(
        &mut params_copy,
        Params::EmitterSizeY,
        preset.emitter_size_y,
    )?;
    set_float_param(
        &mut params_copy,
        Params::EmitterSizeZ,
        preset.emitter_size_z,
    )?;
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
    set_float_param(
        &mut params_copy,
        Params::RotationSpeed,
        preset.rotation_speed,
    )?;
    set_float_param(
        &mut params_copy,
        Params::GravityStrength,
        preset.gravity_strength,
    )?;
    set_float_param(&mut params_copy, Params::WindX, preset.wind_x)?;
    set_float_param(&mut params_copy, Params::WindY, preset.wind_y)?;
    set_float_param(&mut params_copy, Params::TurbStrength, preset.turb_strength)?;
    set_float_param(&mut params_copy, Params::TurbScale, preset.turb_scale)?;
    set_float_param(&mut params_copy, Params::TurbSpeed, preset.turb_speed)?;
    set_float_param(
        &mut params_copy,
        Params::AirResistance,
        preset.air_resistance,
    )?;
    set_checkbox_param(
        &mut params_copy,
        Params::BounceEnabled,
        preset.bounce_enabled,
    )?;
    set_float_param(
        &mut params_copy,
        Params::BounceDamping,
        preset.bounce_damping,
    )?;
    set_popup_param(&mut params_copy, Params::ColorMode, preset.color_mode)?;
    set_color_param(&mut params_copy, Params::ColorStart, preset.color_start)?;
    set_color_param(&mut params_copy, Params::ColorEnd, preset.color_end)?;
    set_popup_param(
        &mut params_copy,
        Params::OpacityCurvePreset,
        preset.opacity_curve_preset,
    )?;
    set_float_param(&mut params_copy, Params::OpacityStart, preset.opacity_start)?;
    set_float_param(&mut params_copy, Params::OpacityMidA, preset.opacity_mid_a)?;
    set_float_param(&mut params_copy, Params::OpacityMidB, preset.opacity_mid_b)?;
    set_float_param(&mut params_copy, Params::OpacityEnd, preset.opacity_end)?;
    set_popup_param(
        &mut params_copy,
        Params::SizeCurvePreset,
        preset.size_curve_preset,
    )?;
    set_float_param(
        &mut params_copy,
        Params::SizeLifeStart,
        preset.size_life_start,
    )?;
    set_float_param(
        &mut params_copy,
        Params::SizeLifeMidA,
        preset.size_life_mid_a,
    )?;
    set_float_param(
        &mut params_copy,
        Params::SizeLifeMidB,
        preset.size_life_mid_b,
    )?;
    set_float_param(&mut params_copy, Params::SizeLifeEnd, preset.size_life_end)?;
    set_popup_param(&mut params_copy, Params::Shape, preset.shape)?;
    set_popup_param(
        &mut params_copy,
        Params::SpriteTimeSampling,
        preset.sprite_time_sampling,
    )?;
    set_slider_param(
        &mut params_copy,
        Params::SpriteFrameCount,
        preset.sprite_frame_count,
    )?;
    set_popup_param(
        &mut params_copy,
        Params::ImageColorMode,
        preset.image_color_mode,
    )?;
    set_popup_param(
        &mut params_copy,
        Params::ImageFitMode,
        preset.image_fit_mode,
    )?;
    set_checkbox_param(
        &mut params_copy,
        Params::UseSourceAlpha,
        preset.use_source_alpha,
    )?;
    set_checkbox_param(
        &mut params_copy,
        Params::SourcePremultiplied,
        preset.source_premultiplied,
    )?;
    set_float_param(
        &mut params_copy,
        Params::ImageAlphaClip,
        preset.image_alpha_clip,
    )?;
    set_popup_param(&mut params_copy, Params::BlendModeParam, preset.blend_mode)?;
    set_float_param(&mut params_copy, Params::MotionBlur, preset.motion_blur)?;
    set_float_param(&mut params_copy, Params::EdgeSoftness, preset.edge_softness)?;
    set_checkbox_param(&mut params_copy, Params::DOFEnabled, preset.dof_enabled)?;
    set_float_param(
        &mut params_copy,
        Params::DOFFocalDist,
        preset.dof_focal_dist,
    )?;
    set_float_param(&mut params_copy, Params::DOFAperture, preset.dof_aperture)?;
    set_float_param(
        &mut params_copy,
        Params::SizeMultiplier,
        preset.size_multiplier,
    )?;
    set_checkbox_param(
        &mut params_copy,
        Params::CompositeOnOrig,
        preset.composite_on_orig,
    )?;
    set_popup_param(&mut params_copy, Params::ApplyMode, preset.apply_mode)?;
    set_float_param(
        &mut params_copy,
        Params::RotationVar,
        preset.rotation_variation,
    )?;
    set_float_param(
        &mut params_copy,
        Params::OpacityVar,
        preset.opacity_variation,
    )?;
    set_checkbox_param(&mut params_copy, Params::ChildEnabled, preset.child_enabled)?;
    set_slider_param(&mut params_copy, Params::ChildCount, preset.child_count)?;
    set_float_param(
        &mut params_copy,
        Params::ChildInheritVel,
        preset.child_inherit_vel,
    )?;
    set_float_param(
        &mut params_copy,
        Params::ChildLifespan,
        preset.child_lifespan,
    )?;
    set_float_param(&mut params_copy, Params::ChildSpeed, preset.child_speed)?;
    set_float_param(&mut params_copy, Params::ChildSpread, preset.child_spread)?;
    set_float_param(
        &mut params_copy,
        Params::ChildSizeScale,
        preset.child_size_scale,
    )?;
    set_slider_param(&mut params_copy, Params::Seed, preset.seed)?;
    set_float_param(
        &mut params_copy,
        Params::PathSampleDensity,
        preset.path_sample_density,
    )?;
    set_slider_param(&mut params_copy, Params::GridResX, preset.grid_res_x)?;
    set_slider_param(&mut params_copy, Params::GridResY, preset.grid_res_y)?;
    set_slider_param(&mut params_copy, Params::GridResZ, preset.grid_res_z)?;

    update_shape_dependent_ui(&params_copy, None)?;
    Ok(())
}

// ---- Compute current time in seconds ----

fn current_time_sec(in_data: &ae::InData) -> f32 {
    let time = in_data.current_time() as f32;
    let scale = in_data.time_scale() as f32;
    if scale > 0.0 {
        time / scale
    } else {
        0.0
    }
}

fn time_step_sec(in_data: &ae::InData) -> f32 {
    let step = in_data.time_step() as f32;
    let scale = in_data.time_scale() as f32;
    if scale > 0.0 {
        step / scale
    } else {
        1.0 / 30.0
    }
}

fn checked_rgba_len(width: usize, height: usize) -> Result<usize, ae::Error> {
    let surface = RenderSurface::argb8(width, height).map_err(|err| {
        debug_error(format!(
            "Invalid render surface: {}x{} ({:?})",
            width, height, err
        ));
        ae::Error::OutOfMemory
    })?;
    if surface.len_bytes > MAX_RENDER_BYTES {
        debug_error(format!(
            "Refusing oversized render buffer: {}x{} ({} bytes)",
            width, height, surface.len_bytes
        ));
        return Err(ae::Error::OutOfMemory);
    }
    Ok(surface.len_bytes)
}

fn extract_runtime_engine_config(
    params: &ae::Parameters<Params>,
    project_state: &ParticleLabProjectState,
) -> Result<(ParticleEngineConfig, EngineConfigSource), ae::Error> {
    let host_values = collect_published_host_float_values(params, project_state)?;
    let graph_engine = if host_values.is_empty() {
        project_state.engine_config_override()
    } else {
        project_state.engine_config_override_with_host_values(&host_values)
    };
    if let Some((engine, source)) = graph_engine {
        Ok((engine.normalized_for_render(), source))
    } else {
        Ok((
            extract_engine_config(params)?.normalized_for_render(),
            EngineConfigSource::ClassicParams,
        ))
    }
}

fn collect_published_host_float_values(
    params: &ae::Parameters<Params>,
    project_state: &ParticleLabProjectState,
) -> Result<Vec<GraphPublishedValueOverride>, ae::Error> {
    let mut values = Vec::new();
    for binding in &project_state.graph.host_float_bindings {
        let Some(param_id) = published_host_float_param(binding.slot) else {
            continue;
        };
        let value = params.get(param_id)?.as_float_slider()?.value() as f32;
        values.push(GraphPublishedValueOverride {
            stable_id: binding.stable_id.clone(),
            value: GraphPublishedValue::Float(value),
        });
    }
    Ok(values)
}

fn estimate_render_margin_from_engine(engine: &ParticleEngineConfig) -> i32 {
    let emitter = &engine.emitter;
    let physics = &engine.physics;
    let render = &engine.render;
    let child_lifespan = if engine.child.enabled {
        engine.child.lifespan
    } else {
        0.0
    };

    let max_life = (emitter.lifespan * (1.0 + emitter.lifespan_variation.clamp(0.0, 1.0)))
        .max(child_lifespan)
        .max(0.01);
    let parent_peak_speed =
        emitter.initial_speed.max(0.0) * (1.0 + emitter.speed_variation.clamp(0.0, 1.0));
    let child_peak_speed = if engine.child.enabled {
        engine.child.initial_speed.max(0.0)
    } else {
        0.0
    };
    let peak_speed = parent_peak_speed.max(child_peak_speed);
    let accel = physics
        .gravity
        .length()
        .max(physics.wind.length())
        .max(physics.turbulence_strength.abs());
    // Air resistance reduces effective travel distance
    let drag_factor = if physics.air_resistance.abs() > 0.01 {
        (1.0 / physics.air_resistance.abs()).min(max_life)
    } else {
        max_life
    };
    let travel = peak_speed * drag_factor + 0.5 * accel * max_life * max_life;
    let radius = emitter.initial_size.max(0.0)
        * (1.0 + emitter.size_variation.clamp(0.0, 1.0))
        * render.size_multiplier.max(0.01)
        * (1.0 + render.motion_blur.clamp(0.0, 1.0) * 0.5);

    (travel + radius + 64.0).ceil().clamp(
        MIN_SMART_PRE_RENDER_MARGIN as f32,
        MAX_SMART_PRE_RENDER_MARGIN as f32,
    ) as i32
}

fn estimated_emitter_bounds_from_engine(engine: &ParticleEngineConfig, margin: i32) -> ae::Rect {
    let emitter = &engine.emitter;
    let size_x = emitter.size.x.abs();
    let size_y = emitter.size.y.abs();
    let max_emitter_half = MAX_SMART_PRE_RENDER_MARGIN as f32;
    let half_w = if matches!(
        emitter.emitter_type,
        EmitterType::Box | EmitterType::Sphere | EmitterType::Grid
    ) {
        (size_x * 0.5).min(max_emitter_half).ceil() as i32
    } else {
        0
    };
    let half_h = if matches!(
        emitter.emitter_type,
        EmitterType::Box | EmitterType::Sphere | EmitterType::Grid
    ) {
        (size_y * 0.5).min(max_emitter_half).ceil() as i32
    } else {
        0
    };
    let center_x = emitter.position.x.round() as i32;
    let center_y = emitter.position.y.round() as i32;

    ae::Rect {
        left: center_x.saturating_sub(half_w).saturating_sub(margin),
        top: center_y.saturating_sub(half_h).saturating_sub(margin),
        right: center_x.saturating_add(half_w).saturating_add(margin),
        bottom: center_y.saturating_add(half_h).saturating_add(margin),
    }
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
                                buf[si + ch * 4],
                                buf[si + ch * 4 + 1],
                                buf[si + ch * 4 + 2],
                                buf[si + ch * 4 + 3],
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
                    flat[dst_off..dst_off + row_len]
                        .copy_from_slice(&buf[src_off..src_off + row_len]);
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
            // 8-bit ARGB ↁE16-bit ARGB (AE 16bpc: 0-32768 range, where 32768 = 1.0)
            for y in 0..h {
                let src_row = y * w * 4;
                let dst_row = y * stride;
                for x in 0..w {
                    let si = src_row + x * 4;
                    let di = dst_row + x * 8; // 4 channels ÁE2 bytes
                    if si + 3 < flat.len() && di + 7 < buf.len() {
                        for ch in 0..4usize {
                            let v8 = flat[si + ch] as u16;
                            // AE 16bpc: max value is 32768 (not 32767). 255 ↁE32768.
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
            // 8-bit ARGB ↁE32-bit float ARGB (0.0 - 1.0)
            for y in 0..h {
                let src_row = y * w * 4;
                let dst_row = y * stride;
                for x in 0..w {
                    let si = src_row + x * 4;
                    let di = dst_row + x * 16; // 4 channels ÁE4 bytes
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
                    buf[dst_off..dst_off + row_len]
                        .copy_from_slice(&flat[src_off..src_off + row_len]);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderer::{BlendMode, ImageColorMode, ImageFitMode};

    macro_rules! params {
        ($($param:ident),+ $(,)?) => {
            &[$(Params::$param),+]
        };
    }

    const PARAM_ENUM_ABI_ORDER: &[Params] = params![
        PresetGroupStart,
        SavePreset,
        LoadPreset,
        DeletePreset,
        OpenPresetFolder,
        PresetGroupEnd,
        EmitterGroupStart,
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
        ChildEnabled,
        ChildCount,
        ChildInheritVel,
        ChildLifespan,
        ChildSpeed,
        ChildSpread,
        ChildSizeScale,
        ChildGroupEnd,
        SystemGroupStart,
        Seed,
        SystemGroupEnd,
        PluginMode,
        PlexusGroupStart,
        PlexusGroupEnd,
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
        PointGroupBStart,
        PointGroupBEnd,
        PointBEnabled,
        PointBSourceType,
        PointBSourceLayer,
        PointBGridResX,
        PointBGridResY,
        PointBGridSpacing,
        NoiseGroupStart,
        NoiseGroupEnd,
        NoiseEnabled,
        NoiseAmplitude,
        NoiseFrequency,
        NoiseSpeed,
        NoiseOctaves,
        NoiseAxisScale,
        LinesGroupStart,
        LinesGroupEnd,
        LinesEnabled,
        LinesMaxDistance,
        LinesWidth,
        LinesOpacityFalloff,
        LinesColor,
        MeshGroupStart,
        MeshGroupEnd,
        MeshEnabled,
        MeshMaxEdge,
        MeshOpacity,
        MeshColor,
        BeamsGroupStart,
        BeamsGroupEnd,
        BeamsEnabled,
        BeamsSourceGroup,
        BeamsMaxDistance,
        BeamsWidth,
        BeamsColor,
        PlexusRenderGroupStart,
        PlexusRenderGroupEnd,
        PlexusPointSize,
        PlexusPointColor,
        SpriteSourceLayer,
        PathSampleDensity,
        SpriteTimeSampling,
        SpriteFrameCount,
        GridResX,
        GridResY,
        GridResZ,
        EmitMode,
        RotationVar,
        ApplyMode,
        OpacityVar,
        PublishedControlsGroupStart,
        PublishedFloat1,
        PublishedFloat2,
        PublishedFloat3,
        PublishedFloat4,
        PublishedControlsGroupEnd,
        NodeGraphGroupStart,
        ExportNodeGraphState,
        ImportNodeGraphState,
        SeedNodeGraphFromParams,
        DisableNodeGraph,
        NodeGraphGroupEnd,
        NodeUiSidecarGroupStart,
        OpenNodeUiShell,
        NodeUiSidecarGroupEnd,
    ];

    const PARAM_SETUP_ABI_ORDER: &[Params] = params![
        PresetGroupStart,
        SavePreset,
        LoadPreset,
        DeletePreset,
        OpenPresetFolder,
        PresetGroupEnd,
        EmitterGroupStart,
        EmitterType,
        EmitMode,
        PositionPoint,
        PositionZ,
        ImageSourceLayer,
        RefreshImageCache,
        PathSampleDensity,
        EmitterSizeLinked,
        EmitterSizeX,
        EmitterSizeY,
        EmitterSizeZ,
        GridResX,
        GridResY,
        GridResZ,
        BirthRate,
        Lifespan,
        LifespanVar,
        EmitterGroupEnd,
        MotionGroupStart,
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
        RotationVar,
        MotionGroupEnd,
        PhysicsGroupStart,
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
        ColorMode,
        ColorStart,
        ColorEnd,
        OpacityVar,
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
        Shape,
        SpriteSourceLayer,
        SpriteTimeSampling,
        SpriteFrameCount,
        ImageProxyScale,
        EdgeSoftness,
        ImageColorMode,
        ImageFitMode,
        UseSourceAlpha,
        SourcePremultiplied,
        ImageAlphaClip,
        BlendModeParam,
        MotionBlur,
        DOFEnabled,
        DOFFocalDist,
        DOFAperture,
        SizeMultiplier,
        CompositeOnOrig,
        ApplyMode,
        RenderingGroupEnd,
        ChildGroupStart,
        ChildEnabled,
        ChildCount,
        ChildInheritVel,
        ChildLifespan,
        ChildSpeed,
        ChildSpread,
        ChildSizeScale,
        ChildGroupEnd,
        SystemGroupStart,
        Seed,
        SystemGroupEnd,
        PluginMode,
        PlexusGroupStart,
        PointGroupAStart,
        PointAEnabled,
        PointASourceType,
        PointASourceLayer,
        PointAGridResX,
        PointAGridResY,
        PointAGridResZ,
        PointAGridSpacing,
        PointAMaxPoints,
        PointGroupAEnd,
        PointGroupBStart,
        PointBEnabled,
        PointBSourceType,
        PointBSourceLayer,
        PointBGridResX,
        PointBGridResY,
        PointBGridSpacing,
        PointGroupBEnd,
        NoiseGroupStart,
        NoiseEnabled,
        NoiseAmplitude,
        NoiseFrequency,
        NoiseSpeed,
        NoiseOctaves,
        NoiseAxisScale,
        NoiseGroupEnd,
        LinesGroupStart,
        LinesEnabled,
        LinesMaxDistance,
        LinesWidth,
        LinesOpacityFalloff,
        LinesColor,
        LinesGroupEnd,
        MeshGroupStart,
        MeshEnabled,
        MeshMaxEdge,
        MeshOpacity,
        MeshColor,
        MeshGroupEnd,
        BeamsGroupStart,
        BeamsEnabled,
        BeamsSourceGroup,
        BeamsMaxDistance,
        BeamsWidth,
        BeamsColor,
        BeamsGroupEnd,
        PlexusRenderGroupStart,
        PlexusPointSize,
        PlexusPointColor,
        PlexusRenderGroupEnd,
        PlexusGroupEnd,
        PublishedControlsGroupStart,
        PublishedFloat1,
        PublishedFloat2,
        PublishedFloat3,
        PublishedFloat4,
        PublishedControlsGroupEnd,
        NodeGraphGroupStart,
        ExportNodeGraphState,
        ImportNodeGraphState,
        SeedNodeGraphFromParams,
        DisableNodeGraph,
        NodeGraphGroupEnd,
        NodeUiSidecarGroupStart,
        OpenNodeUiShell,
        NodeUiSidecarGroupEnd,
    ];

    fn assert_unique_params(label: &str, params: &[Params]) -> std::collections::HashSet<Params> {
        let mut seen = std::collections::HashSet::new();
        for &param in params {
            assert!(seen.insert(param), "duplicate {label} param: {param:?}");
        }
        seen
    }

    fn assert_close(label: &str, actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() < 0.0001,
            "{label}: {actual} != {expected}"
        );
    }

    fn assert_vec3_close(label: &str, actual: glam::Vec3, expected: glam::Vec3) {
        assert!(
            (actual - expected).length() < 0.0001,
            "{label}: {actual:?} != {expected:?}"
        );
    }

    fn assert_color_close(label: &str, actual: [f32; 4], expected: [f32; 4]) {
        for (index, (&actual, &expected)) in actual.iter().zip(expected.iter()).enumerate() {
            assert_close(&format!("{label}[{index}]"), actual, expected);
        }
    }

    fn assert_preset_engine_matches_classic_default(config: &ParticleEngineConfig) {
        let expected = ParticleEngineConfig::classic_default();

        assert!(matches!(config.emitter.emitter_type, EmitterType::Point));
        assert_vec3_close(
            "emitter.position",
            config.emitter.position,
            expected.emitter.position,
        );
        assert_vec3_close("emitter.size", config.emitter.size, expected.emitter.size);
        assert_close(
            "emitter.birth_rate",
            config.emitter.birth_rate,
            expected.emitter.birth_rate,
        );
        assert_close(
            "emitter.lifespan",
            config.emitter.lifespan,
            expected.emitter.lifespan,
        );
        assert_close(
            "emitter.lifespan_variation",
            config.emitter.lifespan_variation,
            expected.emitter.lifespan_variation,
        );
        assert_close(
            "emitter.initial_speed",
            config.emitter.initial_speed,
            expected.emitter.initial_speed,
        );
        assert_close(
            "emitter.speed_variation",
            config.emitter.speed_variation,
            expected.emitter.speed_variation,
        );
        assert_vec3_close(
            "emitter.initial_direction",
            config.emitter.initial_direction,
            expected.emitter.initial_direction,
        );
        assert_close(
            "emitter.spread",
            config.emitter.spread,
            expected.emitter.spread,
        );
        assert_close(
            "emitter.initial_size",
            config.emitter.initial_size,
            expected.emitter.initial_size,
        );
        assert_close(
            "emitter.size_variation",
            config.emitter.size_variation,
            expected.emitter.size_variation,
        );
        assert_close(
            "emitter.rotation_speed",
            config.emitter.rotation_speed,
            expected.emitter.rotation_speed,
        );
        assert_eq!(
            config.emitter.sprite_frame_count,
            expected.emitter.sprite_frame_count
        );
        assert_eq!(
            config.emitter.sprite_time_sampling,
            expected.emitter.sprite_time_sampling
        );
        assert_eq!(config.emitter.grid_res_x, expected.emitter.grid_res_x);
        assert_eq!(config.emitter.grid_res_y, expected.emitter.grid_res_y);
        assert_eq!(config.emitter.grid_res_z, expected.emitter.grid_res_z);
        assert_eq!(
            config.emitter.emit_all_at_start,
            expected.emitter.emit_all_at_start
        );

        assert_vec3_close(
            "physics.gravity",
            config.physics.gravity,
            expected.physics.gravity,
        );
        assert_vec3_close("physics.wind", config.physics.wind, expected.physics.wind);
        assert_close(
            "physics.air_resistance",
            config.physics.air_resistance,
            expected.physics.air_resistance,
        );
        assert_close(
            "physics.turbulence_strength",
            config.physics.turbulence_strength,
            expected.physics.turbulence_strength,
        );
        assert_close(
            "physics.turbulence_scale",
            config.physics.turbulence_scale,
            expected.physics.turbulence_scale,
        );
        assert_close(
            "physics.turbulence_speed",
            config.physics.turbulence_speed,
            expected.physics.turbulence_speed,
        );
        assert_eq!(
            config.physics.bounce_enabled,
            expected.physics.bounce_enabled
        );
        assert_close(
            "physics.bounce_damping",
            config.physics.bounce_damping,
            expected.physics.bounce_damping,
        );

        assert_color_close(
            "appearance.color_start",
            config.appearance.color_start,
            expected.appearance.color_start,
        );
        assert_color_close(
            "appearance.color_end",
            config.appearance.color_end,
            expected.appearance.color_end,
        );
        assert_color_close(
            "appearance.size_over_life",
            config.appearance.size_over_life,
            expected.appearance.size_over_life,
        );
        assert_color_close(
            "appearance.opacity_over_life",
            config.appearance.opacity_over_life,
            expected.appearance.opacity_over_life,
        );

        assert_eq!(config.child.enabled, expected.child.enabled);
        assert_eq!(config.child.count, expected.child.count);
        assert_close(
            "child.inherit_velocity",
            config.child.inherit_velocity,
            expected.child.inherit_velocity,
        );
        assert_close(
            "child.lifespan",
            config.child.lifespan,
            expected.child.lifespan,
        );
        assert_close(
            "child.initial_speed",
            config.child.initial_speed,
            expected.child.initial_speed,
        );
        assert_close("child.spread", config.child.spread, expected.child.spread);
        assert_close(
            "child.size_scale",
            config.child.size_scale,
            expected.child.size_scale,
        );

        assert!(matches!(config.render.shape, ParticleShape::Circle));
        assert!(matches!(config.render.blend_mode, BlendMode::Normal));
        assert!(matches!(config.render.apply_mode, ApplyMode::Normal));
        assert_close(
            "render.motion_blur",
            config.render.motion_blur,
            expected.render.motion_blur,
        );
        assert_close(
            "render.edge_softness",
            config.render.edge_softness,
            expected.render.edge_softness,
        );
        assert_eq!(config.render.dof_enabled, expected.render.dof_enabled);
        assert_close(
            "render.dof_focal_distance",
            config.render.dof_focal_distance,
            expected.render.dof_focal_distance,
        );
        assert_close(
            "render.dof_aperture",
            config.render.dof_aperture,
            expected.render.dof_aperture,
        );
        assert_eq!(
            config.render.composite_on_original,
            expected.render.composite_on_original
        );
        assert_close(
            "render.size_multiplier",
            config.render.size_multiplier,
            expected.render.size_multiplier,
        );
        assert!(matches!(
            config.render.time_sampling,
            TimeSamplingMode::CurrentTime
        ));
        assert!(matches!(
            config.render.image_color_mode,
            ImageColorMode::Tint
        ));
        assert!(matches!(
            config.render.image_fit_mode,
            ImageFitMode::Contain
        ));
        assert_eq!(
            config.render.image_sampling.use_source_alpha,
            expected.render.image_sampling.use_source_alpha
        );
        assert_eq!(
            config.render.image_sampling.source_premultiplied,
            expected.render.image_sampling.source_premultiplied
        );
        assert_close(
            "render.image_sampling.alpha_clip",
            config.render.image_sampling.alpha_clip,
            expected.render.image_sampling.alpha_clip,
        );
        assert_eq!(config.seed, expected.seed);
    }

    #[test]
    fn params_enum_order_is_append_only_abi() {
        assert_eq!(PARAM_ENUM_ABI_ORDER.len(), 166);
        assert_eq!(
            PARAM_ENUM_ABI_ORDER.len(),
            Params::NodeUiSidecarGroupEnd as usize + 1
        );

        for (index, &param) in PARAM_ENUM_ABI_ORDER.iter().enumerate() {
            assert_eq!(
                param as u16, index as u16,
                "Params::{param:?} moved from ABI slot {index}"
            );
        }
    }

    #[test]
    fn params_setup_order_covers_each_abi_slot_once() {
        let enum_set = assert_unique_params("enum ABI", PARAM_ENUM_ABI_ORDER);
        let setup_set = assert_unique_params("setup ABI", PARAM_SETUP_ABI_ORDER);

        assert_eq!(setup_set, enum_set);
        assert_eq!(PARAM_SETUP_ABI_ORDER.len(), PARAM_ENUM_ABI_ORDER.len());
    }

    #[test]
    fn load_preset_snapshot_migrates_apply_mode_from_composite_flag() {
        let snapshot = load_preset_snapshot(r#"{"composite_on_orig":false}"#).unwrap();
        assert_eq!(snapshot.version, PRESET_VERSION);
        assert_eq!(snapshot.apply_mode, 1);
        assert!(matches!(
            snapshot.to_engine_config().render.apply_mode,
            ApplyMode::OnTransparent
        ));

        let snapshot = load_preset_snapshot(r#"{"composite_on_orig":true}"#).unwrap();
        assert_eq!(snapshot.apply_mode, 2);
        assert!(matches!(
            snapshot.to_engine_config().render.apply_mode,
            ApplyMode::Normal
        ));
    }

    #[test]
    fn legacy_plexus_slots_stay_registered_for_project_compatibility() {
        assert_eq!(LEGACY_PLEXUS_PARAMS.len(), 53);
        assert_eq!(LEGACY_PLEXUS_PARAMS[0], Params::PluginMode);
        assert_eq!(
            LEGACY_PLEXUS_PARAMS[LEGACY_PLEXUS_PARAMS.len() - 1],
            Params::PlexusGroupEnd
        );
        let legacy_start = PARAM_SETUP_ABI_ORDER
            .windows(LEGACY_PLEXUS_PARAMS.len())
            .position(|window| window == LEGACY_PLEXUS_PARAMS)
            .expect("legacy Plexus setup slots must remain contiguous");
        assert_eq!(
            &PARAM_SETUP_ABI_ORDER[legacy_start..legacy_start + LEGACY_PLEXUS_PARAMS.len()],
            LEGACY_PLEXUS_PARAMS
        );

        let mut seen = std::collections::HashSet::new();
        for &param in LEGACY_PLEXUS_PARAMS {
            assert!(seen.insert(param), "duplicate legacy param: {:?}", param);
        }
    }

    #[test]
    fn published_host_float_slots_stay_before_node_graph_and_sidecar_tail_params() {
        let published_params = params![
            PublishedControlsGroupStart,
            PublishedFloat1,
            PublishedFloat2,
            PublishedFloat3,
            PublishedFloat4,
            PublishedControlsGroupEnd,
        ];

        let node_graph_params = params![
            NodeGraphGroupStart,
            ExportNodeGraphState,
            ImportNodeGraphState,
            SeedNodeGraphFromParams,
            DisableNodeGraph,
            NodeGraphGroupEnd,
        ];
        let node_ui_sidecar_params = params![
            NodeUiSidecarGroupStart,
            OpenNodeUiShell,
            NodeUiSidecarGroupEnd,
        ];
        let published_start = PARAM_SETUP_ABI_ORDER
            .windows(published_params.len())
            .position(|window| window == published_params)
            .expect("published host params must remain contiguous");
        let node_graph_start = PARAM_SETUP_ABI_ORDER
            .windows(node_graph_params.len())
            .position(|window| window == node_graph_params)
            .expect("node graph tool params must remain contiguous");
        let node_ui_sidecar_start = PARAM_SETUP_ABI_ORDER
            .windows(node_ui_sidecar_params.len())
            .position(|window| window == node_ui_sidecar_params)
            .expect("node UI sidecar params must remain contiguous");

        assert_eq!(
            &PARAM_SETUP_ABI_ORDER[published_start..published_start + published_params.len()],
            published_params
        );
        assert_eq!(
            &PARAM_ENUM_ABI_ORDER[published_start..published_start + published_params.len()],
            published_params
        );
        assert_eq!(
            &PARAM_SETUP_ABI_ORDER[node_graph_start..node_graph_start + node_graph_params.len()],
            node_graph_params
        );
        assert_eq!(
            &PARAM_ENUM_ABI_ORDER[node_graph_start..node_graph_start + node_graph_params.len()],
            node_graph_params
        );
        assert_eq!(
            &PARAM_SETUP_ABI_ORDER[PARAM_SETUP_ABI_ORDER.len() - node_ui_sidecar_params.len()..],
            node_ui_sidecar_params
        );
        assert_eq!(
            &PARAM_ENUM_ABI_ORDER[PARAM_ENUM_ABI_ORDER.len() - node_ui_sidecar_params.len()..],
            node_ui_sidecar_params
        );
        assert_eq!(
            node_graph_start,
            published_start + published_params.len(),
            "node graph tools must be appended after published host controls"
        );
        assert_eq!(
            node_ui_sidecar_start,
            node_graph_start + node_graph_params.len(),
            "node UI sidecar must be appended after node graph tools"
        );
        assert_eq!(PUBLISHED_HOST_FLOAT_SLOT_COUNT, 4);
        assert_eq!(published_host_float_param(0), None);
        assert_eq!(published_host_float_param(1), Some(Params::PublishedFloat1));
        assert_eq!(published_host_float_param(4), Some(Params::PublishedFloat4));
        assert_eq!(published_host_float_param(5), None);
    }

    #[test]
    fn node_ui_sidecar_assets_are_embedded_for_host_launch() {
        let assets = node_ui_shell_assets();
        assert_eq!(assets.len(), 4);
        assert!(assets
            .iter()
            .any(|(filename, contents)| *filename == "index.html"
                && contents.contains("Node UI Shell")
                && contents.contains("startup-payload.js")
                && contents.contains("app.js")));
        assert!(assets.iter().any(|(filename, contents)| {
            *filename == "app.js"
                && contents.contains("createParticleDemoPayload")
                && contents.contains("createLatticeDemoPayload")
                && contents.contains("PARTICLELAB_NODE_UI_BOOTSTRAP")
                && contents.contains("canSaveCurrentPayload")
        }));
        assert!(assets.iter().any(|(filename, contents)| {
            *filename == "styles.css" && contents.contains(".graph-canvas")
        }));
        assert!(assets.iter().any(|(filename, contents)| {
            *filename == "startup-payload.js"
                && contents.contains("PARTICLELAB_NODE_UI_BOOTSTRAP")
                && contents.contains("PARTICLELAB_NODE_UI_BOOTSTRAP_SOURCE")
        }));
    }

    #[test]
    fn node_ui_sidecar_startup_payload_carries_current_project_state() {
        let mut state = ParticleLabProjectState::default();
        state
            .commit_node_graph_document(graph::GraphDocument::from_engine_config(
                &ParticleEngineConfig::classic_default(),
            ))
            .unwrap();

        let startup = node_ui_shell_startup_payload_js(&state).unwrap();
        assert!(startup.starts_with("window.PARTICLELAB_NODE_UI_BOOTSTRAP = {"));
        assert!(startup.contains("\"product_id\": \"particlelab\""));
        assert!(startup.contains("\"enabled\": true"));
        assert!(startup.contains("\"document\""));
        assert!(startup.contains("AE sidecar startup payload"));
    }

    #[test]
    fn published_host_float_display_names_follow_graph_bindings() {
        let mut document =
            graph::GraphDocument::from_engine_config(&ParticleEngineConfig::classic_default());
        document.published_params.push(graph::GraphPublishedParam {
            stable_id: "birth_rate".to_string(),
            label: "Birth Rate".to_string(),
            target: graph::GraphSocket {
                node: graph::NodeId(2),
                socket: "birth_rate".to_string(),
            },
            value_type: graph::GraphPublishedValueType::Float,
            default_value: GraphPublishedValue::Float(180.0),
        });
        let mut state = ParticleLabProjectState::default();
        state
            .commit_node_ui_graph_state_snapshot(NodeUiGraphStateSnapshot {
                version: project_state::NODE_UI_GRAPH_STATE_VERSION,
                enabled: true,
                document: Some(document),
                published_values: Vec::new(),
                host_float_bindings: vec![project_state::GraphPublishedHostFloatBinding {
                    stable_id: "birth_rate".to_string(),
                    slot: 1,
                }],
            })
            .unwrap();

        assert_eq!(
            published_host_float_display_name(1, Some(&state)),
            "F1: Birth Rate"
        );
        assert_eq!(
            published_host_float_binding_label(Some(&state), 1),
            Some("Birth Rate")
        );
        assert_eq!(
            published_host_float_display_name(2, Some(&state)),
            "Published Float 2"
        );
        assert_eq!(published_host_float_binding_label(Some(&state), 2), None);
        assert_eq!(
            truncate_ae_param_name("12345678901234567890123456789012345"),
            "1234567890123456789012345678901"
        );
    }

    #[test]
    fn load_preset_snapshot_ignores_retired_plexus_fields() {
        let snapshot = load_preset_snapshot(
            r#"{
                "plugin_mode": 2,
                "point_a_enabled": false,
                "lines_max_distance": 42.0
            }"#,
        )
        .unwrap();

        assert_eq!(snapshot.version, PRESET_VERSION);
        assert_eq!(
            snapshot.emitter_type,
            PresetSnapshot::default().emitter_type
        );
    }

    #[test]
    fn preset_default_compiles_to_classic_engine_default() {
        let config = PresetSnapshot::default().to_engine_config();
        assert_preset_engine_matches_classic_default(&config);
    }

    #[test]
    fn missing_preset_fields_compile_to_classic_engine_default() {
        let snapshot = load_preset_snapshot("{}").unwrap();
        let config = snapshot.to_engine_config();
        assert_preset_engine_matches_classic_default(&config);
    }

    #[test]
    fn preset_single_color_mode_uses_start_color_for_engine_end_color() {
        let mut snapshot = PresetSnapshot::default();
        snapshot.color_mode = 1;
        snapshot.color_start = PresetColor {
            alpha: 255,
            red: 128,
            green: 64,
            blue: 32,
        };
        snapshot.color_end = PresetColor {
            alpha: 255,
            red: 1,
            green: 2,
            blue: 3,
        };
        snapshot.opacity_end = 25.0;

        let config = snapshot.to_engine_config();
        assert_color_close(
            "single color end",
            config.appearance.color_end,
            [128.0 / 255.0, 64.0 / 255.0, 32.0 / 255.0, 0.25],
        );
    }

    #[test]
    fn composite_apply_normal_preserves_argb_order() {
        let original = [128u8, 64, 0, 0];
        let particles = [128u8, 0, 64, 0];
        let mut output = [0u8; 4];
        renderer::composite_apply(
            &original,
            1,
            1,
            0,
            0,
            &particles,
            1,
            1,
            0,
            0,
            &mut output,
            1,
            1,
            0,
            0,
            ApplyMode::Normal,
        );

        assert_eq!(output[0], 192);
        assert_eq!(output[1], 32);
        assert_eq!(output[2], 64);
        assert_eq!(output[3], 0);
    }

    #[test]
    fn renderer_final_composite_normal_preserves_argb_order() {
        let original = [128u8, 64, 0, 0];
        let mut particles = vec![128u8, 0, 64, 0];

        renderer::apply_final_composite(
            &original,
            1,
            1,
            0,
            0,
            &mut particles,
            1,
            1,
            0,
            0,
            ApplyMode::Normal,
        );

        assert_eq!(particles, vec![192, 32, 64, 0]);
    }

    #[test]
    fn renderer_blit_argb_into_respects_origins() {
        let source = [1u8, 10, 11, 12, 2, 20, 21, 22, 3, 30, 31, 32, 4, 40, 41, 42];
        let mut output = vec![0u8; 3 * 3 * 4];

        renderer::blit_argb_into(&source, 2, 2, 10, 10, &mut output, 3, 3, 9, 9);

        let center = (1 * 3 + 1) * 4;
        assert_eq!(&output[center..center + 4], &[1, 10, 11, 12]);
        let bottom_right = (2 * 3 + 2) * 4;
        assert_eq!(&output[bottom_right..bottom_right + 4], &[4, 40, 41, 42]);
    }

    #[test]
    fn sprite_timing_params_do_not_force_source_cache_invalidations() {
        assert!(!should_invalidate_source_cache(Params::SpriteTimeSampling));
        assert!(!should_invalidate_source_cache(Params::SpriteFrameCount));
        assert!(should_invalidate_source_cache(Params::ImageSourceLayer));
        assert!(should_invalidate_source_cache(Params::ImageProxyScale));
    }

    fn make_test_sprite(w: usize, h: usize, pixels: Vec<u8>) -> SpriteImage {
        finish_sprite(w, h, pixels)
    }

    fn make_straight_test_sprite(w: usize, h: usize, pixels: Vec<u8>) -> SpriteImage {
        finish_sprite_with_alpha_mode(w, h, pixels, false)
    }

    #[test]
    fn mip_chain_opaque_preserves_colors() {
        let w = 8;
        let h = 8;
        let mut pixels = vec![0u8; w * h * 4];
        for y in 0..h {
            for x in 0..w {
                let i = (y * w + x) * 4;
                pixels[i] = 255;
                pixels[i + 1] = 128;
                pixels[i + 2] = 64;
                pixels[i + 3] = 32;
            }
        }
        let sprite = make_test_sprite(w, h, pixels);
        assert_eq!(sprite.width, 8);
        assert_eq!(sprite.height, 8);
        assert!(sprite.mips.len() >= 2);
        let mip0 = &sprite.mips[0];
        assert_eq!(mip0.width, 4);
        assert_eq!(mip0.height, 4);
        assert_eq!(mip0.pixels[0], 255);
        assert_eq!(mip0.pixels[1], 128);
        assert_eq!(mip0.pixels[2], 64);
        assert_eq!(mip0.pixels[3], 32);
    }

    #[test]
    fn mip_chain_fully_transparent_produces_black_alpha_zero() {
        let w = 4;
        let h = 4;
        let mut pixels = vec![0u8; w * h * 4];
        for i in (0..pixels.len()).step_by(4) {
            pixels[i] = 0;
            pixels[i + 1] = 255;
            pixels[i + 2] = 128;
            pixels[i + 3] = 64;
        }
        let sprite = make_test_sprite(w, h, pixels);
        assert!(sprite.mips.len() >= 1);
        let mip0 = &sprite.mips[0];
        assert_eq!(mip0.width, 2);
        assert_eq!(mip0.pixels[0], 0);
        assert_eq!(mip0.pixels[1], 0);
        assert_eq!(mip0.pixels[2], 0);
        assert_eq!(mip0.pixels[3], 0);
    }

    #[test]
    fn mip_chain_transparent_pixel_keeps_premultiplied_average() {
        let w = 4;
        let h = 4;
        let mut pixels = vec![0u8; w * h * 4];

        for j in 0..h {
            for i in 0..w {
                let off = (j * w + i) * 4;
                if i == 2 && j == 2 {
                    pixels[off] = 0;
                    pixels[off + 1] = 0;
                    pixels[off + 2] = 255;
                    pixels[off + 3] = 0;
                } else {
                    pixels[off] = 255;
                    pixels[off + 1] = 128;
                    pixels[off + 2] = 128;
                    pixels[off + 3] = 128;
                }
            }
        }

        let sprite = make_test_sprite(w, h, pixels);
        assert!(sprite.mips.len() >= 1);
        let mip0 = &sprite.mips[0];
        assert_eq!(mip0.width, 2);
        assert_eq!(mip0.height, 2);

        let idx = (1 * mip0.width + 1) * 4;
        assert_eq!(mip0.pixels[idx], 191);
        assert_eq!(mip0.pixels[idx + 1], 96);
        assert_eq!(mip0.pixels[idx + 2], 96);
        assert_eq!(mip0.pixels[idx + 3], 96);
    }

    #[test]
    fn mip_chain_partial_alpha_uses_premultiplied_area_average() {
        // Premultiplied ARGB should be area-averaged without converting to straight RGB.
        let w = 4;
        let h = 4;
        let mut pixels = vec![0u8; w * h * 4];
        for j in 0..h {
            for i in 0..w {
                let off = (j * w + i) * 4;
                if i < 2 && j < 2 {
                    pixels[off] = 255;
                    pixels[off + 1] = 0;
                    pixels[off + 2] = 0;
                    pixels[off + 3] = 255;
                } else if i >= 2 && j < 2 {
                    pixels[off] = 1;
                    pixels[off + 1] = 1;
                    pixels[off + 2] = 1;
                    pixels[off + 3] = 1;
                } else if i < 2 && j >= 2 {
                    pixels[off] = 255;
                    pixels[off + 1] = 255;
                    pixels[off + 2] = 0;
                    pixels[off + 3] = 0;
                } else {
                    pixels[off] = 2;
                    pixels[off + 1] = 2;
                    pixels[off + 2] = 2;
                    pixels[off + 3] = 2;
                }
            }
        }

        let sprite = make_test_sprite(w, h, pixels);
        assert!(sprite.mips.len() >= 1);
        let mip0 = &sprite.mips[0];
        assert_eq!(mip0.width, 2);
        assert_eq!(mip0.height, 2);

        // Quadrant at (0,0): opaque blue.
        let idx00 = 0;
        assert_eq!(mip0.pixels[idx00], 255);
        assert_eq!(mip0.pixels[idx00 + 1], 0);
        assert_eq!(mip0.pixels[idx00 + 2], 0);
        assert_eq!(mip0.pixels[idx00 + 3], 255);

        // Quadrant at (1,0): near-transparent white stays premultiplied.
        let idx10 = 4;
        assert_eq!(mip0.pixels[idx10], 1);
        assert_eq!(mip0.pixels[idx10 + 1], 1);
        assert_eq!(mip0.pixels[idx10 + 2], 1);
        assert_eq!(mip0.pixels[idx10 + 3], 1);

        // Quadrant at (0,1): opaque red.
        let idx01 = 8;
        assert_eq!(mip0.pixels[idx01], 255);
        assert_eq!(mip0.pixels[idx01 + 1], 255);
        assert_eq!(mip0.pixels[idx01 + 2], 0);
        assert_eq!(mip0.pixels[idx01 + 3], 0);
    }

    #[test]
    fn straight_alpha_mip_preserves_color_without_transparent_bleed() {
        let w = 4;
        let h = 4;
        let mut pixels = vec![0u8; w * h * 4];
        for j in 0..h {
            for i in 0..w {
                let off = (j * w + i) * 4;
                if i < 2 && j < 2 {
                    pixels[off] = 255;
                    pixels[off + 1] = 0;
                    pixels[off + 2] = 128;
                    pixels[off + 3] = 255;
                } else {
                    pixels[off] = 0;
                    pixels[off + 1] = 255;
                    pixels[off + 2] = 0;
                    pixels[off + 3] = 0;
                }
            }
        }

        let sprite = make_straight_test_sprite(w, h, pixels);
        let mip0 = &sprite.mips[0];
        assert_eq!(mip0.pixels[0], 255);
        assert_eq!(mip0.pixels[1], 0);
        assert_eq!(mip0.pixels[2], 128);
        assert_eq!(mip0.pixels[3], 255);
        assert_eq!(mip0.pixels[4], 0);
        assert_eq!(mip0.pixels[5], 0);
        assert_eq!(mip0.pixels[6], 0);
        assert_eq!(mip0.pixels[7], 0);
    }

    #[test]
    fn cache_invalidation_sets_populated_flags_false() {
        let k = ImageCacheKey {
            instance_id: 1,
            time: 0,
            time_step: 1,
            time_scale: 30,
            proxy_divisor: 1,
            source_width: 100,
            source_height: 100,
            source_origin_x: 0,
            source_origin_y: 0,
            source_signature: 0,
            generation: 1,
        };
        {
            let mut cache = image_cache().write().unwrap();
            cache.insert(k, Arc::new(vec![]));
            IMAGE_CACHE_POPULATED.store(true, Ordering::Release);
        }
        assert!(image_cache_has_entries());
        invalidate_image_cache();
        assert!(!image_cache_has_entries());
        assert!(!emitter_cache_has_entries());
        {
            let cache = image_cache().read().unwrap();
            assert!(cache.is_empty());
        }
    }

    #[test]
    fn finish_sprite_empty_input_produces_small_valid_sprite() {
        let sprite = finish_sprite(0, 0, vec![]);
        assert_eq!(sprite.width, 0);
        assert_eq!(sprite.height, 0);
        assert!(sprite.pixels.is_empty());
        assert!(sprite.mips.is_empty());
    }

    #[test]
    fn generate_mip_chain_1x1_no_mips() {
        let pixels = vec![255u8, 128, 64, 32];
        let mips = generate_mip_chain(1, 1, &pixels);
        assert!(mips.is_empty());
    }
}

fn image_cache() -> &'static RwLock<SpriteCacheMap> {
    IMAGE_CACHE.get_or_init(|| RwLock::new(SpriteCacheMap::default()))
}

fn emitter_cache() -> &'static RwLock<EmitterPointCacheMap> {
    EMITTER_CACHE.get_or_init(|| RwLock::new(EmitterPointCacheMap::default()))
}

fn image_cache_has_entries() -> bool {
    IMAGE_CACHE_POPULATED.load(Ordering::Acquire)
}

fn emitter_cache_has_entries() -> bool {
    EMITTER_CACHE_POPULATED.load(Ordering::Acquire)
}

fn invalidate_image_cache() {
    IMAGE_CACHE_GENERATION.fetch_add(1, Ordering::Relaxed);
    IMAGE_CACHE_POPULATED.store(false, Ordering::Release);
    EMITTER_CACHE_POPULATED.store(false, Ordering::Release);
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

fn update_shape_dependent_ui(
    params: &ae::Parameters<Params>,
    project_state: Option<&ParticleLabProjectState>,
) -> Result<(), ae::Error> {
    let is_image_shape = params.get(Params::Shape)?.as_popup()?.value() == 6;
    let emitter_type_val = params.get(Params::EmitterType)?.as_popup()?.value();
    let is_layer_alpha_emitter = emitter_type_val == 5;
    let is_path_emitter = emitter_type_val == 6;
    let uses_emitter_volume = matches!(emitter_type_val, 2 | 3 | 4);
    let emitter_source_enabled = is_layer_alpha_emitter || is_path_emitter;
    let any_image_in_use = emitter_source_enabled || is_image_shape;
    let use_source_alpha = params.get(Params::UseSourceAlpha)?.as_checkbox()?.value();
    let size_linked = params
        .get(Params::EmitterSizeLinked)?
        .as_checkbox()?
        .value();
    let mut params_copy = params.cloned();

    // Emitter source layer control (Layer Alpha or Path emitter)
    {
        let mut param = params_copy.get_mut(Params::ImageSourceLayer)?;
        param.set_ui_flag(ae::ParamUIFlags::DISABLED, !emitter_source_enabled);
        param.update_param_ui()?;
    }

    // Refresh Image Cache + Image Proxy  Eactive if either emitter source OR
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

    // Grid resolution (Grid emitter only)
    let is_grid_emitter = emitter_type_val == 4;
    for param_id in [Params::GridResX, Params::GridResY, Params::GridResZ] {
        let mut param = params_copy.get_mut(param_id)?;
        param.set_ui_flag(ae::ParamUIFlags::DISABLED, !is_grid_emitter);
        param.update_param_ui()?;
    }

    // Sprite source layer + time sampling + frame count (Image shape only)
    for param_id in [
        Params::SpriteSourceLayer,
        Params::SpriteTimeSampling,
        Params::SpriteFrameCount,
    ] {
        let mut param = params_copy.get_mut(param_id)?;
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

    for param_id in [Params::SourcePremultiplied, Params::ImageAlphaClip] {
        let mut param = params_copy.get_mut(param_id)?;
        param.set_ui_flag(
            ae::ParamUIFlags::DISABLED,
            !is_image_shape || !use_source_alpha,
        );
        param.update_param_ui()?;
    }

    // BirthRate: disabled when "All at Start" + Grid (Grid uses GridRes for count)
    // For "All at Start" + non-Grid, BirthRate is reused as total particle count
    let emit_all = params.get(Params::EmitMode)?.as_popup()?.value() == 2;
    {
        let mut param = params_copy.get_mut(Params::BirthRate)?;
        param.set_ui_flag(ae::ParamUIFlags::DISABLED, emit_all && is_grid_emitter);
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
    for param_id in [
        Params::OpacityStart,
        Params::OpacityMidA,
        Params::OpacityMidB,
        Params::OpacityEnd,
    ] {
        let mut param = params_copy.get_mut(param_id)?;
        param.set_ui_flag(ae::ParamUIFlags::DISABLED, !opacity_custom);
        param.update_param_ui()?;
    }

    let size_custom = params.get(Params::SizeCurvePreset)?.as_popup()?.value() == 1;
    for param_id in [
        Params::SizeLifeStart,
        Params::SizeLifeMidA,
        Params::SizeLifeMidB,
        Params::SizeLifeEnd,
    ] {
        let mut param = params_copy.get_mut(param_id)?;
        param.set_ui_flag(ae::ParamUIFlags::DISABLED, !size_custom);
        param.update_param_ui()?;
    }

    for &param_id in LEGACY_PLEXUS_PARAMS {
        let mut param = params_copy.get_mut(param_id)?;
        param.set_ui_flag(ae::ParamUIFlags::INVISIBLE, true);
        param.set_ui_flag(ae::ParamUIFlags::NO_ECW_UI, true);
        param.update_param_ui()?;
    }

    update_published_host_float_ui(&mut params_copy, project_state)?;

    Ok(())
}

fn update_published_host_float_ui(
    params: &mut ae::Parameters<Params>,
    project_state: Option<&ParticleLabProjectState>,
) -> Result<(), ae::Error> {
    for slot in 1..=PUBLISHED_HOST_FLOAT_SLOT_COUNT {
        let Some(param_id) = published_host_float_param(slot) else {
            continue;
        };
        let display_name = published_host_float_display_name(slot, project_state);
        let enabled = published_host_float_binding_label(project_state, slot).is_some();
        let mut param = params.get_mut(param_id)?;
        param.set_name(&display_name)?;
        param.set_ui_flag(ae::ParamUIFlags::DISABLED, !enabled);
        param.update_param_ui()?;
    }
    Ok(())
}

fn published_host_float_binding_label(
    project_state: Option<&ParticleLabProjectState>,
    slot: u8,
) -> Option<&str> {
    let project_state = project_state?;
    let binding = project_state
        .graph
        .host_float_bindings
        .iter()
        .find(|binding| binding.slot == slot)?;
    let document = project_state.graph.document.as_ref()?;
    let published = document
        .published_params
        .iter()
        .find(|published| published.stable_id == binding.stable_id)?;
    let label = published.label.trim();
    if label.is_empty() {
        Some(binding.stable_id.as_str())
    } else {
        Some(label)
    }
}

fn published_host_float_display_name(
    slot: u8,
    project_state: Option<&ParticleLabProjectState>,
) -> String {
    if let Some(label) = published_host_float_binding_label(project_state, slot) {
        truncate_ae_param_name(&format!("F{}: {}", slot, label))
    } else {
        format!("Published Float {}", slot)
    }
}

fn truncate_ae_param_name(name: &str) -> String {
    name.chars().take(AE_PARAM_DYNAMIC_NAME_MAX_CHARS).collect()
}

fn sync_box_size_axes(params: &ae::Parameters<Params>, changed: Params) -> Result<(), ae::Error> {
    let emitter_type = params.get(Params::EmitterType)?.as_popup()?.value();
    let size_linked = params
        .get(Params::EmitterSizeLinked)?
        .as_checkbox()?
        .value();
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
    for param_id in [
        Params::EmitterSizeX,
        Params::EmitterSizeY,
        Params::EmitterSizeZ,
    ] {
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
        2 => [100.0, 100.0, 100.0, 100.0], // Constant
        3 => [100.0, 80.0, 40.0, 0.0],     // Fade Out (linear-ish)
        4 => [0.0, 100.0, 100.0, 0.0],     // Fade In-Out
        5 => [100.0, 95.0, 70.0, 0.0],     // Ease Out (slow start, fast end)
        6 => [100.0, 30.0, 5.0, 0.0],      // Quick Fade
        _ => return Ok(()),                // Custom: don't change
    };
    let param_ids = [
        Params::OpacityStart,
        Params::OpacityMidA,
        Params::OpacityMidB,
        Params::OpacityEnd,
    ];
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
        2 => [1.0, 1.0, 1.0, 1.0], // Constant
        3 => [1.0, 0.8, 0.4, 0.0], // Shrink
        4 => [0.0, 1.0, 1.0, 0.0], // Grow-Shrink
        5 => [0.0, 0.4, 0.8, 1.0], // Grow
        6 => [1.2, 0.9, 0.4, 0.0], // Pop-Shrink (starts big)
        _ => return Ok(()),        // Custom: don't change
    };
    let param_ids = [
        Params::SizeLifeStart,
        Params::SizeLifeMidA,
        Params::SizeLifeMidB,
        Params::SizeLifeEnd,
    ];
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
    emitter: &EmitterConfig,
) -> Result<Option<Arc<Vec<glam::Vec3>>>, ae::Error> {
    if !matches!(emitter.emitter_type, EmitterType::LayerAlpha) {
        return Ok(None);
    }

    let checked_out = params.checkout(Params::ImageSourceLayer)?;
    let Some(source_layer) = checked_out.as_layer()?.value() else {
        return Ok(None);
    };

    let mut key = source_cache_key(in_data, proxy_divisor(params)?, &source_layer);
    // Pin time to 0 — emitter point sampling is time-independent; source_signature
    // already captures content changes.
    key.time = 0;

    if emitter_cache_has_entries() {
        if let Ok(cache) = emitter_cache().read() {
            if let Some(points) = cache.get(&key) {
                return Ok(Some(points.clone()));
            }
        }
    }

    let points = Arc::new(build_emitter_points(&source_layer, key.proxy_divisor));
    if let Ok(mut cache) = emitter_cache().write() {
        cache.insert(key, points.clone());
        EMITTER_CACHE_POPULATED.store(true, Ordering::Release);
    }
    Ok(Some(points))
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
    emitter: &EmitterConfig,
) -> Result<Option<Arc<Vec<glam::Vec3>>>, ae::Error> {
    if !matches!(emitter.emitter_type, EmitterType::Path) {
        return Ok(None);
    }

    let density = params
        .get(Params::PathSampleDensity)?
        .as_float_slider()?
        .value() as f32;
    let samples_per_seg = (density as usize).max(1);

    let path_query = match ae::pf::suites::PathQuery::new() {
        Ok(s) => s,
        Err(_) => return Ok(None),
    };

    let effect_ref = in_data.effect_ref();
    let num_paths = path_query.num_paths(&effect_ref)?;
    if num_paths <= 0 {
        return Ok(None);
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

    Ok(Some(Arc::new(points)))
}

fn cubic_bezier(
    p0: glam::Vec2,
    p1: glam::Vec2,
    p2: glam::Vec2,
    p3: glam::Vec2,
    t: f32,
) -> glam::Vec2 {
    let u = 1.0 - t;
    let uu = u * u;
    let tt = t * t;
    p0 * (uu * u) + p1 * (3.0 * uu * t) + p2 * (3.0 * u * tt) + p3 * (tt * t)
}

fn populate_image_sprite(
    params: &ae::Parameters<Params>,
    in_data: &ae::InData,
    render_cfg: &RenderConfig,
) -> Result<Vec<SpriteImage>, ae::Error> {
    if !matches!(render_cfg.shape, ParticleShape::Image) {
        return Ok(Vec::new());
    }

    let frame_count = params
        .get(Params::SpriteFrameCount)?
        .as_slider()?
        .value()
        .max(1) as u16;
    let is_multi_frame =
        frame_count > 1 && render_cfg.time_sampling != TimeSamplingMode::CurrentTime;
    let pdiv = proxy_divisor(params)?;
    let checked_out = params.checkout(Params::SpriteSourceLayer)?;
    let Some(source_layer) = checked_out.as_layer()?.value() else {
        return Ok(Vec::new());
    };
    let mut key = source_cache_key(in_data, pdiv, &source_layer);
    // Pin time to 0 for cache stability — source layer content is captured by
    // source_signature and doesn't change with composition time.
    key.time = 0;
    if render_cfg.image_sampling.source_premultiplied {
        key.source_signature ^= 0xA8D7_EF31_5B2C_49D0;
    }
    if is_multi_frame {
        key.source_signature ^= (frame_count as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    }

    if image_cache_has_entries() {
        if let Ok(cache) = image_cache().read() {
            if let Some(sprites) = cache.get(&key) {
                return Ok((**sprites).clone());
            }
        }
    }

    if !is_multi_frame {
        let sprite = build_sprite_image(
            &source_layer,
            pdiv,
            render_cfg.image_sampling.source_premultiplied,
        );
        let sprites = Arc::new(vec![sprite]);
        if let Ok(mut cache) = image_cache().write() {
            cache.insert(key, sprites.clone());
            IMAGE_CACHE_POPULATED.store(true, Ordering::Release);
        }
        return Ok((*sprites).clone());
    }

    // Multi-frame: checkout source layer at different times
    let time_step = in_data.time_step();
    let time_scale = in_data.time_scale();
    drop(checked_out);

    let mut sprites = Vec::with_capacity(frame_count as usize);
    for i in 0..frame_count {
        check_render_abort(in_data)?;
        let frame_time = (i as i32) * time_step;
        match params.checkout_at(
            Params::SpriteSourceLayer,
            Some(frame_time),
            Some(time_step),
            Some(time_scale),
        ) {
            Ok(checked) => {
                if let Ok(layer_ref) = checked.as_layer() {
                    if let Some(layer) = layer_ref.value() {
                        sprites.push(build_sprite_image(
                            &layer,
                            pdiv,
                            render_cfg.image_sampling.source_premultiplied,
                        ));
                        check_render_abort(in_data)?;
                        continue;
                    }
                }
                sprites.push(finish_sprite(1, 1, vec![255, 255, 255, 255]));
                check_render_abort(in_data)?;
            }
            Err(_) => {
                sprites.push(finish_sprite(1, 1, vec![255, 255, 255, 255]));
                check_render_abort(in_data)?;
            }
        }
    }

    let sprites = Arc::new(sprites);
    if let Ok(mut cache) = image_cache().write() {
        cache.insert(key, sprites.clone());
        IMAGE_CACHE_POPULATED.store(true, Ordering::Release);
    }
    Ok((*sprites).clone())
}

fn collect_particle_runtime_inputs(
    params: &ae::Parameters<Params>,
    in_data: &ae::InData,
    engine: &ParticleEngineConfig,
    output_width: usize,
    output_height: usize,
    origin_x: i32,
    origin_y: i32,
) -> Result<ParticleRuntimeInputs, ae::Error> {
    let mut runtime = ParticleRuntimeInputs::new(
        output_width,
        output_height,
        origin_x,
        origin_y,
        current_time_sec(in_data),
        time_step_sec(in_data),
    )
    .map_err(|err| {
        debug_error(format!(
            "Invalid particle runtime surface: {}x{} ({:?})",
            output_width, output_height, err
        ));
        ae::Error::OutOfMemory
    })?;

    runtime.source_points = populate_layer_alpha_emitter(params, in_data, &engine.emitter)?;
    if runtime.source_points.is_none() {
        runtime.source_points = populate_path_emitter(params, in_data, &engine.emitter)?;
    }
    runtime.sprite_images = populate_image_sprite(params, in_data, &engine.render)?;
    runtime.camera_projection = try_get_camera_projection(in_data);

    Ok(runtime)
}

fn downsample_argb_block(
    pixels: &[u8],
    width: usize,
    x0: usize,
    y0: usize,
    x1: usize,
    y1: usize,
    source_premultiplied: bool,
) -> [u8; 4] {
    let count = ((x1 - x0) * (y1 - y0)) as u32;
    if count == 0 {
        return [0; 4];
    }

    let mut sum_a = 0u32;
    let mut sum_r = 0u32;
    let mut sum_g = 0u32;
    let mut sum_b = 0u32;
    let mut weighted_r = 0u32;
    let mut weighted_g = 0u32;
    let mut weighted_b = 0u32;

    for sy in y0..y1 {
        for sx in x0..x1 {
            let i = (sy * width + sx) * 4;
            if i + 3 >= pixels.len() {
                continue;
            }
            let a = pixels[i] as u32;
            let r = pixels[i + 1] as u32;
            let g = pixels[i + 2] as u32;
            let b = pixels[i + 3] as u32;
            sum_a += a;
            if source_premultiplied {
                if a > 0 {
                    sum_r += r;
                    sum_g += g;
                    sum_b += b;
                }
            } else if a > 0 {
                weighted_r += r * a;
                weighted_g += g * a;
                weighted_b += b * a;
            }
        }
    }

    if source_premultiplied {
        [
            (sum_a / count) as u8,
            (sum_r / count) as u8,
            (sum_g / count) as u8,
            (sum_b / count) as u8,
        ]
    } else if sum_a > 0 {
        [
            (sum_a / count) as u8,
            (weighted_r / sum_a).min(255) as u8,
            (weighted_g / sum_a).min(255) as u8,
            (weighted_b / sum_a).min(255) as u8,
        ]
    } else {
        [0; 4]
    }
}

fn generate_mip_chain_with_alpha_mode(
    base_w: usize,
    base_h: usize,
    base_pixels: &[u8],
    source_premultiplied: bool,
) -> Vec<renderer::MipLevel> {
    let mut mips = Vec::new();
    let mut src_w = base_w;
    let mut src_h = base_h;
    let mut src = base_pixels.to_vec();
    while src_w > 2 || src_h > 2 {
        let nw = (src_w / 2).max(1);
        let nh = (src_h / 2).max(1);
        let mut dst = vec![0u8; nw * nh * 4];
        for oy in 0..nh {
            for ox in 0..nw {
                let x0 = ox * 2;
                let y0 = oy * 2;
                let x1 = (x0 + 2).min(src_w);
                let y1 = (y0 + 2).min(src_h);
                let px = downsample_argb_block(&src, src_w, x0, y0, x1, y1, source_premultiplied);
                let di = (oy * nw + ox) * 4;
                dst[di] = px[0];
                dst[di + 1] = px[1];
                dst[di + 2] = px[2];
                dst[di + 3] = px[3];
            }
        }
        mips.push(renderer::MipLevel {
            width: nw,
            height: nh,
            pixels: dst.clone(),
        });
        src = dst;
        src_w = nw;
        src_h = nh;
    }
    mips
}

#[cfg(test)]
fn generate_mip_chain(base_w: usize, base_h: usize, base_pixels: &[u8]) -> Vec<renderer::MipLevel> {
    generate_mip_chain_with_alpha_mode(base_w, base_h, base_pixels, true)
}

fn finish_sprite(width: usize, height: usize, pixels: Vec<u8>) -> SpriteImage {
    finish_sprite_with_alpha_mode(width, height, pixels, true)
}

fn finish_sprite_with_alpha_mode(
    width: usize,
    height: usize,
    pixels: Vec<u8>,
    source_premultiplied: bool,
) -> SpriteImage {
    let mips = generate_mip_chain_with_alpha_mode(width, height, &pixels, source_premultiplied);
    SpriteImage {
        width,
        height,
        pixels: Arc::new(pixels),
        mips: Arc::new(mips),
    }
}

fn build_sprite_image(
    layer: &ae::Layer,
    proxy_divisor: u8,
    source_premultiplied: bool,
) -> SpriteImage {
    let (flat, width, height) = layer_to_flat(layer);
    if width == 0 || height == 0 || flat.is_empty() {
        return finish_sprite(1, 1, vec![255, 255, 255, 255]);
    }

    let step = proxy_divisor.max(1) as usize;
    if step <= 1 {
        return finish_sprite_with_alpha_mode(width, height, flat, source_premultiplied);
    }

    let out_w = width.div_ceil(step).max(1);
    let out_h = height.div_ceil(step).max(1);
    let mut pixels = vec![0u8; out_w * out_h * 4];

    for oy in 0..out_h {
        for ox in 0..out_w {
            let x0 = ox * step;
            let y0 = oy * step;
            let x1 = (x0 + step).min(width);
            let y1 = (y0 + step).min(height);
            let px = downsample_argb_block(&flat, width, x0, y0, x1, y1, source_premultiplied);
            let dst_idx = (oy * out_w + ox) * 4;
            if dst_idx + 3 < pixels.len() {
                pixels[dst_idx] = px[0];
                pixels[dst_idx + 1] = px[1];
                pixels[dst_idx + 2] = px[2];
                pixels[dst_idx + 3] = px[3];
            }
        }
    }

    finish_sprite_with_alpha_mode(out_w, out_h, pixels, source_premultiplied)
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
    flat.iter()
        .map(|v| format!("{:.3}", v))
        .collect::<Vec<_>>()
        .join(",")
}

fn fmt_mat44(m: &[[f64; 4]; 4]) -> String {
    let flat: [f64; 16] = [
        m[0][0], m[0][1], m[0][2], m[0][3], m[1][0], m[1][1], m[1][2], m[1][3], m[2][0], m[2][1],
        m[2][2], m[2][3], m[3][0], m[3][1], m[3][2], m[3][3],
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
        ("topleft_z0", glam::Vec3::new(0.0, 0.0, 0.0)),
        ("center_z0", glam::Vec3::new(cw * 0.5, ch * 0.5, 0.0)),
        ("center_zp500", glam::Vec3::new(cw * 0.5, ch * 0.5, 500.0)),
        ("center_zn500", glam::Vec3::new(cw * 0.5, ch * 0.5, -500.0)),
        ("right_z0", glam::Vec3::new(cw, ch * 0.5, 0.0)),
    ];
    let proj_str = test_points
        .iter()
        .map(|(name, p)| match project_point_3d(*p, projection) {
            Some((sx, sy, d)) => format!("{}=({:.1},{:.1},d{:.1})", name, sx, sy, d),
            None => format!("{}=cull", name),
        })
        .collect::<Vec<_>>()
        .join(" ");

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
    let time = ae::Time {
        value: in_data.current_time(),
        scale: in_data.time_scale(),
    };
    let result = panic::catch_unwind(AssertUnwindSafe(|| in_data.effect().camera_matrix(time)));
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
        [flat[0], flat[4], flat[8], flat[12]],
        [flat[1], flat[5], flat[9], flat[13]],
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
    project_state: &ParticleLabProjectState,
    in_data: &ae::InData,
    in_layer: &ae::Layer,
    out_layer: &mut ae::Layer,
) -> Result<(), ae::Error> {
    let (engine, source) = extract_runtime_engine_config(params, project_state)?;

    let out_w = out_layer.width() as usize;
    let out_h = out_layer.height() as usize;
    let out_origin = out_layer.origin();
    let runtime = collect_particle_runtime_inputs(
        params,
        in_data,
        &engine,
        out_w,
        out_h,
        out_origin.h,
        out_origin.v,
    )?;
    let plan = ParticleRenderPlan::new(&engine, runtime);
    let composite_on_original = engine.render.composite_on_original;

    let final_apply_mode = plan.final_apply_mode;
    let (original, in_w, in_h) = if final_apply_mode.is_some() {
        layer_to_flat(in_layer)
    } else {
        (Vec::new(), 0, 0)
    };
    let output_len = checked_rgba_len(out_w, out_h)?;
    let mut output = vec![0u8; output_len];
    if matches!(final_apply_mode, Some(ApplyMode::Normal)) {
        let in_origin = in_layer.origin();
        renderer::blit_argb_into(
            &original,
            in_w,
            in_h,
            in_origin.h,
            in_origin.v,
            &mut output,
            out_w,
            out_h,
            out_origin.h,
            out_origin.v,
        );
    }

    let particle_count =
        render_particle_engine_8bit(engine, plan.clone(), &mut output, None, || {
            Ok::<(), ae::Error>(())
        })?;

    if let Some(mode) = final_apply_mode {
        if !matches!(mode, ApplyMode::Normal) {
            let in_origin = in_layer.origin();
            renderer::apply_final_composite(
                &original,
                in_w,
                in_h,
                in_origin.h,
                in_origin.v,
                &mut output,
                out_w,
                out_h,
                out_origin.h,
                out_origin.v,
                mode,
            );
        }
    }

    flat_to_layer(&output, out_layer, out_w, out_h);

    debug_info(format!(
        "Render frame time={:.3}s dt={:.3}s size={}x{} particles={} source={:?} composite={} origin=({}, {})",
        plan.runtime.frame.time,
        plan.runtime.frame.dt,
        out_w,
        out_h,
        particle_count,
        source,
        composite_on_original,
        plan.runtime.frame.origin_x,
        plan.runtime.frame.origin_y
    ));

    Ok(())
}

fn smart_pre_render_particles(
    params: &ae::Parameters<Params>,
    project_state: &ParticleLabProjectState,
    in_data: &ae::InData,
    extra: &mut ae::pf::PreRenderExtra,
) -> Result<(), ae::Error> {
    let (engine, source) = extract_runtime_engine_config(params, project_state)?;

    let req = extra.output_request();
    let cb = extra.callbacks();
    let in_result = cb.checkout_layer(
        0,
        0,
        &req,
        in_data.current_time(),
        in_data.time_step(),
        in_data.time_scale(),
    )?;

    let in_rect: ae::Rect = in_result.result_rect.into();
    let in_max: ae::Rect = in_result.max_result_rect.into();
    let margin = estimate_render_margin_from_engine(&engine);
    let expanded_input = ae::Rect {
        left: in_rect.left.saturating_sub(margin),
        top: in_rect.top.saturating_sub(margin),
        right: in_rect.right.saturating_add(margin),
        bottom: in_rect.bottom.saturating_add(margin),
    };
    let expanded_max = ae::Rect {
        left: in_max.left.saturating_sub(margin),
        top: in_max.top.saturating_sub(margin),
        right: in_max.right.saturating_add(margin),
        bottom: in_max.bottom.saturating_add(margin),
    };
    let emitter_rect = estimated_emitter_bounds_from_engine(&engine, margin);
    let expanded =
        clamp_rect_to_pixel_budget(union_rect(expanded_input, emitter_rect), MAX_OUTPUT_PIXELS);
    let max_expanded =
        clamp_rect_to_pixel_budget(union_rect(expanded_max, emitter_rect), MAX_OUTPUT_PIXELS);
    extra.set_result_rect(expanded);
    extra.set_max_result_rect(max_expanded);
    extra.set_returns_extra_pixels(true);

    let expected_output_w = (expanded.right - expanded.left).max(1) as usize;
    let expected_output_h = (expanded.bottom - expanded.top).max(1) as usize;
    let expected_origin_x = expanded.left;
    let expected_origin_y = expanded.top;
    // Collect all runtime data on the PreRender thread; SmartRender does not
    // call AE param, layer-source, path, or camera APIs.
    let runtime = collect_particle_runtime_inputs(
        params,
        in_data,
        &engine,
        expected_output_w,
        expected_output_h,
        expected_origin_x,
        expected_origin_y,
    )?;
    let plan = ParticleRenderPlan::new(&engine, runtime);
    extra.set_pre_render_data(SmartRenderData {
        engine,
        plan,
        source,
    });

    debug_info(format!(
        "SmartPreRender source={:?} in_rect left={} top={} right={} bottom={} expanded_margin={} expanded_rect=({}, {}, {}, {})",
        source,
        in_rect.left,
        in_rect.top,
        in_rect.right,
        in_rect.bottom,
        margin,
        expanded.left,
        expanded.top,
        expanded.right,
        expanded.bottom
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
    // Retrieve data collected in SmartPreRender  Eno AE API calls on render thread.
    let data = extra
        .pre_render_data::<SmartRenderData>()
        .ok_or(ae::Error::Generic)?;
    let engine = data.engine.clone();
    let plan = data.plan.clone();
    let source = data.source;

    let cb = extra.callbacks();

    // === Phase 1: Read input pixels into local memory ===
    // Keep input checked out so AE's "input before output" requirement is met for Phase 3.
    let input_world = cb.checkout_layer_pixels(0)?.ok_or(ae::Error::Generic)?;
    let input_origin = input_world.origin();
    let final_apply_mode = plan.final_apply_mode;
    let need_original = final_apply_mode.is_some();
    let (original, input_w, input_h) = if need_original {
        layer_to_flat(&input_world)
    } else {
        (Vec::new(), 0, 0)
    };

    let output_w = plan.runtime.frame.surface.width;
    let output_h = plan.runtime.frame.surface.height;
    let apply_mode = engine.render.apply_mode;
    let camera_projection_enabled = plan.runtime.camera_projection.is_some();

    debug_info(format!(
        "SmartRender V10 START output={}x{} source={:?} apply={:?} elapsed={}ms",
        output_w,
        output_h,
        source,
        apply_mode,
        render_start.elapsed().as_millis()
    ));

    let output_len = checked_rgba_len(output_w, output_h)?;
    let origin_x = plan.runtime.frame.origin_x;
    let origin_y = plan.runtime.frame.origin_y;
    let mut output = vec![0u8; output_len];
    if matches!(final_apply_mode, Some(ApplyMode::Normal)) {
        renderer::blit_argb_into(
            &original,
            input_w,
            input_h,
            input_origin.h,
            input_origin.v,
            &mut output,
            output_w,
            output_h,
            origin_x,
            origin_y,
        );
    }

    check_render_abort(in_data)?;
    let deadline = Instant::checked_add(
        &render_start,
        std::time::Duration::from_millis(RENDER_TIME_BUDGET_MS as u64),
    );
    let particle_count =
        render_particle_engine_8bit(engine, plan.clone(), &mut output, deadline, || {
            check_render_abort(in_data)
        })?;
    debug_info(format!(
        "SmartRender V9 STEP:engine particles={} elapsed={}ms",
        particle_count,
        render_start.elapsed().as_millis()
    ));

    if let Some(mode) = final_apply_mode {
        if !matches!(mode, ApplyMode::Normal) {
            renderer::apply_final_composite(
                &original,
                input_w,
                input_h,
                input_origin.h,
                input_origin.v,
                &mut output,
                output_w,
                output_h,
                origin_x,
                origin_y,
                mode,
            );
        }
    }
    drop(original);

    check_render_abort(in_data)?;

    debug_info(format!(
        "SmartRender V10 STEP:pre_write elapsed={}ms",
        render_start.elapsed().as_millis()
    ));

    // === Phase 3: Checkout output and write result  Ehold AE buffer as briefly as possible ===
    let mut output_world = cb.checkout_output()?.ok_or(ae::Error::Generic)?;
    let bit_depth = output_world.bit_depth();
    flat_to_layer(&output, &mut output_world, output_w, output_h);
    // output_world and _input_world2 drop here

    debug_info(format!(
        "SmartRender V9 {}bpc time={:.3}s output={}x{} particles={} cam={}",
        bit_depth,
        plan.runtime.frame.time,
        output_w,
        output_h,
        particle_count,
        camera_projection_enabled
    ));
    Ok(())
}
