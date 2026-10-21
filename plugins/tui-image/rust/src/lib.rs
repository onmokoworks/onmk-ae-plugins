use after_effects as ae;
mod gpu;
use serde::Deserialize;
use std::sync::OnceLock;
use std::thread;

static GPU_RENDERER: OnceLock<Option<gpu::GpuProcessor>> = OnceLock::new();
static PRESET_REGISTRY: OnceLock<PresetRegistry> = OnceLock::new();

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params {
    Preset,
    ScaleGroupStart,
    UniformScale,
    GlyphScaleX,
    GlyphScaleY,
    ColumnGap,
    RowGap,
    ScaleGroupEnd,
    UniformCellSize,
    CellWidth,
    CellHeight,
    Columns,
    Rows,
    RenderScale,
    Contrast,
    Gamma,
    EdgeBoost,
    Invert,
    ColorMode,
    Foreground,
    Background,
    SourceMix,
    UseSourceLuma,
    PreserveSourceColor,
}

#[derive(Default)]
struct Plugin;

ae::define_effect!(Plugin, (), Params);

const PRESET_ASCII: i32 = 1;
const PRESET_BLOCK: i32 = 2;
const PRESET_BRAILLE: i32 = 3;
const PRESET_TUI_GRADIENT: i32 = 4;

const COLOR_MONO: i32 = 1;
const COLOR_SOURCE: i32 = 2;
const COLOR_GRADIENT: i32 = 3;
const COLOR_LEGACY_PRESET: i32 = 5;

const DEFAULT_FOREGROUND: [f32; 3] = [1.0, 1.0, 1.0];
const DEFAULT_BACKGROUND: [f32; 3] = [0.0, 0.0, 0.0];

impl AdobePluginGlobal for Plugin {
    fn params_setup(
        &self,
        params: &mut ae::Parameters<Params>,
        _in_data: ae::InData,
        _out_data: ae::OutData,
    ) -> Result<(), ae::Error> {
        params.add(
            Params::Preset,
            "Preset",
            ae::PopupDef::setup(|f| {
                f.set_options(&["ASCII Classic", "Block", "Braille", "TUI Gradient"]);
                f.set_default(PRESET_BLOCK);
            }),
        )?;

        params.add_group(
            Params::ScaleGroupStart,
            Params::ScaleGroupEnd,
            "Scale",
            true,
            |params| {
                params.add_with_flags(
                    Params::UniformScale,
                    "Uniform Scale",
                    ae::CheckBoxDef::setup(|f| {
                        f.set_default(true);
                        f.set_label("Uniform Scale");
                    }),
                    ae::ParamFlag::SUPERVISE
                        | ae::ParamFlag::CANNOT_TIME_VARY
                        | ae::ParamFlag::CANNOT_INTERP,
                    ae::ParamUIFlags::empty(),
                )?;

                params.add_with_flags(
                    Params::GlyphScaleX,
                    "Scale",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(10.0);
                        f.set_valid_max(100.0);
                        f.set_slider_min(25.0);
                        f.set_slider_max(100.0);
                        f.set_default(100.0);
                        f.set_precision(1);
                        f.set_display_flags(ae::ValueDisplayFlag::PERCENT);
                    }),
                    ae::ParamFlag::SUPERVISE | ae::ParamFlag::START_COLLAPSED,
                    ae::ParamUIFlags::empty(),
                )?;

                params.add_with_flags(
                    Params::GlyphScaleY,
                    " ",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(10.0);
                        f.set_valid_max(100.0);
                        f.set_slider_min(25.0);
                        f.set_slider_max(100.0);
                        f.set_default(100.0);
                        f.set_precision(1);
                        f.set_display_flags(ae::ValueDisplayFlag::PERCENT);
                    }),
                    ae::ParamFlag::SUPERVISE | ae::ParamFlag::START_COLLAPSED,
                    ae::ParamUIFlags::DISABLED,
                )?;

                params.add(
                    Params::ColumnGap,
                    "Column Gap %",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.0);
                        f.set_valid_max(80.0);
                        f.set_slider_min(0.0);
                        f.set_slider_max(50.0);
                        f.set_default(0.0);
                        f.set_precision(1);
                        f.set_display_flags(ae::ValueDisplayFlag::PERCENT);
                    }),
                )?;

                params.add(
                    Params::RowGap,
                    "Row Gap %",
                    ae::FloatSliderDef::setup(|f| {
                        f.set_valid_min(0.0);
                        f.set_valid_max(80.0);
                        f.set_slider_min(0.0);
                        f.set_slider_max(50.0);
                        f.set_default(0.0);
                        f.set_precision(1);
                        f.set_display_flags(ae::ValueDisplayFlag::PERCENT);
                    }),
                )?;

                Ok(())
            },
        )?;

        params.add_with_flags(
            Params::UniformCellSize,
            "Uniform Cell Size",
            ae::CheckBoxDef::setup(|f| {
                f.set_default(false);
                f.set_label("Uniform Cell Size");
            }),
            ae::ParamFlag::SUPERVISE
                | ae::ParamFlag::CANNOT_TIME_VARY
                | ae::ParamFlag::CANNOT_INTERP,
            ae::ParamUIFlags::empty(),
        )?;

        params.add_with_flags(
            Params::CellWidth,
            "Cell Width",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(2);
                f.set_valid_max(64);
                f.set_slider_min(4);
                f.set_slider_max(32);
                f.set_default(10);
            }),
            ae::ParamFlag::SUPERVISE | ae::ParamFlag::START_COLLAPSED,
            ae::ParamUIFlags::empty(),
        )?;

        params.add_with_flags(
            Params::CellHeight,
            "Cell Height",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(2);
                f.set_valid_max(64);
                f.set_slider_min(4);
                f.set_slider_max(32);
                f.set_default(16);
            }),
            ae::ParamFlag::SUPERVISE | ae::ParamFlag::START_COLLAPSED,
            ae::ParamUIFlags::empty(),
        )?;

        params.add(
            Params::Columns,
            "Columns (0=Auto)",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(0);
                f.set_valid_max(1000);
                f.set_slider_min(0);
                f.set_slider_max(240);
                f.set_default(0);
            }),
        )?;

        params.add(
            Params::Rows,
            "Rows (0=Auto)",
            ae::SliderDef::setup(|f| {
                f.set_valid_min(0);
                f.set_valid_max(1000);
                f.set_slider_min(0);
                f.set_slider_max(160);
                f.set_default(0);
            }),
        )?;

        params.add(
            Params::RenderScale,
            "Render Scale",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(10.0);
                f.set_valid_max(200.0);
                f.set_slider_min(25.0);
                f.set_slider_max(100.0);
                f.set_default(100.0);
                f.set_precision(1);
            }),
        )?;

        params.add(
            Params::Contrast,
            "Contrast",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.1);
                f.set_valid_max(5.0);
                f.set_slider_min(0.5);
                f.set_slider_max(2.5);
                f.set_default(1.0);
                f.set_precision(2);
            }),
        )?;

        params.add(
            Params::Gamma,
            "Gamma",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.1);
                f.set_valid_max(5.0);
                f.set_slider_min(0.5);
                f.set_slider_max(2.5);
                f.set_default(1.0);
                f.set_precision(2);
            }),
        )?;

        params.add(
            Params::EdgeBoost,
            "Edge Boost",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(5.0);
                f.set_slider_min(0.0);
                f.set_slider_max(2.0);
                f.set_default(0.0);
                f.set_precision(2);
            }),
        )?;

        params.add(
            Params::Invert,
            "Invert",
            ae::CheckBoxDef::setup(|f| {
                f.set_default(false);
                f.set_label("Invert Luma");
            }),
        )?;

        params.add(
            Params::ColorMode,
            "Color Mode",
            ae::PopupDef::setup(|f| {
                f.set_options(&["Mono", "Source", "Gradient"]);
                f.set_default(COLOR_MONO);
            }),
        )?;

        params.add(
            Params::Foreground,
            "Foreground",
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
            Params::Background,
            "Background",
            ae::ColorDef::setup(|f| {
                f.set_default(ae::Pixel8 {
                    alpha: 255,
                    red: 0,
                    green: 0,
                    blue: 0,
                });
            }),
        )?;

        params.add(
            Params::SourceMix,
            "Source Mix",
            ae::FloatSliderDef::setup(|f| {
                f.set_valid_min(0.0);
                f.set_valid_max(100.0);
                f.set_slider_min(0.0);
                f.set_slider_max(100.0);
                f.set_default(0.0);
                f.set_precision(1);
            }),
        )?;

        params.add(
            Params::UseSourceLuma,
            "Use Source Luma",
            ae::CheckBoxDef::setup(|f| {
                f.set_default(true);
                f.set_label("Map density from source brightness");
            }),
        )?;

        params.add(
            Params::PreserveSourceColor,
            "Preserve Source Color",
            ae::CheckBoxDef::setup(|f| {
                f.set_default(false);
                f.set_label("Color glyphs from source");
            }),
        )?;

        Ok(())
    }

    fn handle_command(
        &mut self,
        cmd: ae::Command,
        in_data: ae::InData,
        mut out_data: ae::OutData,
        params: &mut ae::Parameters<Params>,
    ) -> Result<(), ae::Error> {
        match cmd {
            ae::Command::About => {
                out_data.set_return_msg(
                    "TuiImage v0.1\rRust/WGPU TUI-style renderer.\rASCII / Block / Braille / TUI Gradient raster presets.",
                );
            }
            ae::Command::Render {
                in_layer,
                mut out_layer,
            } => {
                let (src, w, h) = layer_to_flat(&in_layer)?;
                let ep = get_params(params)?;
                let out = render_tui(&src, w, h, &ep)?;
                flat_to_layer(&out, &mut out_layer, w, h)?;
            }
            ae::Command::SmartPreRender { mut extra } => {
                smart_pre_render(&in_data, &mut extra)?;
            }
            ae::Command::SmartRender { extra } | ae::Command::SmartRenderGpu { extra } => {
                smart_render(&extra, params)?;
            }
            ae::Command::UpdateParamsUi => {
                update_scale_ui(params)?;
                update_cell_ui(params)?;
                out_data.set_out_flag(ae::OutFlags::RefreshUi, true);
            }
            ae::Command::UserChangedParam { param_index } => {
                let param = params.type_at(param_index);
                if param == Params::Preset {
                    apply_selected_preset_to_params(params)?;
                }

                let uniform_scale = params.get(Params::UniformScale)?.as_checkbox()?.value();
                if uniform_scale && (param == Params::UniformScale || param == Params::GlyphScaleX)
                {
                    let x = params.get(Params::GlyphScaleX)?.as_float_slider()?.value();
                    let mut y = params.get_mut(Params::GlyphScaleY)?;
                    y.as_float_slider_mut()?.set_value(x);
                    y.update_param_ui()?;
                }

                let uniform_cell = params.get(Params::UniformCellSize)?.as_checkbox()?.value();
                if uniform_cell && (param == Params::UniformCellSize || param == Params::CellWidth)
                {
                    let width = params.get(Params::CellWidth)?.as_slider()?.value();
                    let mut height = params.get_mut(Params::CellHeight)?;
                    height.as_slider_mut()?.set_value(width);
                    height.update_param_ui()?;
                }

                update_scale_ui(params)?;
                update_cell_ui(params)?;
                out_data.set_out_flag(ae::OutFlags::RefreshUi, true);
                out_data.set_force_rerender();
            }
            _ => {}
        }

        Ok(())
    }
}

#[derive(Clone, Copy)]
pub(crate) struct TuiParams {
    pub(crate) preset: i32,
    pub(crate) cell_w: usize,
    pub(crate) cell_h: usize,
    pub(crate) columns: usize,
    pub(crate) rows: usize,
    pub(crate) render_scale: f32,
    pub(crate) contrast: f32,
    pub(crate) gamma: f32,
    pub(crate) edge_boost: f32,
    pub(crate) invert: bool,
    pub(crate) color_mode: i32,
    pub(crate) foreground: [f32; 3],
    pub(crate) background: [f32; 3],
    pub(crate) source_mix: f32,
    pub(crate) use_source_luma: bool,
    pub(crate) preserve_source_color: bool,
    pub(crate) uniform_scale: bool,
    pub(crate) glyph_scale_x: f32,
    pub(crate) glyph_scale_y: f32,
    pub(crate) column_gap: f32,
    pub(crate) row_gap: f32,
}

#[derive(Clone, Copy)]
struct PresetRuntime {
    cell_w: usize,
    cell_h: usize,
    contrast: f32,
    gamma: f32,
    edge_boost: f32,
    invert: bool,
    color_mode: i32,
    foreground: [f32; 3],
    background: [f32; 3],
}

struct PresetRegistry {
    ascii: PresetRuntime,
    block: PresetRuntime,
    braille: PresetRuntime,
    tui_gradient: PresetRuntime,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PresetFile {
    cell: PresetCell,
    sampling: PresetSampling,
    color: PresetColor,
}

#[derive(Deserialize)]
struct PresetCell {
    width: usize,
    height: usize,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PresetSampling {
    contrast: f32,
    gamma: f32,
    #[serde(default)]
    edge_boost: f32,
    #[serde(default)]
    invert: bool,
}

#[derive(Deserialize)]
struct PresetColor {
    mode: String,
    foreground: Option<String>,
    background: Option<String>,
}

fn get_params(params: &ae::Parameters<Params>) -> Result<TuiParams, ae::Error> {
    let uniform_cell_size = params.get(Params::UniformCellSize)?.as_checkbox()?.value();
    let cell_w = params
        .get(Params::CellWidth)?
        .as_slider()?
        .value()
        .clamp(2, 64) as usize;
    let raw_cell_h = params
        .get(Params::CellHeight)?
        .as_slider()?
        .value()
        .clamp(2, 64) as usize;

    let preset = params.get(Params::Preset)?.as_popup()?.value() as i32;
    let raw_color_mode = params.get(Params::ColorMode)?.as_popup()?.value() as i32;
    let color_mode = normalize_color_mode(preset, raw_color_mode);

    Ok(TuiParams {
        preset,
        cell_w,
        cell_h: if uniform_cell_size {
            cell_w
        } else {
            raw_cell_h
        },
        columns: params
            .get(Params::Columns)?
            .as_slider()?
            .value()
            .clamp(0, 1000) as usize,
        rows: params
            .get(Params::Rows)?
            .as_slider()?
            .value()
            .clamp(0, 1000) as usize,
        render_scale: (params
            .get(Params::RenderScale)?
            .as_float_slider()?
            .value()
            .clamp(10.0, 200.0) as f32)
            / 100.0,
        contrast: params
            .get(Params::Contrast)?
            .as_float_slider()?
            .value()
            .clamp(0.1, 5.0) as f32,
        gamma: params
            .get(Params::Gamma)?
            .as_float_slider()?
            .value()
            .clamp(0.1, 5.0) as f32,
        edge_boost: params
            .get(Params::EdgeBoost)?
            .as_float_slider()?
            .value()
            .clamp(0.0, 5.0) as f32,
        invert: params.get(Params::Invert)?.as_checkbox()?.value(),
        color_mode,
        foreground: pixel_to_rgb(params.get(Params::Foreground)?.as_color()?.value()),
        background: pixel_to_rgb(params.get(Params::Background)?.as_color()?.value()),
        source_mix: (params
            .get(Params::SourceMix)?
            .as_float_slider()?
            .value()
            .clamp(0.0, 100.0) as f32)
            / 100.0,
        use_source_luma: params.get(Params::UseSourceLuma)?.as_checkbox()?.value(),
        preserve_source_color: params
            .get(Params::PreserveSourceColor)?
            .as_checkbox()?
            .value(),
        uniform_scale: params.get(Params::UniformScale)?.as_checkbox()?.value(),
        glyph_scale_x: (params
            .get(Params::GlyphScaleX)?
            .as_float_slider()?
            .value()
            .clamp(10.0, 100.0) as f32)
            / 100.0,
        glyph_scale_y: (params
            .get(Params::GlyphScaleY)?
            .as_float_slider()?
            .value()
            .clamp(10.0, 100.0) as f32)
            / 100.0,
        column_gap: (params
            .get(Params::ColumnGap)?
            .as_float_slider()?
            .value()
            .clamp(0.0, 80.0) as f32)
            / 100.0,
        row_gap: (params
            .get(Params::RowGap)?
            .as_float_slider()?
            .value()
            .clamp(0.0, 80.0) as f32)
            / 100.0,
    })
}

fn preset_registry() -> &'static PresetRegistry {
    PRESET_REGISTRY.get_or_init(PresetRegistry::load)
}

impl PresetRegistry {
    fn load() -> Self {
        Self {
            ascii: load_preset(include_str!("../../presets/ascii-classic.json")),
            block: load_preset(include_str!("../../presets/block.json")),
            braille: load_preset(include_str!("../../presets/braille.json")),
            tui_gradient: load_preset(include_str!("../../presets/tui-gradient.json")),
        }
    }

    fn get(&self, preset: i32) -> PresetRuntime {
        match preset {
            PRESET_ASCII => self.ascii,
            PRESET_BRAILLE => self.braille,
            PRESET_TUI_GRADIENT => self.tui_gradient,
            PRESET_BLOCK | _ => self.block,
        }
    }
}

fn load_preset(raw: &str) -> PresetRuntime {
    let file: PresetFile =
        serde_json::from_str(raw).expect("preset JSON should parse at compile time");
    PresetRuntime {
        cell_w: file.cell.width.clamp(2, 64),
        cell_h: file.cell.height.clamp(2, 64),
        contrast: file.sampling.contrast.clamp(0.1, 5.0),
        gamma: file.sampling.gamma.clamp(0.1, 5.0),
        edge_boost: file.sampling.edge_boost.clamp(0.0, 5.0),
        invert: file.sampling.invert,
        color_mode: parse_color_mode(&file.color.mode),
        foreground: parse_color_value(file.color.foreground.as_deref(), DEFAULT_FOREGROUND),
        background: parse_color_value(file.color.background.as_deref(), DEFAULT_BACKGROUND),
    }
}

fn parse_color_mode(mode: &str) -> i32 {
    match mode {
        "mono" => COLOR_MONO,
        "gradient" => COLOR_GRADIENT,
        "source" | _ => COLOR_SOURCE,
    }
}

fn normalize_color_mode(preset: i32, mode: i32) -> i32 {
    match mode {
        COLOR_MONO | COLOR_SOURCE | COLOR_GRADIENT => mode,
        COLOR_LEGACY_PRESET => preset_registry().get(preset).color_mode,
        _ => COLOR_MONO,
    }
}

fn preset_step_count(preset: i32) -> i32 {
    match preset {
        PRESET_ASCII => 10,
        PRESET_BRAILLE => 8,
        PRESET_TUI_GRADIENT => 12,
        PRESET_BLOCK | _ => 8,
    }
}

fn parse_color_value(raw: Option<&str>, fallback: [f32; 3]) -> [f32; 3] {
    let Some(raw) = raw else {
        return fallback;
    };
    parse_hex_rgb(raw).unwrap_or(fallback)
}

fn parse_hex_rgb(raw: &str) -> Option<[f32; 3]> {
    let hex = raw.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some([r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0])
}

fn apply_preset_defaults(ep: &TuiParams) -> TuiParams {
    let preset = preset_registry().get(ep.preset);
    let mut out = *ep;

    if out.cell_w == 10 {
        out.cell_w = preset.cell_w;
    }
    if out.cell_h == 16 {
        out.cell_h = preset.cell_h;
    }
    if (out.contrast - 1.0).abs() < f32::EPSILON {
        out.contrast = preset.contrast;
    }
    if (out.gamma - 1.0).abs() < f32::EPSILON {
        out.gamma = preset.gamma;
    }
    if out.edge_boost.abs() < f32::EPSILON {
        out.edge_boost = preset.edge_boost;
    }
    if !out.invert {
        out.invert = preset.invert;
    }
    out
}

fn apply_selected_preset_to_params(params: &mut ae::Parameters<Params>) -> Result<(), ae::Error> {
    let preset_id = params.get(Params::Preset)?.as_popup()?.value() as i32;
    let preset = preset_registry().get(preset_id);

    {
        let mut cell_w = params.get_mut(Params::CellWidth)?;
        cell_w.as_slider_mut()?.set_value(preset.cell_w as i32);
        cell_w.update_param_ui()?;
    }
    {
        let mut cell_h = params.get_mut(Params::CellHeight)?;
        cell_h.as_slider_mut()?.set_value(preset.cell_h as i32);
        cell_h.update_param_ui()?;
    }
    {
        let mut contrast = params.get_mut(Params::Contrast)?;
        contrast
            .as_float_slider_mut()?
            .set_value(preset.contrast as f64);
        contrast.update_param_ui()?;
    }
    {
        let mut gamma = params.get_mut(Params::Gamma)?;
        gamma.as_float_slider_mut()?.set_value(preset.gamma as f64);
        gamma.update_param_ui()?;
    }
    {
        let mut edge_boost = params.get_mut(Params::EdgeBoost)?;
        edge_boost
            .as_float_slider_mut()?
            .set_value(preset.edge_boost as f64);
        edge_boost.update_param_ui()?;
    }
    {
        let mut invert = params.get_mut(Params::Invert)?;
        invert.as_checkbox_mut()?.set_value(preset.invert);
        invert.update_param_ui()?;
    }
    {
        let mut color_mode = params.get_mut(Params::ColorMode)?;
        color_mode.as_popup_mut()?.set_value(preset.color_mode);
        color_mode.update_param_ui()?;
    }
    {
        let mut foreground = params.get_mut(Params::Foreground)?;
        foreground
            .as_color_mut()?
            .set_value(rgb_to_pixel(preset.foreground));
        foreground.update_param_ui()?;
    }
    {
        let mut background = params.get_mut(Params::Background)?;
        background
            .as_color_mut()?
            .set_value(rgb_to_pixel(preset.background));
        background.update_param_ui()?;
    }

    Ok(())
}

fn update_scale_ui(params: &ae::Parameters<Params>) -> Result<(), ae::Error> {
    let uniform = params.get(Params::UniformScale)?.as_checkbox()?.value();
    let mut params_copy = params.cloned();

    {
        let mut scale_x = params_copy.get_mut(Params::GlyphScaleX)?;
        scale_x.set_name(if uniform { "Scale" } else { "Column" })?;
        scale_x.update_param_ui()?;
    }

    {
        let mut scale_y = params_copy.get_mut(Params::GlyphScaleY)?;
        scale_y.set_name(if uniform { " " } else { "Row" })?;
        scale_y.set_ui_flag(ae::ParamUIFlags::DISABLED, uniform);
        scale_y.update_param_ui()?;
    }

    Ok(())
}

fn update_cell_ui(params: &ae::Parameters<Params>) -> Result<(), ae::Error> {
    let uniform = params.get(Params::UniformCellSize)?.as_checkbox()?.value();
    let mut params_copy = params.cloned();

    {
        let mut cell_w = params_copy.get_mut(Params::CellWidth)?;
        cell_w.set_name(if uniform { "Cell Size" } else { "Cell Width" })?;
        cell_w.update_param_ui()?;
    }

    {
        let mut cell_h = params_copy.get_mut(Params::CellHeight)?;
        cell_h.set_name(if uniform { " " } else { "Cell Height" })?;
        cell_h.set_ui_flag(ae::ParamUIFlags::DISABLED, uniform);
        cell_h.update_param_ui()?;
    }

    Ok(())
}

fn smart_pre_render(
    in_data: &ae::InData,
    extra: &mut ae::pf::PreRenderExtra,
) -> Result<(), ae::Error> {
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

    let req_rect: ae::Rect = req.rect.into();
    let mut res: ae::Rect = in_result.result_rect.into();
    let mut max_res: ae::Rect = in_result.max_result_rect.into();
    res.left = res.left.max(req_rect.left);
    res.top = res.top.max(req_rect.top);
    res.right = res.right.min(req_rect.right);
    res.bottom = res.bottom.min(req_rect.bottom);
    max_res.left = max_res.left.max(req_rect.left);
    max_res.top = max_res.top.max(req_rect.top);
    max_res.right = max_res.right.min(req_rect.right);
    max_res.bottom = max_res.bottom.min(req_rect.bottom);

    extra.set_result_rect(res);
    extra.set_max_result_rect(max_res);
    Ok(())
}

fn smart_render(
    extra: &ae::pf::SmartRenderExtra,
    params: &ae::Parameters<Params>,
) -> Result<(), ae::Error> {
    let cb = extra.callbacks();
    let input_world = cb.checkout_layer_pixels(0)?.ok_or(ae::Error::Generic)?;
    with_pixel_checkin(|| {
    let mut output_world = cb.checkout_output()?.ok_or(ae::Error::Generic)?;
    let (src, w, h) = layer_to_flat(&input_world)?;
    let ep = get_params(params)?;
    let out = render_tui(&src, w, h, &ep)?;
    flat_to_layer(&out, &mut output_world, w, h)?;
    Ok(())
    }, || cb.checkin_layer_pixels(0))
}

fn with_pixel_checkin<T>(work:impl FnOnce()->Result<T,ae::Error>, checkin:impl FnOnce()->Result<(),ae::Error>)->Result<T,ae::Error> {
    let result=std::panic::catch_unwind(std::panic::AssertUnwindSafe(work)).unwrap_or(Err(ae::Error::Generic));
    let released=checkin();
    match result {Err(error)=>Err(error),Ok(value)=>released.map(|()|value)}
}

#[cfg(test)]
mod cleanup_tests {
    use super::*;
    #[test]
    #[ignore = "requires a GPU; explicit parity audit"]
    fn gpu_matches_cpu_source_luma_disabled() {
        let mut gpu=gpu::GpuProcessor::new().expect("GPU required for this explicit audit");
        let (w,h)=(17,31);
        let src=[255,0,0,0].repeat(w*h);
        let mut ep=TuiParams {
            preset:PRESET_BLOCK,cell_w:4,cell_h:4,columns:0,rows:0,render_scale:1.0,
            contrast:1.0,gamma:1.0,edge_boost:0.0,invert:false,color_mode:COLOR_MONO,
            foreground:[1.0;3],background:[0.0;3],source_mix:0.0,
            use_source_luma:false,preserve_source_color:false,uniform_scale:true,
            glyph_scale_x:1.0,glyph_scale_y:1.0,column_gap:0.0,row_gap:0.0,
        };
        let actual=gpu.process(&src,w,h,&ep).expect("GPU must execute, not CPU fallback");
        let expected=render_tui_cpu(&src,w,h,&ep);
        let differences=actual.iter().zip(&expected).filter(|(a,b)|a!=b).count();
        assert_eq!(differences,0,"GPU/CPU byte mismatch count");
        for preserve in [false,true] {
            ep.preserve_source_color=preserve;
            let colored=[255,51,102,153].repeat(w*h);
            let actual=gpu.process(&colored,w,h,&ep).expect("GPU execution required");
            let expected=render_tui_cpu(&colored,w,h,&ep);
            let differences=actual.iter().zip(&expected).filter(|(a,b)|a!=b).count();
            assert_eq!(differences,0,"PreserveSourceColor={preserve}");
        }
        ep.preserve_source_color=false;
        for uniform in [false,true] {
            ep.uniform_scale=uniform;
            ep.glyph_scale_x=0.5;
            ep.glyph_scale_y=0.75;
            ep.column_gap=0.2;
            ep.row_gap=0.25;
            let actual=gpu.process(&src,w,h,&ep).expect("GPU execution required");
            let expected=render_tui_cpu(&src,w,h,&ep);
            let differences=actual.iter().zip(&expected).filter(|(a,b)|a!=b).count();
            assert_eq!(differences,0,"glyph size/gaps uniform={uniform}");
        }
        ep.use_source_luma=true;
        ep.glyph_scale_x=1.0; ep.glyph_scale_y=1.0; ep.column_gap=0.0; ep.row_gap=0.0;
        for preset in 1..=4 {
            ep.preset=preset;
            for value in [0u8,32,64,96,128,160,192,224,255] {
                let input=[255,value,value,value].repeat(w*h);
                let actual=gpu.process(&input,w,h,&ep).expect("GPU execution required");
                let expected=render_tui_cpu(&input,w,h,&ep);
                let differences=actual.iter().zip(&expected).filter(|(a,b)|a!=b).count();
                assert_eq!(differences,0,"preset={preset} gray={value}");
            }
        }
        let spatial:Vec<u8>=(0..w*h).flat_map(|i| {let v=if ((i%w)/2+(i/w)/2)%2==0 {32} else {192};[255,v,v,v]}).collect();
        assert!(cell_edge_score(&spatial,w,h,0,0,4,4)>0.1,"fixture must exercise edges");
        for boost in [0.0,1.0] {
            ep.preset=PRESET_BLOCK; ep.edge_boost=boost;
            let actual=gpu.process(&spatial,w,h,&ep).expect("GPU execution required");
            let expected=render_tui_cpu(&spatial,w,h,&ep);
            let differences=actual.iter().zip(&expected).filter(|(a,b)|a!=b).count();
            assert_eq!(differences,0,"spatial edge_boost={boost}");
        }
        ep.edge_boost=0.0;
        let colored:Vec<u8>=(0..w*h).flat_map(|i| [255,((i*37)%256) as u8,((i*73+19)%256) as u8,((i*11+83)%256) as u8]).collect();
        for mode in [COLOR_MONO,COLOR_SOURCE,COLOR_GRADIENT] {
            for amount in [0.0,0.37,1.0] {
                ep.color_mode=mode; ep.source_mix=amount;
                ep.foreground=[0.83,0.61,0.27]; ep.background=[0.07,0.13,0.19];
                let actual=gpu.process(&colored,w,h,&ep).expect("GPU color execution required");
                let expected=render_tui_cpu(&colored,w,h,&ep);
                let max_error=actual.iter().zip(&expected).map(|(a,b)|a.abs_diff(*b)).max().unwrap();
                assert_eq!(max_error,0,"color mode={mode} source_mix={amount}");
            }
        }
        for preset in 1..=4 {
            for (contrast,gamma,invert) in [(0.65,0.7,false),(1.3,1.8,false),(1.3,0.8,true)] {
                ep.preset=preset; ep.contrast=contrast; ep.gamma=gamma; ep.invert=invert;
                ep.color_mode=COLOR_GRADIENT; ep.source_mix=0.37;
                let actual=gpu.process(&colored,w,h,&ep).expect("GPU tone execution required");
                let expected=render_tui_cpu(&colored,w,h,&ep);
                let max_error=actual.iter().zip(&expected).map(|(a,b)|a.abs_diff(*b)).max().unwrap();
                assert_eq!(max_error,0,"preset={preset} contrast={contrast} gamma={gamma} invert={invert}");
            }
        }
        ep.preset=PRESET_BLOCK; ep.contrast=1.0; ep.gamma=1.0; ep.invert=false;
        ep.color_mode=COLOR_MONO; ep.source_mix=0.0;
        ep.foreground=[1.0;3]; ep.background=[0.0;3];
        for (rw,rh) in [(1,1),(63,65),(1,31),(31,1),(17,31),(1,1)] {
            let resized=[255,128,128,128].repeat(rw*rh);
            let actual=gpu.process(&resized,rw,rh,&ep).expect("GPU resize execution required");
            assert_eq!(actual,render_tui_cpu(&resized,rw,rh,&ep),"resize {rw}x{rh}");
            assert_eq!(gpu.cached_layouts(),[(rw as u32,rh as u32)],"same-thread cache must replace old size");
        }
        // A/B/A replay on one processor and fixed dimensions must not retain
        // old input, glyph geometry, uniforms, or pixels outside previous cells.
        let baseline=gpu.process(&colored,w,h,&ep).expect("GPU baseline");
        for (cw,ch) in [(1,1),(3,7),(17,31),(7,3)] {
            let old_cell=(ep.cell_w,ep.cell_h);
            for preset in 1..=4 {
                ep.cell_w=cw; ep.cell_h=ch; ep.preset=preset;
                let changed=gpu.process(&spatial,w,h,&ep).expect("GPU changed-state replay");
                assert_eq!(changed,render_tui_cpu(&spatial,w,h,&ep),"replay cell={cw}x{ch} preset={preset}");
                ep.cell_w=old_cell.0; ep.cell_h=old_cell.1; ep.preset=PRESET_BLOCK;
                assert_eq!(gpu.process(&colored,w,h,&ep).expect("GPU restored-state replay"),baseline);
            }
        }
        gpu.limit_cache_to_two_small_frames(17,31);
        for value in [32u8,64,96,128,160,192] {
            let input=[255,value,value,value].repeat(17*31);
            std::thread::scope(|scope| {
                let gpu=&gpu;
                let ep=&ep;
                scope.spawn(|| {
                    assert_eq!(gpu.process(&input,17,31,ep).expect("GPU worker execution required"),render_tui_cpu(&input,17,31,ep));
                }).join().unwrap();
            });
            assert!(gpu.cached_layouts().len()<=2,"cache must evict retired thread entries");
        }
        assert_eq!(gpu.cached_layouts().len(),2);
    }
    #[test]
    fn row_copy_validates_before_writing_and_preserves_padding() {
        let src:Vec<u8>=(0..30).collect();
        for sr in [false,true] {for dr in [false,true] {
            let mut dst=vec![99;30];
            copy_rows(&src,&mut dst,2,3,10,10,sr,dr).unwrap();
            for y in 0..3 {
                let s=(if sr {2-y} else {y})*10;
                let d=(if dr {2-y} else {y})*10;
                assert_eq!(&dst[d..d+8],&src[s..s+8]);
                assert_eq!(&dst[d+8..d+10],&[99;2]);
            }
        }}
        let mut dst=vec![99;30];
        assert!(copy_rows(&src[..10],&mut dst,2,3,10,10,false,false).is_err());
        assert_eq!(dst,[99;30]);
        assert!(checked_extent(2,3,7).is_err());
        assert!(checked_extent(usize::MAX,2,usize::MAX).is_err());
        assert_eq!(checked_extent(0,usize::MAX,0),Ok(0));
    }
    #[test]
    fn resize_checks_geometry_and_preserves_nearest_mapping() {
        let src=[255,10,20,30,128,40,50,60];
        assert_eq!(resize_nearest(&src,2,1,2,1).unwrap(),src);
        assert_eq!(resize_nearest(&src,2,1,3,1).unwrap(),[255,10,20,30,255,10,20,30,128,40,50,60]);
        assert_eq!(resize_nearest(&[],0,3,2,2).unwrap(),[0;16]);
        assert_eq!(resize_nearest(&src,2,1,0,2).unwrap(),Vec::<u8>::new());
        assert!(resize_nearest(&src[..3],2,1,2,1).is_err());
        assert!(resize_nearest(&[],usize::MAX,2,1,1).is_err());
        assert!(resize_nearest(&src,2,1,usize::MAX,2).is_err());
    }
    #[test]
    fn cpu_parallel_matches_serial_across_presets_and_controls() {
        for (w,h) in [(1,1),(1,17),(19,1),(17,31),(63,65)] {
            let src:Vec<u8>=(0..w*h).flat_map(|i| {
                let a=((i*53+17)%256) as u8;
                [a,a/2,a/3,a]
            }).collect();
            for preset in 1..=4 {for color_mode in 1..=3 {for variant in 0..2 {
                let ep=TuiParams {
                    preset,cell_w:3,cell_h:7,columns:0,rows:0,render_scale:1.0,
                    contrast:1.3,gamma:0.8,edge_boost:0.7,invert:variant==1,color_mode,
                    foreground:[0.9,0.6,0.3],background:[0.1,0.2,0.3],source_mix:if variant==0 {0.0} else {0.5},
                    use_source_luma:variant==0,preserve_source_color:variant==1,uniform_scale:false,
                    glyph_scale_x:0.75,glyph_scale_y:0.5,column_gap:0.1,row_gap:0.2,
                };
                let mut serial=vec![0;src.len()];
                render_rows(&mut serial,&src,&src,w,h,0,h,&ep);
                assert_eq!(render_tui_cpu(&src,w,h,&ep),serial,"{w}x{h} preset{preset} color{color_mode} variant{variant}");
            }}}
        }
    }
    #[test]
    fn acquired_input_released_on_success_error_and_panic() {
        for stage in 0..3 {for release_fails in [false,true] {
            let calls=std::cell::Cell::new(0);
            let result=with_pixel_checkin(|| match stage {
                0=>Ok(7),1=>Err(ae::Error::BadCallbackParameter),_=>panic!("injected render panic"),
            },|| {calls.set(calls.get()+1); if release_fails {Err(ae::Error::OutOfMemory)} else {Ok(())}});
            assert_eq!(calls.get(),1);
            assert_eq!(result,match stage {0=>if release_fails {Err(ae::Error::OutOfMemory)} else {Ok(7)},1=>Err(ae::Error::BadCallbackParameter),_=>Err(ae::Error::Generic)});
        }}
    }
}

fn checked_extent(w:usize,h:usize,stride:usize)->Result<usize,ae::Error> {
    if w==0 || h==0 {return Ok(0);}
    let row=w.checked_mul(4).ok_or(ae::Error::OutOfMemory)?;
    if stride<row {return Err(ae::Error::BadCallbackParameter);}
    (h-1).checked_mul(stride).and_then(|v|v.checked_add(row)).ok_or(ae::Error::OutOfMemory)
}

fn copy_rows(src:&[u8],dst:&mut[u8],w:usize,h:usize,ss:usize,ds:usize,sr:bool,dr:bool)->Result<(),ae::Error> {
    if src.len()<checked_extent(w,h,ss)? || dst.len()<checked_extent(w,h,ds)? {return Err(ae::Error::BadCallbackParameter);}
    if w==0 || h==0 {return Ok(());}
    let row=w*4;
    for y in 0..h {
        let s=(if sr {h-1-y} else {y})*ss;
        let d=(if dr {h-1-y} else {y})*ds;
        dst[d..d+row].copy_from_slice(&src[s..s+row]);
    }
    Ok(())
}

fn validate_world(layer:&ae::Layer)->Result<(),ae::Error> {
    if layer.width()>i32::MAX as usize || layer.height()>i32::MAX as usize || layer.row_bytes()==i32::MIN as isize {return Err(ae::Error::BadCallbackParameter);}
    if layer.width()==0 || layer.height()==0 {return Ok(());}
    checked_extent(layer.width(),layer.height(),layer.buffer_stride())?;
    let len=layer.buffer_stride().checked_mul(layer.height()).ok_or(ae::Error::OutOfMemory)?;
    if len>isize::MAX as usize || unsafe {layer.data_ptr()}.is_null() || layer.pixel_format()?!=ae::PixelFormat::Argb32 {return Err(ae::Error::BadCallbackParameter);}
    Ok(())
}

fn layer_to_flat(layer: &ae::Layer) -> Result<(Vec<u8>, usize, usize),ae::Error> {
    validate_world(layer)?;
    let w = layer.width() as usize;
    let h = layer.height() as usize;
    if w==0 || h==0 {return Ok((Vec::new(),w,h));}
    let stride = layer.buffer_stride();
    let count=w.checked_mul(h).and_then(|v|v.checked_mul(4)).ok_or(ae::Error::OutOfMemory)?;
    let mut flat=Vec::new();
    flat.try_reserve_exact(count).map_err(|_|ae::Error::OutOfMemory)?;
    flat.resize(count,0);
    copy_rows(layer.buffer(),&mut flat,w,h,stride,w*4,layer.row_bytes()<0,false)?;
    Ok((flat, w, h))
}

fn flat_to_layer(flat: &[u8], layer: &mut ae::Layer, w: usize, h: usize) -> Result<(),ae::Error> {
    if (w,h)!=(layer.width(),layer.height()) {return Err(ae::Error::BadCallbackParameter);}
    validate_world(layer)?;
    let count=w.checked_mul(h).and_then(|v|v.checked_mul(4)).ok_or(ae::Error::OutOfMemory)?;
    if flat.len()!=count {return Err(ae::Error::BadCallbackParameter);}
    if w==0 || h==0 {return Ok(());}
    let stride = layer.buffer_stride();
    let reverse=layer.row_bytes()<0;
    copy_rows(flat,layer.buffer_mut(),w,h,w*4,stride,false,reverse)
}

fn render_tui_cpu(src: &[u8], w: usize, h: usize, ep: &TuiParams) -> Vec<u8> {
    if w == 0 || h == 0 {
        return src.to_vec();
    }

    let mut out = vec![0u8; w * h * 4];
    let threads = num_cpus::get()
        .max(1)
        .min((h + ep.cell_h - 1) / ep.cell_h)
        .max(1);
    let cell_rows = (h + ep.cell_h - 1) / ep.cell_h;
    let rows_per_thread = (cell_rows + threads - 1) / threads;

    thread::scope(|scope| {
        for (chunk_index, out_chunk) in out
            .chunks_mut(rows_per_thread * ep.cell_h * w * 4)
            .enumerate()
        {
            let y_start = chunk_index * rows_per_thread * ep.cell_h;
            let rows = (out_chunk.len() / (w * 4)).min(h.saturating_sub(y_start));
            scope.spawn(move || {
                // Scoped workers cannot outlive this immutable source slice.
                render_rows(out_chunk, src, src, w, h, y_start, rows, ep);
            });
        }
    });

    out
}

fn render_tui(src: &[u8], w: usize, h: usize, ep: &TuiParams) -> Result<Vec<u8>,ae::Error> {
    let count=w.checked_mul(h).and_then(|v|v.checked_mul(4)).ok_or(ae::Error::OutOfMemory)?;
    if src.len()!=count {return Err(ae::Error::BadCallbackParameter);}
    if w==0 || h==0 {return Ok(Vec::new());}
    let ep = apply_preset_defaults(ep);
    if (ep.render_scale - 1.0).abs() > 0.01 {
        let scaled_w = ((w as f32 * ep.render_scale).round() as usize).clamp(1, 8192);
        let scaled_h = ((h as f32 * ep.render_scale).round() as usize).clamp(1, 8192);
        if scaled_w != w || scaled_h != h {
            let scaled_src = resize_nearest(src, w, h, scaled_w, scaled_h)?;
            let mut scaled_params = ep;
            scaled_params.render_scale = 1.0;
            let scaled_out = render_tui_native(&scaled_src, scaled_w, scaled_h, &scaled_params);
            return resize_nearest(&scaled_out, scaled_w, scaled_h, w, h);
        }
    }

    Ok(render_tui_native(src, w, h, &ep))
}

fn render_tui_native(src: &[u8], w: usize, h: usize, ep: &TuiParams) -> Vec<u8> {
    let ep = effective_grid_params(w, h, ep);
    if std::env::var_os("TUIIMAGE_ENABLE_GPU").is_some() {
        let renderer = GPU_RENDERER.get_or_init(|| gpu::GpuProcessor::new().ok());
        if let Some(renderer) = renderer {
            if let Some(out) = renderer.process(src, w, h, &ep) {
                return out;
            }
        }
    }
    render_tui_cpu(src, w, h, &ep)
}

fn effective_grid_params(w: usize, h: usize, ep: &TuiParams) -> TuiParams {
    let mut out = *ep;
    if ep.columns > 0 {
        out.cell_w = w.div_ceil(ep.columns).clamp(1, 4096);
    }
    if ep.rows > 0 {
        out.cell_h = h.div_ceil(ep.rows).clamp(1, 4096);
    }
    out
}

fn resize_nearest(src: &[u8], src_w: usize, src_h: usize, dst_w: usize, dst_h: usize) -> Result<Vec<u8>,ae::Error> {
    let source_len=src_w.checked_mul(src_h).and_then(|v|v.checked_mul(4)).ok_or(ae::Error::OutOfMemory)?;
    if src.len()!=source_len {return Err(ae::Error::BadCallbackParameter);}
    let count=dst_w.checked_mul(dst_h).and_then(|v|v.checked_mul(4)).ok_or(ae::Error::OutOfMemory)?;
    let mut dst=Vec::new();
    dst.try_reserve_exact(count).map_err(|_|ae::Error::OutOfMemory)?;
    dst.resize(count,0);
    if src_w == 0 || src_h == 0 || dst_w == 0 || dst_h == 0 {
        return Ok(dst);
    }

    for y in 0..dst_h {
        let sy = ((y as u128 * src_h as u128) / dst_h as u128) as usize;
        for x in 0..dst_w {
            let sx = ((x as u128 * src_w as u128) / dst_w as u128) as usize;
            let src_off = (sy * src_w + sx) * 4;
            let dst_off = (y * dst_w + x) * 4;
            if src_off + 3 < src.len() {
                dst[dst_off..dst_off + 4].copy_from_slice(&src[src_off..src_off + 4]);
            }
        }
    }
    Ok(dst)
}

fn render_rows(
    out: &mut [u8],
    src: &[u8],
    color_src: &[u8],
    w: usize,
    h: usize,
    y_start: usize,
    rows: usize,
    ep: &TuiParams,
) {
    let y_end = (y_start + rows).min(h);
    let mut cell_y = y_start;
    while cell_y < y_end {
        let cell_h = ep.cell_h.min(h - cell_y);
        let mut cell_x = 0usize;
        while cell_x < w {
            let cell_w = ep.cell_w.min(w - cell_x);
            let sample = sample_cell(src, color_src, w, h, cell_x, cell_y, cell_w, cell_h, ep);
            draw_cell(
                out, src, color_src, w, y_start, cell_x, cell_y, cell_w, cell_h, sample, ep,
            );
            cell_x += ep.cell_w;
        }
        cell_y += ep.cell_h;
    }
}

#[derive(Clone, Copy)]
struct CellSample {
    luma: f32,
    color: [f32; 3],
}

fn sample_cell(
    src: &[u8],
    color_src: &[u8],
    w: usize,
    h: usize,
    x0: usize,
    y0: usize,
    cw: usize,
    ch: usize,
    ep: &TuiParams,
) -> CellSample {
    let mut sum_luma = 0.0f32;
    let mut sum_rgb = [0.0f32; 3];
    let mut count = 0.0f32;

    for y in y0..(y0 + ch).min(h) {
        for x in x0..(x0 + cw).min(w) {
            let off = (y * w + x) * 4;
            let r = src[off + 1] as f32 / 255.0;
            let g = src[off + 2] as f32 / 255.0;
            let b = src[off + 3] as f32 / 255.0;
            let color_off = (y * w + x) * 4;
            let cr = color_src[color_off + 1] as f32 / 255.0;
            let cg = color_src[color_off + 2] as f32 / 255.0;
            let cb = color_src[color_off + 3] as f32 / 255.0;
            sum_luma += 0.2126 * r + 0.7152 * g + 0.0722 * b;
            sum_rgb[0] += cr;
            sum_rgb[1] += cg;
            sum_rgb[2] += cb;
            count += 1.0;
        }
    }

    let inv = if count > 0.0 { 1.0 / count } else { 0.0 };
    let edge = if ep.edge_boost > 0.0 {
        cell_edge_score(src, w, h, x0, y0, cw, ch)
    } else {
        0.0
    };
    let mut y = ((sum_luma * inv + edge * ep.edge_boost) * ep.contrast)
        .clamp(0.0, 1.0)
        .powf(ep.gamma.max(0.001));
    if ep.invert {
        y = 1.0 - y;
    }

    CellSample {
        luma: y,
        color: [sum_rgb[0] * inv, sum_rgb[1] * inv, sum_rgb[2] * inv],
    }
}

fn cell_edge_score(
    src: &[u8],
    w: usize,
    h: usize,
    x0: usize,
    y0: usize,
    cw: usize,
    ch: usize,
) -> f32 {
    if w < 3 || h < 3 {
        return 0.0;
    }

    let x1 = (x0 + cw).min(w - 1);
    let y1 = (y0 + ch).min(h - 1);
    let mut sum = 0.0f32;
    let mut count = 0.0f32;

    for y in y0.max(1)..y1 {
        for x in x0.max(1)..x1 {
            let left = pixel_luma(src, w, x - 1, y);
            let right = pixel_luma(src, w, x + 1, y);
            let up = pixel_luma(src, w, x, y - 1);
            let down = pixel_luma(src, w, x, y + 1);
            sum += ((right - left).abs() + (down - up).abs()) * 0.5;
            count += 1.0;
        }
    }

    if count > 0.0 {
        (sum / count).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn pixel_luma(src: &[u8], w: usize, x: usize, y: usize) -> f32 {
    let off = (y * w + x) * 4;
    let r = src[off + 1] as f32 / 255.0;
    let g = src[off + 2] as f32 / 255.0;
    let b = src[off + 3] as f32 / 255.0;
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

fn draw_cell(
    out: &mut [u8],
    src: &[u8],
    color_src: &[u8],
    w: usize,
    y_base: usize,
    x0: usize,
    y0: usize,
    cw: usize,
    ch: usize,
    sample: CellSample,
    ep: &TuiParams,
) {
    let density_luma = if ep.use_source_luma { sample.luma } else { 1.0 };
    let levels = preset_step_count(ep.preset) as f32;
    let level = (density_luma * levels).round() as usize;
    let coverage = level as f32 / levels;
    let fg = choose_foreground(sample, ep);
    let (glyph_x, glyph_y, glyph_w, glyph_h) = glyph_rect(cw, ch, ep);

    for yy in 0..ch {
        for xx in 0..cw {
            let fill = if xx >= glyph_x
                && xx < glyph_x + glyph_w
                && yy >= glyph_y
                && yy < glyph_y + glyph_h
            {
                let gx = xx - glyph_x;
                let gy = yy - glyph_y;
                match ep.preset {
                    PRESET_ASCII => ascii_like_coverage(level.min(9), gx, gy, glyph_w, glyph_h),
                    PRESET_BRAILLE => braille_dot_coverage(level.min(8), gx, gy, glyph_w, glyph_h),
                    _ => (glyph_h - gy) as f32 / glyph_h as f32 <= coverage,
                }
            } else {
                false
            };
            let x = x0 + xx;
            let y = y0 + yy;
            let src_off = (y * w + x) * 4;
            let dst_off = ((y - y_base) * w + x) * 4;
            if dst_off + 3 >= out.len() || src_off + 3 >= src.len() {
                continue;
            }

            let source_color = [
                color_src[src_off + 1] as f32 / 255.0,
                color_src[src_off + 2] as f32 / 255.0,
                color_src[src_off + 3] as f32 / 255.0,
            ];
            let base = if fill {
                if ep.preserve_source_color {
                    source_color
                } else {
                    fg
                }
            } else {
                ep.background
            };
            let mixed = mix_rgb(base, source_color, ep.source_mix);

            // Adjustment layers often need a fully visible generated frame.
            // Preserving source alpha can make the effect appear to do nothing.
            out[dst_off] = 255;
            out[dst_off + 1] = to_u8(mixed[0]);
            out[dst_off + 2] = to_u8(mixed[1]);
            out[dst_off + 3] = to_u8(mixed[2]);
        }
    }
}

fn glyph_rect(cw: usize, ch: usize, ep: &TuiParams) -> (usize, usize, usize, usize) {
    let scale_x = ep.glyph_scale_x.clamp(0.1, 1.0);
    let scale_y = if ep.uniform_scale {
        scale_x
    } else {
        ep.glyph_scale_y.clamp(0.1, 1.0)
    };
    let sx = (scale_x * (1.0 - ep.column_gap.clamp(0.0, 0.8))).clamp(0.05, 1.0);
    let sy = (scale_y * (1.0 - ep.row_gap.clamp(0.0, 0.8))).clamp(0.05, 1.0);
    let gw = ((cw as f32 * sx).round() as usize).max(1).min(cw);
    let gh = ((ch as f32 * sy).round() as usize).max(1).min(ch);
    ((cw - gw) / 2, (ch - gh) / 2, gw, gh)
}

fn ascii_like_coverage(level: usize, x: usize, y: usize, w: usize, h: usize) -> bool {
    if level == 0 {
        return false;
    }
    let cx = w / 2;
    let cy = h / 2;
    let qx1 = (w / 3).max(1);
    let qx2 = ((w * 2) / 3).max(1);
    let qy1 = (h / 3).max(1);
    let qy2 = ((h * 2) / 3).max(1);
    let t = (w.min(h) / 7).max(1);
    let near = |v: usize, target: usize| v.abs_diff(target) <= t;

    match level {
        1 => near(x, cx) && near(y, qy2),
        2 => near(x, cx) && (near(y, qy1) || near(y, qy2)),
        3 => near(y, cy) && x > t && x + t < w,
        4 => (near(y, qy1) || near(y, qy2)) && x > t && x + t < w,
        5 => near(y, cy) || near(x, cx),
        6 => {
            let dx = x as f32 / (w.saturating_sub(1).max(1) as f32);
            let dy = y as f32 / (h.saturating_sub(1).max(1) as f32);
            near(y, cy) || near(x, cx) || (dx - dy).abs() < 0.18 || (dx + dy - 1.0).abs() < 0.18
        }
        7 => near(x, qx1) || near(x, qx2) || near(y, qy1) || near(y, qy2),
        8 => (x + y) % 3 != 0,
        _ => true,
    }
}

fn braille_dot_coverage(level: usize, x: usize, y: usize, w: usize, h: usize) -> bool {
    if level == 0 {
        return false;
    }
    let col = ((x * 2) / w.max(1)).min(1);
    let row = ((y * 4) / h.max(1)).min(3);
    if row * 2 + col >= level {
        return false;
    }

    let cx = (col as f32 + 0.5) * w as f32 * 0.5;
    let cy = (row as f32 + 0.5) * h as f32 * 0.25;
    let rx = (w as f32 * 0.18).max(1.0);
    let ry = (h as f32 * 0.10).max(1.0);
    let dx = (x as f32 + 0.5 - cx) / rx;
    let dy = (y as f32 + 0.5 - cy) / ry;
    dx * dx + dy * dy <= 1.0
}

fn choose_foreground(sample: CellSample, ep: &TuiParams) -> [f32; 3] {
    match ep.color_mode {
        COLOR_MONO => ep.foreground,
        COLOR_SOURCE => sample.color,
        COLOR_GRADIENT => mix_rgb(ep.background, ep.foreground, sample.luma),
        _ => ep.foreground,
    }
}

fn pixel_to_rgb(pixel: ae::Pixel8) -> [f32; 3] {
    [
        pixel.red as f32 / 255.0,
        pixel.green as f32 / 255.0,
        pixel.blue as f32 / 255.0,
    ]
}

fn rgb_to_pixel(rgb: [f32; 3]) -> ae::Pixel8 {
    ae::Pixel8 {
        alpha: 255,
        red: to_u8(rgb[0]),
        green: to_u8(rgb[1]),
        blue: to_u8(rgb[2]),
    }
}

fn mix_rgb(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

fn to_u8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8
}
