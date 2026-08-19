//! Colorspace helpers and stock response curves.

use crate::params::{FilmStock, InputColorspace};

#[inline]
pub fn srgb_eotf(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

#[inline]
pub fn srgb_oetf(c: f32) -> f32 {
    let c = c.max(0.0);
    if c <= 0.0031308 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

#[inline]
pub fn rec709_eotf(c: f32) -> f32 {
    if c < 0.081 {
        c / 4.5
    } else {
        ((c + 0.099) / 1.099).powf(1.0 / 0.45)
    }
}

#[inline]
pub fn rec709_oetf(c: f32) -> f32 {
    let c = c.max(0.0);
    if c < 0.018 {
        c * 4.5
    } else {
        1.099 * c.powf(0.45) - 0.099
    }
}

/// Decode camera log / display encodings into scene-linear-ish working space.
pub fn decode_input(rgb: [f32; 3], cs: InputColorspace) -> [f32; 3] {
    match cs {
        InputColorspace::Bypass => rgb,
        InputColorspace::Rec709 => rgb.map(rec709_eotf),
        InputColorspace::Srgb => rgb.map(srgb_eotf),
        InputColorspace::Linear => rgb,
        InputColorspace::SLog3 => rgb.map(slog3_to_linear),
        InputColorspace::LogC3 => rgb.map(logc3_to_linear),
        InputColorspace::VLog => rgb.map(vlog_to_linear),
        InputColorspace::CLog3 => rgb.map(clog3_to_linear),
    }
}

pub fn encode_output(rgb: [f32; 3], cs: InputColorspace) -> [f32; 3] {
    match cs {
        InputColorspace::Bypass | InputColorspace::Linear => rgb,
        // Log inputs are returned as display-referred Rec.709.
        InputColorspace::SLog3
        | InputColorspace::LogC3
        | InputColorspace::VLog
        | InputColorspace::CLog3
        | InputColorspace::Rec709 => rgb.map(rec709_oetf),
        InputColorspace::Srgb => rgb.map(srgb_oetf),
    }
}

#[inline]
fn slog3_to_linear(x: f32) -> f32 {
    // Sony S-Log3 (public curve).
    if x >= 171.2102946929 / 1023.0 {
        10.0f32.powf((x * 1023.0 - 420.0) / 261.5) * 0.18
    } else {
        (x * 1023.0 - 95.0) * 0.01125000 / (171.2102946929 - 95.0)
    }
}

#[inline]
fn logc3_to_linear(x: f32) -> f32 {
    // ARRI LogC3 EI800 cut.
    const CUT: f32 = 0.010591;
    const A: f32 = 5.555556;
    const B: f32 = 0.052272;
    const C: f32 = 0.247190;
    const D: f32 = 0.385537;
    const E: f32 = 5.367655;
    const F: f32 = 0.092809;
    if x > E * CUT + F {
        ((10.0f32.powf((x - D) / C)) - B) / A
    } else {
        (x - F) / E
    }
}

#[inline]
fn vlog_to_linear(x: f32) -> f32 {
    const B: f32 = 0.00873;
    const C: f32 = 0.241514;
    const D: f32 = 0.598206;
    if x < 0.181 {
        (x - 0.125) / 5.6
    } else {
        10.0f32.powf((x - D) / C) - B
    }
}

#[inline]
fn clog3_to_linear(x: f32) -> f32 {
    // Canon C-Log3 approximate.
    if x < 0.09746547 {
        -(10.0f32.powf((0.12783901 - x) / 0.36726845) - 1.0) / 14.98325
    } else if x <= 0.15277891 {
        (x - 0.073059361) / 2.3069815
    } else {
        (10.0f32.powf((x - 0.12240537) / 0.36726845) - 1.0) / 14.98325
    }
}

#[derive(Clone, Copy)]
pub struct StockProfile {
    pub gamma_r: f32,
    pub gamma_g: f32,
    pub gamma_b: f32,
    pub toe: f32,
    pub shoulder: f32,
    pub sat_bias: f32,
    /// Per-channel density offsets (signature / stock cast).
    pub cast: [f32; 3],
    pub grain_base: f32,
    pub mono: bool,
}

pub fn stock_profile(stock: FilmStock) -> StockProfile {
    match stock {
        FilmStock::DaylightFine => StockProfile {
            gamma_r: 0.92,
            gamma_g: 0.95,
            gamma_b: 0.98,
            toe: 0.08,
            shoulder: 0.18,
            sat_bias: 0.95,
            cast: [0.02, 0.0, -0.01],
            grain_base: 0.55,
            mono: false,
        },
        FilmStock::Daylight250 => StockProfile {
            gamma_r: 0.95,
            gamma_g: 0.97,
            gamma_b: 1.0,
            toe: 0.1,
            shoulder: 0.22,
            sat_bias: 1.0,
            cast: [0.015, 0.0, -0.02],
            grain_base: 0.85,
            mono: false,
        },
        FilmStock::Tungsten200 => StockProfile {
            gamma_r: 0.9,
            gamma_g: 0.96,
            gamma_b: 1.05,
            toe: 0.12,
            shoulder: 0.2,
            sat_bias: 0.98,
            cast: [-0.02, 0.0, 0.04],
            grain_base: 0.9,
            mono: false,
        },
        FilmStock::Tungsten500 => StockProfile {
            gamma_r: 0.88,
            gamma_g: 0.95,
            gamma_b: 1.08,
            toe: 0.14,
            shoulder: 0.25,
            sat_bias: 0.92,
            cast: [-0.03, 0.0, 0.05],
            grain_base: 1.35,
            mono: false,
        },
        FilmStock::Mono250 => StockProfile {
            gamma_r: 1.0,
            gamma_g: 1.0,
            gamma_b: 1.0,
            toe: 0.12,
            shoulder: 0.2,
            sat_bias: 0.0,
            cast: [0.0; 3],
            grain_base: 0.95,
            mono: true,
        },
        FilmStock::Portrait400 => StockProfile {
            gamma_r: 0.93,
            gamma_g: 0.96,
            gamma_b: 0.99,
            toe: 0.14,
            shoulder: 0.28,
            sat_bias: 0.88,
            cast: [0.04, 0.01, -0.03],
            grain_base: 1.05,
            mono: false,
        },
        FilmStock::Vivid100 => StockProfile {
            gamma_r: 1.05,
            gamma_g: 1.0,
            gamma_b: 1.08,
            toe: 0.06,
            shoulder: 0.15,
            sat_bias: 1.25,
            cast: [0.03, -0.01, 0.02],
            grain_base: 0.5,
            mono: false,
        },
        FilmStock::Amber200 => StockProfile {
            gamma_r: 1.0,
            gamma_g: 0.95,
            gamma_b: 0.9,
            toe: 0.1,
            shoulder: 0.24,
            sat_bias: 1.05,
            cast: [0.06, 0.02, -0.05],
            grain_base: 0.9,
            mono: false,
        },
        FilmStock::Pastel400 => StockProfile {
            gamma_r: 0.88,
            gamma_g: 0.9,
            gamma_b: 0.92,
            toe: 0.16,
            shoulder: 0.35,
            sat_bias: 0.7,
            cast: [0.02, 0.01, 0.03],
            grain_base: 1.1,
            mono: false,
        },
        FilmStock::Neon800 => StockProfile {
            gamma_r: 0.95,
            gamma_g: 0.9,
            gamma_b: 1.15,
            toe: 0.08,
            shoulder: 0.3,
            sat_bias: 1.35,
            cast: [-0.02, -0.04, 0.08],
            grain_base: 1.6,
            mono: false,
        },
        FilmStock::Chrome50 => StockProfile {
            gamma_r: 1.1,
            gamma_g: 1.05,
            gamma_b: 1.12,
            toe: 0.04,
            shoulder: 0.1,
            sat_bias: 1.3,
            cast: [0.01, 0.0, 0.02],
            grain_base: 0.4,
            mono: false,
        },
        FilmStock::Mono400 => StockProfile {
            gamma_r: 1.05,
            gamma_g: 1.05,
            gamma_b: 1.05,
            toe: 0.1,
            shoulder: 0.18,
            sat_bias: 0.0,
            cast: [0.0; 3],
            grain_base: 1.2,
            mono: true,
        },
        FilmStock::PrintStock => StockProfile {
            gamma_r: 1.15,
            gamma_g: 1.12,
            gamma_b: 1.1,
            toe: 0.05,
            shoulder: 0.12,
            sat_bias: 1.05,
            cast: [0.01, 0.0, -0.01],
            grain_base: 0.35,
            mono: false,
        },
    }
}

/// Soft shoulder: compress highlights above mid.
#[inline]
pub fn apply_shoulder(v: f32, amount: f32) -> f32 {
    if amount <= 1.0e-5 || v <= 0.5 {
        return v;
    }
    let t = ((v - 0.5) / 0.5).clamp(0.0, 1.0);
    let soft = 0.5 + 0.5 * (1.0 - (1.0 - t).powf(1.0 + amount * 2.5));
    v * (1.0 - amount) + soft * amount
}

/// Negative density response then print back to positive display-linear.
pub fn film_response(rgb: [f32; 3], stock: FilmStock, character: f32, shoulder: f32) -> [f32; 3] {
    let p = stock_profile(stock);
    let ch = character.clamp(0.0, 2.0);
    let mut out = [0.0f32; 3];
    for i in 0..3 {
        let v = rgb[i].max(0.0); // exposure applied outside
                                 // Exposure-to-density-ish log curve.
        let x = (v * 4.0 + 1.0e-4).ln_1p();
        let g = match i {
            0 => p.gamma_r,
            1 => p.gamma_g,
            _ => p.gamma_b,
        };
        // Density
        let mut d = (x * g).max(0.0);
        d += p.toe * (1.0 - (-v * 3.0).exp());
        d *= 0.55 + 0.45 * ch;
        d += p.cast[i] * ch;
        // Print back to a positive image. Increased exposure must increase output.
        let pos = 1.0 - (-d * 0.85).exp();
        out[i] = apply_shoulder(pos, shoulder.max(p.shoulder * 0.5));
    }
    if p.mono {
        let y = 0.2126 * out[0] + 0.7152 * out[1] + 0.0722 * out[2];
        out = [y, y, y];
    } else {
        let y = 0.2126 * out[0] + 0.7152 * out[1] + 0.0722 * out[2];
        let sat = p.sat_bias * (0.5 + 0.5 * ch);
        for i in 0..3 {
            out[i] = y + (out[i] - y) * sat;
        }
    }
    out
}

#[inline]
pub fn luma(rgb: [f32; 3]) -> f32 {
    0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2]
}

#[inline]
pub fn saturate_around_luma(rgb: [f32; 3], sat: f32) -> [f32; 3] {
    let y = luma(rgb);
    [
        y + (rgb[0] - y) * sat,
        y + (rgb[1] - y) * sat,
        y + (rgb[2] - y) * sat,
    ]
}

/// Rough skin detection in linear-ish RGB.
#[inline]
pub fn skin_weight(rgb: [f32; 3]) -> f32 {
    let r = rgb[0];
    let g = rgb[1];
    let b = rgb[2];
    let y = luma(rgb);
    if y < 0.05 || y > 0.92 {
        return 0.0;
    }
    let rg = r - g;
    let rb = r - b;
    if rg > 0.02 && rb > 0.04 && r > g && g > b * 0.7 {
        ((rg * 4.0).clamp(0.0, 1.0) * (rb * 2.5).clamp(0.0, 1.0)).min(1.0)
    } else {
        0.0
    }
}

pub fn apply_skin(rgb: [f32; 3], lift: f32, sat: f32, hue_deg: f32) -> [f32; 3] {
    let w = skin_weight(rgb);
    if w <= 1.0e-4 {
        return rgb;
    }
    let mut t = rgb;
    t = [t[0] * lift, t[1] * lift, t[2] * lift];
    t = saturate_around_luma(t, sat);
    // Small hue rotate around luma toward orange/magenta.
    let ang = hue_deg.to_radians() * w;
    let cos_a = ang.cos();
    let sin_a = ang.sin();
    let y = luma(t);
    let cr = t[0] - y;
    let cb = t[2] - y;
    let cr2 = cr * cos_a - cb * sin_a;
    let cb2 = cr * sin_a + cb * cos_a;
    let shifted = [y + cr2, t[1], y + cb2];
    [
        rgb[0] + (shifted[0] - rgb[0]) * w,
        rgb[1] + (shifted[1] - rgb[1]) * w,
        rgb[2] + (shifted[2] - rgb[2]) * w,
    ]
}

/// Split-tone style crossover along an axis angle (degrees).
pub fn apply_crossover(rgb: [f32; 3], amount: f32, axis_deg: f32) -> [f32; 3] {
    if amount.abs() < 1.0e-5 {
        return rgb;
    }
    let y = luma(rgb);
    let t = (y - 0.5) * 2.0; // -1 shadow .. +1 highlight
    let ang = axis_deg.to_radians();
    let dir = [ang.cos(), 0.0, ang.sin()];
    let strength = amount * t * 0.15;
    [
        (rgb[0] + dir[0] * strength).max(0.0),
        (rgb[1] + dir[1] * strength).max(0.0),
        (rgb[2] + dir[2] * strength).max(0.0),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rec709_round_trip() {
        for linear in [0.0, 0.018, 0.18, 0.5, 1.0, 4.0] {
            let decoded = rec709_eotf(rec709_oetf(linear));
            assert!((decoded - linear).abs() < 2.0e-5);
        }
    }

    #[test]
    fn camera_log_middle_grey_references() {
        assert!((slog3_to_linear(420.0 / 1023.0) - 0.18).abs() < 1.0e-4);
        assert!((logc3_to_linear(0.391_007) - 0.18).abs() < 2.0e-3);
        assert!((vlog_to_linear(0.423_311) - 0.18).abs() < 2.0e-3);
    }
}
