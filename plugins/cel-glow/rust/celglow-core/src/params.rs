use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Params {
    pub source: SourceParams,
    pub rings: RingsParams,
    pub wobble: WobbleParams,
    pub texture: TextureParams,
    pub unevenness: UnevennessParams,
    pub output: OutputParams,
    #[serde(skip)]
    pub downsample: (f32, f32),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SourceParams {
    pub channel: SourceChannel,
    pub gain: f32,
    pub gamma: f32,
    pub spread: f32,
    pub field_gamma: f32,
    /// Input gate before field blur. 0% preserves the original behaviour.
    pub threshold: f32,
    /// Width of the soft transition above `threshold`.
    pub threshold_softness: f32,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceChannel {
    Alpha,
    Luma,
    LumaXAlpha,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct RingsParams {
    pub ring_count: u8,
    pub distribution: f32,
    pub line_width: f32,
    pub line_hardness: f32,
    pub core_level: f32,
    pub core_softness: f32,
    pub outer_falloff: f32,
    pub brightness_variation: f32,
    pub color_scramble: f32,
    pub seed: u16,
    pub phase: f32,
    pub show_outermost: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct WobbleParams {
    pub amount: f32,
    pub scale: f32,
    pub complexity: u8,
    pub evolution: f32,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TextureParams {
    pub edge_type: RoughenEdgeType,
    pub border: f32,
    pub influence: f32,
    pub scale: f32,
    pub sharpness: f32,
    pub complexity: u8,
    pub evolution: f32,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UnevennessParams {
    pub amount: f32,
    pub scale: f32,
    pub evolution: f32,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct OutputParams {
    pub color_mode: ColorMode,
    pub fill_color: [f32; 3],
    pub inner_color: [f32; 3],
    pub outer_color: [f32; 3],
    pub rainbow_cycles: f32,
    pub rainbow_saturation: f32,
    pub opacity: f32,
    pub blend_mode: BlendMode,
    pub placement: Placement,
    pub write_glow_alpha: bool,
    pub quality: Quality,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColorMode {
    Fill,
    InnerOuter,
    Rainbow,
    SourceColor,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlendMode {
    Add,
    Screen,
    Normal,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Placement {
    BehindSource,
    InFront,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Quality {
    Draft,
    Normal,
    Best,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoughenEdgeType {
    Cut,
    Roughen,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            source: SourceParams::default(),
            rings: RingsParams::default(),
            wobble: WobbleParams::default(),
            texture: TextureParams::default(),
            unevenness: UnevennessParams::default(),
            output: OutputParams::default(),
            downsample: (1.0, 1.0),
        }
    }
}
impl Default for SourceParams {
    fn default() -> Self {
        Self {
            channel: SourceChannel::LumaXAlpha,
            gain: 100.0,
            gamma: 1.0,
            spread: 260.0,
            field_gamma: 1.0,
            threshold: 0.0,
            threshold_softness: 0.0,
        }
    }
}
impl Default for RingsParams {
    fn default() -> Self {
        Self {
            ring_count: 12,
            distribution: 1.0,
            line_width: 30.0,
            line_hardness: 80.0,
            core_level: 70.0,
            core_softness: 10.0,
            // Cel-style bands are solid by default: no radial brightness fade
            // and no per-band colour/brightness scrambling.
            outer_falloff: 0.0,
            brightness_variation: 0.0,
            color_scramble: 0.0,
            seed: 1,
            phase: 0.0,
            show_outermost: false,
        }
    }
}
impl Default for WobbleParams {
    fn default() -> Self {
        Self {
            amount: 3.0,
            scale: 24.0,
            complexity: 1,
            evolution: 0.0,
        }
    }
}
impl Default for TextureParams {
    fn default() -> Self {
        Self {
            edge_type: RoughenEdgeType::Cut,
            border: 4.0,
            influence: 3.5,
            scale: 3.5,
            sharpness: 20.0,
            complexity: 3,
            evolution: 0.0,
        }
    }
}
impl Default for UnevennessParams {
    fn default() -> Self {
        Self {
            // Keep the cel bands opaque by default. Unevenness remains an
            // optional atmospheric opacity modulation rather than a built-in
            // radial fade.
            amount: 28.0,
            scale: 430.0,
            evolution: 0.0,
        }
    }
}
impl Default for OutputParams {
    fn default() -> Self {
        Self {
            color_mode: ColorMode::Fill,
            fill_color: [0.20, 0.36, 0.62],
            inner_color: [1.0; 3],
            outer_color: [0.5, 0.8, 1.0],
            rainbow_cycles: 1.0,
            rainbow_saturation: 70.0,
            opacity: 75.0,
            blend_mode: BlendMode::Add,
            placement: Placement::BehindSource,
            // The visible glow is part of the layer by default. Turn this
            // off when an AE comp needs to retain only the source alpha.
            write_glow_alpha: true,
            quality: Quality::Normal,
        }
    }
}
