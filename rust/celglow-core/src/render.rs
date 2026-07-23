use crate::{field::Field, noise, params::ColorMode, DebugView, FrameBuf, Params};

pub fn render_pixel(
    input: FrameBuf<'_>,
    field: &Field,
    params: &Params,
    input_xy: (usize, usize),
    layer_xy: (f32, f32),
    view: DebugView,
) -> [f32; 4] {
    let index = (input_xy.1 * input.w + input_xy.0) * 4;
    let source = [
        input.rgba[index],
        input.rgba[index + 1],
        input.rgba[index + 2],
        input.rgba[index + 3],
    ];
    let strength = source_strength(source, params);
    match view {
        DebugView::Source => [strength, strength, strength, 1.0],
        DebugView::Field => {
            let q = displaced_input_position_for_debug(input_xy, layer_xy, params);
            let f = field.sample(q.0, q.1);
            let value = field.normalized(f);
            [value, value, value, 1.0]
        }
        DebugView::Radius => {
            let q = displaced_input_position_for_debug(input_xy, layer_xy, params);
            let f = field.sample(q.0, q.1);
            let value = 1.0 - field.normalized(f);
            [value, value, value, 1.0]
        }
        DebugView::Result | DebugView::GlowOnly | DebugView::Rings | DebugView::TextureMask => {
            let q = displaced_input_position_for_debug(input_xy, layer_xy, params);
            let raw_rings = ring_intensity(field, params, q, layer_xy);
            match view {
                DebugView::Result => composite_preview(source, raw_rings),
                DebugView::GlowOnly => rgba_from_rgb(raw_rings),
                DebugView::Rings | DebugView::TextureMask => rgba_from_rgb(raw_rings),
                _ => unreachable!(),
            }
        }
    }
}

pub(crate) fn displaced_input_position_for_debug(
    input_xy: (usize, usize),
    layer_xy: (f32, f32),
    params: &Params,
) -> (f32, f32) {
    let z = params.wobble.evolution / 360.0;
    let p = [
        layer_xy.0 / params.wobble.scale,
        layer_xy.1 / params.wobble.scale,
        z,
    ];
    let seed = params.rings.seed as u32;
    let dx = noise::fbm(p, params.wobble.complexity, seed ^ 0x517c_c1b7);
    let dy = noise::fbm(
        [p[0] + 19.19, p[1] - 73.73, p[2] + 0.37],
        params.wobble.complexity,
        seed ^ 0xa31f_4d29,
    );
    (
        input_xy.0 as f32 + params.wobble.amount * dx,
        input_xy.1 as f32 + params.wobble.amount * dy,
    )
}

fn ring_intensity(field: &Field, params: &Params, q: (f32, f32), layer_xy: (f32, f32)) -> [f32; 3] {
    let f = field.normalized(field.sample(q.0, q.1));
    let core_level = params.rings.core_level / 100.0;
    let fade = (params.rings.core_softness / 100.0).min(core_level);
    let core = smoothstep(core_level - fade, core_level + fade, f);
    let u = (f / core_level).clamp(0.0, 1.0);
    let macro_warp = noise::fbm(
        [
            layer_xy.0 / params.unevenness.scale,
            layer_xy.1 / params.unevenness.scale,
            params.unevenness.evolution / 360.0,
        ],
        3,
        params.rings.seed as u32 ^ 0x2c8d_61f9,
    ) * 0.035
        * (params.unevenness.amount / 100.0);
    let phase = params.rings.ring_count as f32
        * (1.0 - (u + macro_warp).clamp(0.0, 1.0)).powf(params.rings.distribution)
        + params.rings.phase / 360.0;
    let k = phase.floor() as i32;
    // The field becomes spatially very broad close to zero. Hiding that final
    // band avoids an oversized outer halo while retaining the spacing of all
    // interior bands.
    if !params.rings.show_outermost && k == i32::from(params.rings.ring_count) - 1 {
        return [0.0; 3];
    }
    let d = ((phase - phase.floor()) - 0.5).abs();
    let width_noise = noise::fbm(
        [
            layer_xy.0 / 72.0,
            layer_xy.1 / 72.0,
            params.unevenness.evolution / 360.0 + 0.41,
        ],
        2,
        params.rings.seed as u32 ^ 0x7a91_4f0d,
    );
    let ring_t = (k.max(0) as f32
        / params.rings.ring_count.saturating_sub(1).max(1) as f32)
        .clamp(0.0, 1.0);
    let outer_thinning = 1.0 - ring_t * 0.48;
    let half_width = (params.rings.line_width / 100.0) * 0.5
        * outer_thinning
        * (1.0 + width_noise * 0.22);
    let hardness = params.rings.line_hardness / 100.0;
    let aa = (1.0 - hardness) * half_width;
    let mut wave = if aa <= f32::EPSILON {
        if d <= half_width {
            1.0
        } else {
            0.0
        }
    } else {
        1.0 - smoothstep(half_width - aa, half_width + aa, d)
    };
    if half_width > 0.0 {
        wave += (half_width - wave) * (aa / half_width - 1.0).clamp(0.0, 1.0);
    }
    // A zero falloff intentionally means a flat, cel-painted band. Keeping
    // this explicit also avoids relying on the 0^0 behaviour at the outer
    // edge of the field.
    if k < 0 || k >= i32::from(params.rings.ring_count) {
        return [0.0; 3];
    }
    let envelope = wave * (1.0 - core);
    let key = k as u32; // signed k の2の補数ビット列を乱数キーとして使う。
    let variation = params.rings.brightness_variation / 100.0;
    let scramble = params.rings.color_scramble / 100.0;
    let base = 1.0 - variation * noise::u01(key, params.rings.seed as u32, 0);
    [0_u32, 1, 2].map(|channel| {
        envelope
            * base
            * (1.0 - scramble * (noise::u01(key, params.rings.seed as u32, channel + 1) - 0.5))
    })
}

#[allow(dead_code)]
fn apply_unevenness(rings: [f32; 3], layer_xy: (f32, f32), params: &Params) -> [f32; 3] {
    let macro_noise = (noise::fbm(
        [
            layer_xy.0 / params.unevenness.scale,
            layer_xy.1 / params.unevenness.scale,
            params.unevenness.evolution / 360.0,
        ],
        3,
        params.rings.seed as u32 ^ 0x2c8d_61f9,
    ) + 1.0)
        * 0.5;
    // fBm の中央に集まりやすい値域を緩やかに広げ、広い雲状の濃淡として読む。
    let haze = smoothstep(0.25, 0.75, macro_noise);
    let multiplier = 1.0 - (params.unevenness.amount / 100.0) * haze;
    rings.map(|value| value * multiplier.clamp(0.0, 1.0))
}

pub fn composite_preview(source: [f32; 4], glow: [f32; 3]) -> [f32; 4] {
    [
        source[0] + glow[0],
        source[1] + glow[1],
        source[2] + glow[2],
        source[3].max(glow[0].max(glow[1]).max(glow[2])),
    ]
}

/// Applies the selected line colour to a premultiplied glow buffer.
/// Fill imposes one colour. Band brightness stays flat unless the user
/// explicitly enables a ring variation or macro unevenness control.
pub fn colorize_glow(glow: &mut [f32], params: &Params) {
    if !matches!(params.output.color_mode, ColorMode::Fill) {
        return;
    }
    let opacity = params.output.opacity / 100.0;
    for pixel in glow.chunks_exact_mut(4) {
        pixel[0] *= params.output.fill_color[0] * opacity;
        pixel[1] *= params.output.fill_color[1] * opacity;
        pixel[2] *= params.output.fill_color[2] * opacity;
        pixel[3] *= opacity;
    }
}

/// Glow Alpha controls the final alpha channel; ON (the default) writes the
/// glow alpha, while OFF leaves the source alpha unchanged.
pub fn composite_with_options(source: [f32; 4], glow: [f32; 4], params: &Params) -> [f32; 4] {
    let mut result = composite_preview(source, [glow[0], glow[1], glow[2]]);
    if !params.output.write_glow_alpha {
        result[3] = source[3];
    }
    result
}
fn rgba_from_rgb(rgb: [f32; 3]) -> [f32; 4] {
    [rgb[0], rgb[1], rgb[2], rgb[0].max(rgb[1]).max(rgb[2])]
}

pub(crate) fn source_strength(source: [f32; 4], params: &Params) -> f32 {
    use crate::params::SourceChannel;
    let luma = 0.2126 * source[0] + 0.7152 * source[1] + 0.0722 * source[2];
    let value = match params.source.channel {
        SourceChannel::Alpha => source[3],
        SourceChannel::Luma => luma,
        SourceChannel::LumaXAlpha => luma * source[3],
    };
    let strength = (value * (params.source.gain / 100.0))
        .clamp(0.0, 1.0)
        .powf(params.source.gamma);
    let threshold = (params.source.threshold / 100.0).clamp(0.0, 1.0);
    let softness = (params.source.threshold_softness / 100.0).max(0.0);
    let gate = if threshold <= f32::EPSILON {
        1.0
    } else if softness <= f32::EPSILON {
        if strength >= threshold { 1.0 } else { 0.0 }
    } else {
        let t = ((strength - threshold) / softness).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    };
    strength * gate
}

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    if edge0 >= edge1 {
        return if value >= edge1 { 1.0 } else { 0.0 };
    }
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_color_tints_glow_without_changing_alpha() {
        let mut glow = [0.5, 0.25, 0.75, 0.75];
        let mut params = Params::default();
        params.output.fill_color = [1.0, 0.4, 0.0];
        params.output.opacity = 100.0;
        colorize_glow(&mut glow, &params);
        assert_eq!(glow, [0.5, 0.1, 0.0, 0.75]);
    }

    #[test]
    fn glow_alpha_can_be_disabled() {
        let mut params = Params::default();
        params.output.write_glow_alpha = false;
        let result = composite_with_options([0.2, 0.3, 0.4, 0.25], [0.5, 0.5, 0.5, 0.5], &params);
        assert_eq!(result[3], 0.25);
        assert_eq!(&result[..3], &[0.7, 0.8, 0.9]);

        params.output.write_glow_alpha = true;
        let result = composite_with_options([0.2, 0.3, 0.4, 0.25], [0.5, 0.5, 0.5, 0.5], &params);
        assert_eq!(result[3], 0.5);
    }
}
