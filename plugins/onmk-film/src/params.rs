//! Runtime parameter bag shared by CPU / GPU paths.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u32)]
pub enum InputColorspace {
    Rec709 = 0,
    Srgb = 1,
    Linear = 2,
    Bypass = 3,
    SLog3 = 4,
    LogC3 = 5,
    VLog = 6,
    CLog3 = 7,
}

impl InputColorspace {
    pub fn from_popup(v: i32) -> Self {
        match v {
            2 => Self::Srgb,
            3 => Self::Linear,
            4 => Self::Bypass,
            5 => Self::SLog3,
            6 => Self::LogC3,
            7 => Self::VLog,
            8 => Self::CLog3,
            _ => Self::Rec709,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u32)]
pub enum FilmStock {
    DaylightFine = 0,
    Daylight250 = 1,
    Tungsten200 = 2,
    Tungsten500 = 3,
    Mono250 = 4,
    Portrait400 = 5,
    Vivid100 = 6,
    Amber200 = 7,
    Pastel400 = 8,
    Neon800 = 9,
    Chrome50 = 10,
    Mono400 = 11,
    PrintStock = 12,
}

impl FilmStock {
    pub fn from_popup(v: i32) -> Self {
        match v.saturating_sub(1).clamp(0, 12) {
            0 => Self::DaylightFine,
            1 => Self::Daylight250,
            2 => Self::Tungsten200,
            3 => Self::Tungsten500,
            4 => Self::Mono250,
            5 => Self::Portrait400,
            6 => Self::Vivid100,
            7 => Self::Amber200,
            8 => Self::Pastel400,
            9 => Self::Neon800,
            10 => Self::Chrome50,
            11 => Self::Mono400,
            _ => Self::PrintStock,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::DaylightFine => "Daylight 50",
            Self::Daylight250 => "Daylight 250",
            Self::Tungsten200 => "Tungsten 200",
            Self::Tungsten500 => "Tungsten 500",
            Self::Mono250 => "Mono 250",
            Self::Portrait400 => "Portrait 400",
            Self::Vivid100 => "Vivid 100",
            Self::Amber200 => "Amber 200",
            Self::Pastel400 => "Pastel 400",
            Self::Neon800 => "Neon 800",
            Self::Chrome50 => "Chrome 50",
            Self::Mono400 => "Mono 400",
            Self::PrintStock => "Print",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u32)]
pub enum LookPreset {
    Neutral = 0,
    Classic = 1,
    TealAmber = 2,
    PrintMaster = 3,
    NeonNight = 4,
    FadedPastel = 5,
    Custom = 6,
}

impl LookPreset {
    pub fn from_popup(v: i32) -> Self {
        match v.saturating_sub(1).clamp(0, 6) {
            0 => Self::Neutral,
            1 => Self::Classic,
            2 => Self::TealAmber,
            3 => Self::PrintMaster,
            4 => Self::NeonNight,
            5 => Self::FadedPastel,
            _ => Self::Custom,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u32)]
pub enum Gauge {
    G65mm = 0,
    G35mm = 1,
    G16mm = 2,
    G8mm = 3,
}

impl Gauge {
    pub fn from_popup(v: i32) -> Self {
        match v {
            1 => Self::G65mm,
            3 => Self::G16mm,
            4 => Self::G8mm,
            _ => Self::G35mm,
        }
    }

    /// Relative grain scale vs 35mm.
    pub fn scale(self) -> f32 {
        match self {
            Self::G65mm => 0.55,
            Self::G35mm => 1.0,
            Self::G16mm => 1.85,
            Self::G8mm => 3.2,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FilmParams {
    pub bypass: bool,
    pub input_cs: InputColorspace,
    pub stock: FilmStock,
    pub look: LookPreset,

    pub exposure: f32,
    pub character: f32,
    pub print_contrast: f32,
    pub shoulder: f32,
    pub saturation: f32,
    pub signature: f32,
    pub skin_lift: f32,
    pub skin_sat: f32,
    pub skin_hue: f32,

    pub grain_enable: bool,
    pub gauge: Gauge,
    pub grain_amount: f32,
    pub grain_clump: f32,
    pub grain_size: f32,
    pub grain_shadows: f32,
    pub grain_mids: f32,
    pub grain_highlights: f32,
    pub print_grain: bool,
    pub print_grain_amount: f32,
    pub print_grain_size: f32,

    pub bloom_enable: bool,
    pub bloom_strength: f32,
    pub bloom_radius: f32,
    pub bloom_threshold: f32,
    pub diffusion: f32,

    pub ca_enable: bool,
    pub ca_amount: f32,
    pub ca_falloff: f32,

    pub vignette_enable: bool,
    pub vignette_amount: f32,
    pub vignette_radius: f32,
    pub vignette_softness: f32,

    pub leak_amount: f32,
    pub leak_angle: f32,
    pub leak_softness: f32,
    pub leak_flicker: f32,
    pub leak_color: [f32; 3],

    pub halation_enable: bool,
    pub halation_strength: f32,
    pub halation_radius: f32,
    pub halation_threshold: f32,
    pub halation_color: [f32; 3],
    pub halation_flicker: f32,

    pub bleed_enable: bool,
    pub irradiation: f32,
    pub irradiation_radius: f32,
    pub dye_cloud: f32,
    pub dye_radius: f32,
    pub interlayer: f32,

    pub mtf_enable: bool,
    pub mtf_softness: f32,
    pub acutance: f32,

    pub weave_enable: bool,
    pub weave_amount: f32,

    pub crossover_enable: bool,
    pub crossover: f32,
    pub crossover_axis: f32,

    pub mottle_enable: bool,
    pub mottle_amount: f32,
    pub mottle_size: f32,
    pub mottle_static: bool,

    pub quality: f32,
    pub seed: u32,
    pub frame: u32,
    pub use_gpu: bool,
}

impl Default for FilmParams {
    fn default() -> Self {
        Self {
            bypass: false,
            input_cs: InputColorspace::Rec709,
            stock: FilmStock::Daylight250,
            look: LookPreset::Neutral,
            exposure: 0.0,
            character: 1.0,
            print_contrast: 1.0,
            shoulder: 0.0,
            saturation: 1.0,
            signature: 0.0,
            skin_lift: 1.0,
            skin_sat: 1.0,
            skin_hue: 0.0,
            grain_enable: true,
            gauge: Gauge::G35mm,
            grain_amount: 0.5,
            grain_clump: 0.5,
            grain_size: 1.0,
            grain_shadows: 1.0,
            grain_mids: 1.0,
            grain_highlights: 1.0,
            print_grain: false,
            print_grain_amount: 0.35,
            print_grain_size: 1.0,
            bloom_enable: false,
            bloom_strength: 0.3,
            bloom_radius: 0.03,
            bloom_threshold: 0.6,
            diffusion: 0.0,
            ca_enable: false,
            ca_amount: 0.3,
            ca_falloff: 1.0,
            vignette_enable: false,
            vignette_amount: 0.0,
            vignette_radius: 0.55,
            vignette_softness: 0.6,
            leak_amount: 0.0,
            leak_angle: 0.0,
            leak_softness: 0.55,
            leak_flicker: 0.0,
            leak_color: [1.0, 0.42, 0.16],
            halation_enable: true,
            halation_strength: 0.5,
            halation_radius: 0.01,
            halation_threshold: 1.0,
            halation_color: [1.0, 0.25, 0.08],
            halation_flicker: 0.0,
            bleed_enable: true,
            irradiation: 0.35,
            irradiation_radius: 0.004,
            dye_cloud: 0.35,
            dye_radius: 0.003,
            interlayer: 0.4,
            mtf_enable: true,
            mtf_softness: 0.35,
            acutance: 0.5,
            weave_enable: false,
            weave_amount: 0.5,
            crossover_enable: false,
            crossover: 0.0,
            crossover_axis: 0.0,
            mottle_enable: false,
            mottle_amount: 0.0,
            mottle_size: 1.0,
            mottle_static: false,
            quality: 1.0,
            seed: 0,
            frame: 0,
            use_gpu: true,
        }
    }
}

impl FilmParams {
    /// Apply a look preset by overwriting relevant fields (Custom = no-op).
    pub fn apply_look(&mut self) {
        match self.look {
            LookPreset::Custom | LookPreset::Neutral => {}
            LookPreset::Classic => {
                self.print_contrast = 1.12;
                self.shoulder = 0.18;
                self.saturation = 0.92;
                self.character = 1.15;
                self.signature = 0.25;
                self.halation_enable = true;
                self.halation_strength = 0.55;
                self.grain_amount = 0.55;
                self.crossover = 0.08;
            }
            LookPreset::TealAmber => {
                self.print_contrast = 1.18;
                self.shoulder = 0.22;
                self.saturation = 1.05;
                self.character = 1.35;
                self.signature = 0.55;
                self.skin_hue = 6.0;
                self.halation_strength = 0.4;
                self.grain_amount = 0.45;
            }
            LookPreset::PrintMaster => {
                self.print_contrast = 1.28;
                self.shoulder = 0.12;
                self.saturation = 1.0;
                self.character = 1.0;
                self.mtf_softness = 0.25;
                self.acutance = 0.7;
                self.grain_amount = 0.35;
            }
            LookPreset::NeonNight => {
                self.exposure = 0.15;
                self.print_contrast = 1.22;
                self.shoulder = 0.35;
                self.saturation = 1.25;
                self.bloom_enable = true;
                self.bloom_strength = 0.45;
                self.halation_strength = 0.85;
                self.halation_color = [1.0, 0.15, 0.35];
                self.grain_amount = 0.7;
                self.stock = FilmStock::Neon800;
            }
            LookPreset::FadedPastel => {
                self.print_contrast = 0.82;
                self.shoulder = 0.4;
                self.saturation = 0.72;
                self.signature = 0.15;
                self.crossover_enable = true;
                self.crossover = -0.25;
                self.crossover_axis = 20.0;
                self.grain_amount = 0.4;
                self.stock = FilmStock::Pastel400;
            }
        }
    }
}
