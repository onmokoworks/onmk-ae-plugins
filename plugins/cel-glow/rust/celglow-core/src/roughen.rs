//! Alpha-boundary roughening used for the final Roughen Edges stage.

use crate::{noise, params::RoughenEdgeType, Params, RenderError, field::zero_plane};

const ALPHA_THRESHOLD: f32 = 1.0 / 255.0;
const INF: f32 = 1.0e20;
const DIAGONAL: f32 = std::f32::consts::SQRT_2;

/// Applies inward alpha erosion only within `roughen_border` pixels of a glow edge.
/// `glow` is premultiplied RGBA and is modified in place.
pub fn apply(glow: &mut [f32], w: usize, h: usize, origin: (i32, i32), params: &Params) -> Result<(),RenderError> {
    let count = w.checked_mul(h).ok_or(RenderError::FieldDimensions)?;
    let expected = count.checked_mul(4).ok_or(RenderError::FieldDimensions)?;
    if glow.len() != expected { return Err(RenderError::InputLength {actual:glow.len(),expected}); }
    if w == 0 || h == 0 || params.texture.influence <= 0.0 || params.texture.border <= 0.0 {
        return Ok(());
    }
    let mut alpha = zero_plane(count)?;
    for (value,pixel) in alpha.iter_mut().zip(glow.chunks_exact(4)) {
        *value = pixel[3].max(pixel[0]).max(pixel[1]).max(pixel[2]);
    }
    let distance = inside_distance(&alpha, w, h)?;
    let border = params.texture.border;
    let influence = params.texture.influence;
    let evolution = params.texture.evolution / 360.0;
    let seed = params.rings.seed as u32 ^ 0x43d1_8ae7;

    for y in 0..h {
        for x in 0..w {
            let index = y * w + x;
            let old_alpha = alpha[index];
            if old_alpha <= ALPHA_THRESHOLD || distance[index] > border {
                continue;
            }
            let n = (noise::fbm(
                [
                    (origin.0 as f32 + x as f32) / params.texture.scale,
                    (origin.1 as f32 + y as f32) / params.texture.scale,
                    evolution,
                ],
                params.texture.complexity,
                seed,
            ) + 1.0)
                * 0.5;
            let erosion = match params.texture.edge_type {
                RoughenEdgeType::Roughen => influence * n,
                RoughenEdgeType::Cut => influence * smoothstep(0.42, 0.58, n),
            };
            let softness = match params.texture.edge_type {
                RoughenEdgeType::Cut => 0.04,
                RoughenEdgeType::Roughen => (0.8 / (1.0 + params.texture.sharpness)).max(0.04),
            };
            let coverage = smoothstep(erosion - softness, erosion + softness, distance[index]);
            let pixel = index * 4;
            for channel in 0..4 {
                // No neighbouring RGBA is read; alpha/distance are immutable snapshots.
                glow[pixel + channel] *= coverage;
            }
        }
    }
    Ok(())
}

/// A two-pass 8-neighbour chamfer EDT. The roughening border is small, so this
/// approximation is sufficient while remaining deterministic and linear-time.
fn inside_distance(alpha: &[f32], w: usize, h: usize) -> Result<Vec<f32>,RenderError> {
    let mut d = zero_plane(alpha.len())?;
    for (value,&a) in d.iter_mut().zip(alpha) { *value = if a > ALPHA_THRESHOLD { INF } else { 0.0 }; }
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let mut value = d[i];
            if x > 0 {
                value = value.min(d[i - 1] + 1.0);
            }
            if y > 0 {
                value = value.min(d[i - w] + 1.0);
            }
            if x > 0 && y > 0 {
                value = value.min(d[i - w - 1] + DIAGONAL);
            }
            if x + 1 < w && y > 0 {
                value = value.min(d[i - w + 1] + DIAGONAL);
            }
            d[i] = value;
        }
    }
    for y in (0..h).rev() {
        for x in (0..w).rev() {
            let i = y * w + x;
            let mut value = d[i];
            if x + 1 < w {
                value = value.min(d[i + 1] + 1.0);
            }
            if y + 1 < h {
                value = value.min(d[i + w] + 1.0);
            }
            if x + 1 < w && y + 1 < h {
                value = value.min(d[i + w + 1] + DIAGONAL);
            }
            if x > 0 && y + 1 < h {
                value = value.min(d[i + w - 1] + DIAGONAL);
            }
            d[i] = value;
        }
    }
    Ok(d)
}

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_shape_is_rejected_without_modification_even_when_disabled() {
        let mut params=Params::default();
        for enabled in [false,true] {
            params.texture.influence=if enabled {1.0} else {0.0};
            let mut pixels=[0.2,0.4,0.8,1.0];
            assert!(apply(&mut pixels,2,1,(0,0),&params).is_err());
            assert_eq!(pixels,[0.2,0.4,0.8,1.0]);
            assert!(apply(&mut pixels,usize::MAX,2,(0,0),&params).is_err());
            assert_eq!(pixels,[0.2,0.4,0.8,1.0]);
        }
        apply(&mut [],0,7,(0,0),&params).unwrap();
    }

    #[test]
    fn in_place_erosion_scales_original_channels_consistently() {
        for edge_type in [RoughenEdgeType::Cut,RoughenEdgeType::Roughen] {
            let mut params=Params::default();
            params.texture.edge_type=edge_type;
            params.texture.influence=3.0;
            let mut pixels=vec![0.0;7*9*4];
            for y in 1..8 {for x in 1..6 {
                pixels[(y*7+x)*4..(y*7+x)*4+4].copy_from_slice(&[0.2,0.4,0.8,1.0]);
            }}
            let before=pixels.clone();
            apply(&mut pixels,7,9,(-5,13),&params).unwrap();
            for (old,new) in before.chunks_exact(4).zip(pixels.chunks_exact(4)) {
                assert!(new[3]>=0.0 && new[3]<=old[3]);
                for channel in 0..3 {assert!((new[channel]-old[channel]*new[3]).abs()<1e-6);}
            }
            assert_ne!(pixels,before);
        }
    }

    #[test]
    fn erodes_only_alpha_edges_and_keeps_center() {
        let mut glow = vec![0.0; 9 * 9 * 4];
        for y in 2..7 {
            for x in 2..7 {
                let i = (y * 9 + x) * 4;
                glow[i..i + 4].copy_from_slice(&[1.0, 1.0, 1.0, 1.0]);
            }
        }
        let before = glow.clone();
        let mut params = Params::default();
        params.texture.border = 2.5;
        params.texture.influence = 1.5;
        apply(&mut glow, 9, 9, (0, 0), &params).unwrap();
        assert_eq!(&glow[(4 * 9 + 4) * 4..(4 * 9 + 4) * 4 + 4], &[1.0; 4]);
        assert!(glow[(2 * 9 + 2) * 4 + 3] <= before[(2 * 9 + 2) * 4 + 3]);
        assert_eq!(&glow[..4], &[0.0; 4]);
    }
}
