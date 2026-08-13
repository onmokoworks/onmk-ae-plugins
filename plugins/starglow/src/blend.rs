/// Composite glow onto source using transfer mode
/// All values are 0.0..1.0+
/// Returns (r, g, b)
pub fn composite(
    sr: f32, sg: f32, sb: f32,  // source RGB
    gr: f32, gg: f32, gb: f32,  // glow RGB
    source_opacity: f32,
    transfer_mode: i32,
) -> (f32, f32, f32) {
    // Apply transfer mode to combine glow with source
    let (br, bg, bb) = apply_mode(sr, sg, sb, gr, gg, gb, transfer_mode);

    // source * source_opacity + glow contribution
    let or = sr * source_opacity + br;
    let og = sg * source_opacity + bg;
    let ob = sb * source_opacity + bb;

    (or.max(0.0), og.max(0.0), ob.max(0.0))
}

fn apply_mode(
    sr: f32, sg: f32, sb: f32,
    gr: f32, gg: f32, gb: f32,
    mode: i32,
) -> (f32, f32, f32) {
    match mode {
        // 1 = None - just source
        1 => (0.0, 0.0, 0.0),
        // 2 = Normal
        2 => {
            let a = (gr + gg + gb) / 3.0; // pseudo-alpha from glow intensity
            let a = a.clamp(0.0, 1.0);
            (gr * a, gg * a, gb * a)
        }
        // 4 = Add
        4 => (gr, gg, gb),
        // 5 = Multiply
        5 => (sr * gr, sg * gg, sb * gb),
        // 6 = Screen
        6 => {
            (
                sr + gr - sr * gr,
                sg + gg - sg * gg,
                sb + gb - sb * gb,
            )
        }
        // 7 = Overlay
        7 => (overlay(sr, gr), overlay(sg, gg), overlay(sb, gb)),
        // 8 = Soft Light
        8 => (soft_light(sr, gr), soft_light(sg, gg), soft_light(sb, gb)),
        // 9 = Hard Light
        9 => (overlay(gr, sr), overlay(gg, sg), overlay(gb, sb)),
        // 11 = Color Dodge
        11 => (color_dodge(sr, gr), color_dodge(sg, gg), color_dodge(sb, gb)),
        // 12 = Color Burn
        12 => (color_burn(sr, gr), color_burn(sg, gg), color_burn(sb, gb)),
        // 14 = Darken
        14 => (sr.min(gr), sg.min(gg), sb.min(gb)),
        // 15 = Lighten
        15 => (sr.max(gr), sg.max(gg), sb.max(gb)),
        // 16 = Difference
        16 => ((sr - gr).abs(), (sg - gg).abs(), (sb - gb).abs()),
        // Default: Add
        _ => (gr, gg, gb),
    }
}

fn overlay(base: f32, blend: f32) -> f32 {
    if base < 0.5 {
        2.0 * base * blend
    } else {
        1.0 - 2.0 * (1.0 - base) * (1.0 - blend)
    }
}

fn soft_light(base: f32, blend: f32) -> f32 {
    if blend < 0.5 {
        base - (1.0 - 2.0 * blend) * base * (1.0 - base)
    } else {
        let d = if base < 0.25 {
            ((16.0 * base - 12.0) * base + 4.0) * base
        } else {
            base.sqrt()
        };
        base + (2.0 * blend - 1.0) * (d - base)
    }
}

fn color_dodge(base: f32, blend: f32) -> f32 {
    if blend >= 1.0 {
        1.0
    } else {
        (base / (1.0 - blend)).min(1.0)
    }
}

fn color_burn(base: f32, blend: f32) -> f32 {
    if blend <= 0.0 {
        0.0
    } else {
        (1.0 - (1.0 - base) / blend).max(0.0)
    }
}
