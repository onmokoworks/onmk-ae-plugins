//! AE 非依存の CelGlow v2 コア。
//! M0 では座標・バッファ境界を検証する RGBA パススルーを提供する。

pub mod distance;
pub mod field;
pub mod noise;
pub mod params;
pub mod radial;
pub mod render;
pub mod roughen;

pub use params::Params;

use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrameBuf<'a> {
    pub w: usize,
    pub h: usize,
    pub rgba: &'a [f32],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebugView {
    Result,
    GlowOnly,
    Field,
    /// Normalized inverse field: center is dark and the outer radius is bright.
    Radius,
    Rings,
    TextureMask,
    Source,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RenderError {
    #[error("input buffer length is {actual}; expected {expected} (w*h*4)")]
    InputLength { actual: usize, expected: usize },
    #[error("output buffer length is {actual}; expected {expected} (output_w*output_h*4)")]
    OutputLength { actual: usize, expected: usize },
    #[error("output rect at {output_origin:?} with size {output_size:?} is not contained in input rect at {input_origin:?} with size {input_size:?}")]
    OutputOutsideInput {
        input_origin: (i32, i32),
        input_size: (usize, usize),
        output_origin: (i32, i32),
        output_size: (usize, usize),
    },
}

/// Render an output rect from an input rect.
///
/// Coordinates are full-resolution layer coordinates.
///
/// M1 evaluates the field over the whole checkout input, then renders only the
/// requested output rect. This preserves the distinction needed by SmartFX.
pub fn render(
    input: FrameBuf<'_>,
    input_origin: (i32, i32),
    output_origin: (i32, i32),
    output_size: (usize, usize),
    params: &Params,
    view: DebugView,
    out: &mut [f32],
) -> Result<(), RenderError> {
    let input_expected = input.w.saturating_mul(input.h).saturating_mul(4);
    if input.rgba.len() != input_expected {
        return Err(RenderError::InputLength {
            actual: input.rgba.len(),
            expected: input_expected,
        });
    }
    let output_expected = output_size
        .0
        .saturating_mul(output_size.1)
        .saturating_mul(4);
    if out.len() != output_expected {
        return Err(RenderError::OutputLength {
            actual: out.len(),
            expected: output_expected,
        });
    }

    let x = i64::from(output_origin.0) - i64::from(input_origin.0);
    let y = i64::from(output_origin.1) - i64::from(input_origin.1);
    let within = x >= 0
        && y >= 0
        && x + output_size.0 as i64 <= input.w as i64
        && y + output_size.1 as i64 <= input.h as i64;
    if !within {
        return Err(RenderError::OutputOutsideInput {
            input_origin,
            input_size: (input.w, input.h),
            output_origin,
            output_size,
        });
    }

    let source_x = x as usize;
    let source_y = y as usize;
    if matches!(view, DebugView::Source) {
        for row in 0..output_size.1 {
            for col in 0..output_size.0 {
                let input_x = source_x.saturating_add(col);
                let input_y = source_y.saturating_add(row);
                let destination = (row * output_size.0 + col) * 4;
                if input_x < input.w && input_y < input.h {
                    let i = (input_y * input.w + input_x) * 4;
                    if let Some(source_slice) = input.rgba.get(i..i + 4) {
                        let source = [source_slice[0], source_slice[1], source_slice[2], source_slice[3]];
                        let value = render::source_strength(source, params);
                        out[destination..destination + 4].copy_from_slice(&[value, value, value, 1.0]);
                    } else {
                        out[destination..destination + 4].fill(0.0);
                    }
                } else {
                    out[destination..destination + 4].fill(0.0);
                }
            }
        }
        return Ok(());
    }
    let field = field::Field::build(input, params);
    if matches!(
        view,
        DebugView::Result | DebugView::GlowOnly | DebugView::TextureMask
    ) {
        // Roughen Edges must see the complete checkout glow, rather than only
        // the requested output rect, otherwise its alpha boundary changes with ROI.
        let mut glow = vec![0.0; input_expected];
        for y in 0..input.h {
            for x in 0..input.w {
                let pixel = render::render_pixel(
                    input,
                    &field,
                    params,
                    (x, y),
                    (
                        input_origin.0 as f32 + x as f32,
                        input_origin.1 as f32 + y as f32,
                    ),
                    DebugView::GlowOnly,
                );
                let destination = (y * input.w + x) * 4;
                glow[destination..destination + 4].copy_from_slice(&pixel);
            }
        }
        roughen::apply(&mut glow, input.w, input.h, input_origin, params);
        if matches!(view, DebugView::Result | DebugView::GlowOnly) {
            render::colorize_glow(&mut glow, params);
        }
        for row in 0..output_size.1 {
            for col in 0..output_size.0 {
                let source_index = ((source_y + row) * input.w + source_x + col) * 4;
                let destination = (row * output_size.0 + col) * 4;
                if matches!(view, DebugView::GlowOnly | DebugView::TextureMask) {
                    out[destination..destination + 4]
                        .copy_from_slice(&glow[source_index..source_index + 4]);
                } else {
                    let source = [
                        input.rgba[source_index],
                        input.rgba[source_index + 1],
                        input.rgba[source_index + 2],
                        input.rgba[source_index + 3],
                    ];
                    let result = render::composite_with_options(
                        source,
                        [
                            glow[source_index],
                            glow[source_index + 1],
                            glow[source_index + 2],
                            glow[source_index + 3],
                        ],
                        params,
                    );
                    out[destination..destination + 4].copy_from_slice(&result);
                }
            }
        }
        return Ok(());
    }
    for row in 0..output_size.1 {
        for col in 0..output_size.0 {
            let input_x = source_x.saturating_add(col);
            let input_y = source_y.saturating_add(row);
            let destination = (row * output_size.0 + col) * 4;
            if input_x < input.w && input_y < input.h {
                let layer_xy = (
                    output_origin.0 as f32 + col as f32,
                    output_origin.1 as f32 + row as f32,
                );
                let pixel = match view {
                    // Keep the simple diagnostic views independent of the
                    // ring/noise evaluator. This also makes them safe for
                    // hosts that send a clipped SmartFX ROI.
                    DebugView::Field | DebugView::Radius => {
                        let q = render::displaced_input_position_for_debug(
                            (input_x, input_y), layer_xy, params,
                        );
                        let normalized = field.normalized(field.sample(q.0, q.1));
                        let value = if matches!(view, DebugView::Radius) { 1.0 - normalized } else { normalized };
                        [value, value, value, 1.0]
                    }
                    _ => render::render_pixel(input, &field, params, (input_x, input_y), layer_xy, view),
                };
                out[destination..destination + 4].copy_from_slice(&pixel);
            } else {
                out[destination..destination + 4].fill(0.0);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crops_using_layer_origins() {
        let pixels: Vec<f32> = (0..(4 * 3 * 4)).map(|v| v as f32).collect();
        let mut output = vec![0.0; 2 * 2 * 4];
        let mut params = Params::default();
        params.source.gain = 0.0;
        render(
            FrameBuf {
                w: 4,
                h: 3,
                rgba: &pixels,
            },
            (-1, -1),
            (0, 0),
            (2, 2),
            &params,
            DebugView::Result,
            &mut output,
        )
        .unwrap();
        assert_eq!(&output[..4], &pixels[(1 * 4 + 1) * 4..(1 * 4 + 2) * 4]);
    }

    #[test]
    fn rejects_wrong_output_length() {
        let input = [0.0; 4];
        let mut output = [];
        assert!(matches!(
            render(
                FrameBuf {
                    w: 1,
                    h: 1,
                    rgba: &input
                },
                (0, 0),
                (0, 0),
                (1, 1),
                &Params::default(),
                DebugView::Result,
                &mut output
            ),
            Err(RenderError::OutputLength { .. })
        ));
    }

    #[test]
    fn m2_is_deterministic_and_applies_wobble_and_scramble() {
        let mut pixels = vec![0.0; 96 * 96 * 4];
        for y in 40..56 {
            for x in 40..56 {
                let i = (y * 96 + x) * 4;
                pixels[i..i + 4].copy_from_slice(&[1.0, 1.0, 1.0, 1.0]);
            }
        }
        let mut params = Params::default();
        params.source.spread = 30.0;
        params.rings.ring_count = 12;
        params.rings.color_scramble = 30.0;
        let frame = FrameBuf {
            w: 96,
            h: 96,
            rgba: &pixels,
        };
        let mut first = vec![0.0; pixels.len()];
        let mut second = vec![0.0; pixels.len()];
        render(
            frame,
            (0, 0),
            (0, 0),
            (96, 96),
            &params,
            DebugView::Rings,
            &mut first,
        )
        .unwrap();
        render(
            frame,
            (0, 0),
            (0, 0),
            (96, 96),
            &params,
            DebugView::Rings,
            &mut second,
        )
        .unwrap();
        assert_eq!(first, second);
        assert!(first
            .chunks_exact(4)
            .any(|pixel| (pixel[0] - pixel[1]).abs() > 1e-5));

        params.wobble.amount = 0.0;
        let mut without_wobble = vec![0.0; pixels.len()];
        render(
            frame,
            (0, 0),
            (0, 0),
            (96, 96),
            &params,
            DebugView::Rings,
            &mut without_wobble,
        )
        .unwrap();
        assert_ne!(first, without_wobble);
    }
}
