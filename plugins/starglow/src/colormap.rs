use crate::EffectParams;

/// Get the 5-color gradient from params (or override with preset)
pub fn get_colormap(ep: &EffectParams) -> [[f32; 3]; 5] {
    if ep.colormap_preset == 13 {
        // Spectrum mode: return white as placeholder; actual coloring is per-direction
        return [[1.0; 3]; 5];
    }
    match ep.colormap_preset {
        // 1 = One Color (all white)
        1 => {
            let c = ep.colors[0].map(|v| v as f32);
            [c, c, c, c, c]
        }
        // 2 = 3-Color Gradient (highlights, midtones, shadows)
        2 => {
            let hi = ep.colors[0].map(|v| v as f32);
            let mid = ep.colors[2].map(|v| v as f32);
            let lo = ep.colors[4].map(|v| v as f32);
            let mh = lerp3(hi, mid, 0.5);
            let ml = lerp3(mid, lo, 0.5);
            [hi, mh, mid, ml, lo]
        }
        // 3 = 5-Color Gradient (use all 5 user colors)
        3 => ep.colors.map(|c| c.map(|v| v as f32)),
        // Presets
        5 => FIRE,
        6 => ELECTRIC,
        7 => RAINBOW,
        8 => HEAVEN,
        9 => ROMANCE,
        10 => AQUALIGHT,
        11 => SUNSET,
        // Default: user colors
        _ => ep.colors.map(|c| c.map(|v| v as f32)),
    }
}

/// Sample a 5-point gradient at position t (0..1)
/// t=0 -> highlights (bright center), t=1 -> shadows (streak tip)
pub fn sample_gradient(colors: &[[f32; 3]; 5], t: f32) -> [f32; 3] {
    let t = t.clamp(0.0, 1.0);
    let seg = t * 4.0; // 4 segments between 5 colors
    let idx = (seg as usize).min(3);
    let frac = seg - idx as f32;

    lerp3(colors[idx], colors[idx + 1], frac)
}

fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

/// Sample spectrum color at a given depth along a streak.
/// depth: 0.0 (center) to 1.0 (tip)
/// offset: hue offset in degrees (0-360)
/// density: how many full rainbow cycles per streak (1.0 = one full cycle)
/// random_offset: additional random hue shift (0-360)
pub fn sample_spectrum(depth: f32, offset: f32, density: f32, random_offset: f32) -> [f32; 3] {
    let hue = (offset + random_offset + depth * density * 360.0) % 360.0;
    let hue = if hue < 0.0 { hue + 360.0 } else { hue };
    hsv_to_rgb(hue, 1.0, 1.0)
}

pub fn hsv_to_rgb(h: f32, s: f32, v: f32) -> [f32; 3] {
    let c = v * s;
    let hp = h / 60.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r1, g1, b1) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    [r1 + m, g1 + m, b1 + m]
}

// ---- Preset colormaps ----
// [highlights, mid-high, midtones, mid-low, shadows]

const FIRE: [[f32; 3]; 5] = [
    [1.0, 1.0, 0.9],   // white-yellow
    [1.0, 0.85, 0.3],  // yellow
    [1.0, 0.5, 0.1],   // orange
    [0.9, 0.2, 0.0],   // red-orange
    [0.4, 0.0, 0.0],   // dark red
];

const ELECTRIC: [[f32; 3]; 5] = [
    [1.0, 1.0, 1.0],   // white
    [0.7, 0.85, 1.0],  // light blue
    [0.3, 0.5, 1.0],   // blue
    [0.5, 0.2, 1.0],   // purple
    [0.2, 0.0, 0.5],   // dark purple
];

const RAINBOW: [[f32; 3]; 5] = [
    [1.0, 0.2, 0.2],   // red
    [1.0, 0.8, 0.2],   // yellow
    [0.2, 1.0, 0.3],   // green
    [0.2, 0.5, 1.0],   // blue
    [0.6, 0.2, 1.0],   // purple
];

const HEAVEN: [[f32; 3]; 5] = [
    [1.0, 1.0, 1.0],   // white
    [0.9, 0.95, 1.0],  // near-white blue
    [0.7, 0.85, 1.0],  // soft blue
    [0.5, 0.7, 0.95],  // medium blue
    [0.3, 0.5, 0.8],   // sky blue
];

const ROMANCE: [[f32; 3]; 5] = [
    [1.0, 0.95, 1.0],  // white-pink
    [1.0, 0.7, 0.85],  // pink
    [1.0, 0.4, 0.6],   // hot pink
    [0.8, 0.2, 0.5],   // magenta
    [0.4, 0.05, 0.25], // dark magenta
];

const AQUALIGHT: [[f32; 3]; 5] = [
    [0.9, 1.0, 1.0],   // near white cyan
    [0.5, 0.95, 0.9],  // aqua
    [0.2, 0.8, 0.7],   // teal
    [0.1, 0.5, 0.6],   // dark teal
    [0.0, 0.2, 0.3],   // deep teal
];

const SUNSET: [[f32; 3]; 5] = [
    [1.0, 1.0, 0.8],   // warm white
    [1.0, 0.8, 0.4],   // golden
    [1.0, 0.5, 0.2],   // orange
    [0.8, 0.2, 0.2],   // red
    [0.3, 0.05, 0.2],  // dark magenta
];
