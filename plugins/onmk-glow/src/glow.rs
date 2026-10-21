/// High-performance multi-pass glow for onmkGlow.
///
/// Multi-stage glow algorithm:
/// 1. Threshold source to extract bright areas (per-channel threshold + smoothstep)
/// 2. Core ultra_glow: separable blur with falloff_power = 10^falloff, bias, per-channel width
/// 3. After Glow pipeline: medium → huge → vertical streaks → horizontal streaks
/// 4. Combine with source (Mult, Add, Screen, Difference, Overlay)
///
/// Pixel format: flat native-depth ARGB, four channels per pixel.

use crate::GlowParams;
use after_effects::Error;
type RenderResult<T> = Result<T,Error>;

fn filled<T: Copy>(count: usize,value:T)->RenderResult<Vec<T>> {
    let mut data=Vec::new();
    data.try_reserve_exact(count).map_err(|_|Error::OutOfMemory)?;
    data.resize(count,value);
    Ok(data)
}
fn copied<T: Copy>(src:&[T])->RenderResult<Vec<T>> {
    let mut data=Vec::new();
    data.try_reserve_exact(src.len()).map_err(|_|Error::OutOfMemory)?;
    data.extend_from_slice(src);
    Ok(data)
}

pub trait Channel: Copy + Default {
    fn normalized(self) -> f32;
    fn rgb(value: f32) -> Self;
    fn alpha(value: f32) -> Self { Self::rgb(value) }
}
impl Channel for u8 {
    fn normalized(self) -> f32 { self as f32 / 255.0 }
    fn rgb(value: f32) -> Self { (value * 255.0).clamp(0.0, 255.0) as u8 }
}
impl Channel for u16 {
    fn normalized(self) -> f32 { self as f32 / 32768.0 }
    fn rgb(value: f32) -> Self { (value * 32768.0).round().clamp(0.0, 32768.0) as u16 }
}
impl Channel for f32 {
    fn normalized(self) -> f32 { self }
    fn rgb(value: f32) -> Self { value }
    fn alpha(value: f32) -> Self { value.clamp(0.0, 1.0) }
}

// ---- Box blur (3-pass Gaussian approximation, O(1)/pixel) ----

fn boxes_for_gauss(sigma: f32) -> RenderResult<[usize; 3]> {
    let n = 3.0f32;
    let w_ideal = ((12.0 * sigma * sigma / n) + 1.0).sqrt();
    if !w_ideal.is_finite() {return Err(Error::BadCallbackParameter);}
    let mut wl = w_ideal.floor() as usize;
    if wl % 2 == 0 && wl > 0 {
        wl -= 1;
    }
    let wu = wl.checked_add(2).filter(|&v|v<=isize::MAX as usize)
        .ok_or(Error::BadCallbackParameter)?;
    let m = ((12.0 * sigma * sigma
        - (n * wl as f32 * wl as f32)
        - (4.0 * n * wl as f32)
        - (3.0 * n))
        / (-4.0 * wl as f32 - 4.0))
        .round()
        .max(0.0) as usize;

    let mut sizes = [0usize; 3];
    for i in 0..3 {
        sizes[i] = if i < m { wl } else { wu };
    }
    for s in &mut sizes {
        *s = (*s).max(1) / 2;
    }
    Ok(sizes)
}

/// Horizontal box blur pass on f32 single-channel buffer.
fn box_blur_h(src: &[f32], dst: &mut [f32], w: usize, h: usize, r: usize) {
    if r == 0 {
        dst.copy_from_slice(src);
        return;
    }
    let diam = (2 * r + 1) as f32;
    let inv = 1.0 / diam;
    for y in 0..h {
        let row = y * w;
        let mut running = src[row] * (r + 1) as f32;
        for i in 1..=r.min(w - 1) {
            running += src[row + i];
        }
        if r >= w {
            running += src[row + w - 1] * (r - w + 1) as f32;
        }

        for x in 0..w {
            dst[row + x] = running * inv;
            let add_x = (x + r + 1).min(w - 1);
            let rem_x = x.saturating_sub(r);
            running += src[row + add_x] - src[row + rem_x];
        }
    }
}

/// Vertical box blur pass on f32 single-channel buffer.
fn box_blur_v(src: &[f32], dst: &mut [f32], w: usize, h: usize, r: usize) {
    if r == 0 {
        dst.copy_from_slice(src);
        return;
    }
    let diam = (2 * r + 1) as f32;
    let inv = 1.0 / diam;
    for x in 0..w {
        let mut running = src[x] * (r + 1) as f32;
        for i in 1..=r.min(h - 1) {
            running += src[i * w + x];
        }
        if r >= h {
            running += src[(h - 1) * w + x] * (r - h + 1) as f32;
        }

        for y in 0..h {
            dst[y * w + x] = running * inv;
            let add_y = (y + r + 1).min(h - 1);
            let rem_y = y.saturating_sub(r);
            running += src[add_y * w + x] - src[rem_y * w + x];
        }
    }
}

/// Separable box blur with independent X and Y radii, 3-pass.
fn box_blur_xy(src: &[f32], w: usize, h: usize, sigma_x: f32, sigma_y: f32) -> RenderResult<Vec<f32>> {
    if !sigma_x.is_finite() || !sigma_y.is_finite() {return Err(Error::BadCallbackParameter);}
    if sigma_x < 0.5 && sigma_y < 0.5 {
        return copied(src);
    }

    let boxes_x = boxes_for_gauss(sigma_x.max(0.5))?;
    let boxes_y = boxes_for_gauss(sigma_y.max(0.5))?;

    let mut buf_a = copied(src)?;
    let mut buf_b = filled(w*h,0.0f32)?;

    for i in 0..3 {
        let rx = if sigma_x >= 0.5 { boxes_x[i] } else { 0 };
        let ry = if sigma_y >= 0.5 { boxes_y[i] } else { 0 };
        box_blur_h(&buf_a, &mut buf_b, w, h, rx);
        box_blur_v(&buf_b, &mut buf_a, w, h, ry);
    }
    Ok(buf_a)
}

// ---- Blend modes ----

#[inline]
fn blend_screen(a: f32, b: f32) -> f32 {
    1.0 - (1.0 - a) * (1.0 - b)
}

#[inline]
fn blend_add(a: f32, b: f32) -> f32 {
    a + b
}

#[inline]
fn blend_multiply(a: f32, b: f32) -> f32 {
    a * b
}

#[inline]
fn blend_difference(a: f32, b: f32) -> f32 {
    (a - b).abs()
}

#[inline]
fn blend_overlay(a: f32, b: f32) -> f32 {
    if a < 0.5 {
        2.0 * a * b
    } else {
        1.0 - 2.0 * (1.0 - a) * (1.0 - b)
    }
}

// ---- Core ultra_glow (single glow pass with falloff/bias) ----

/// Perform a single glow pass: threshold → blur with falloff_power and bias.
/// Returns per-channel glow result (not yet combined with source).
fn glow_pass(
    src_r: &[f32], src_g: &[f32], src_b: &[f32],
    w: usize, h: usize,
    sigma_base: f32,
    size_x: f32, size_y: f32,
    width_r: f32, width_g: f32, width_b: f32,
    brightness_r: f32, brightness_g: f32, brightness_b: f32,
    falloff_power: f32,
    bias: f32,
) -> RenderResult<(Vec<f32>, Vec<f32>, Vec<f32>)> {
    let npx = w * h;

    // falloff_power controls the blur kernel shape.
    // With falloff_power = 10^falloff:
    //   falloff=0 → power=1 (standard Gaussian)
    //   falloff>0 → power>1 (wider, softer tails)
    //   falloff<0 → power<1 (sharper, tighter)
    // We approximate this by scaling sigma: higher power = wider blur.
    // The bias shifts brightness: positive = brighter glow, negative = dimmer
    let sigma_mult = falloff_power.sqrt();
    let bias_mult = (1.0 + bias * 0.3).max(0.01);

    let eff_sigma = sigma_base * sigma_mult;

    // Per-channel, per-direction sigma
    let sigma_rx = eff_sigma * size_x * width_r;
    let sigma_ry = eff_sigma * size_y * width_r;
    let sigma_gx = eff_sigma * size_x * width_g;
    let sigma_gy = eff_sigma * size_y * width_g;
    let sigma_bx = eff_sigma * size_x * width_b;
    let sigma_by = eff_sigma * size_y * width_b;

    // Blur each channel independently
    let glow_r = box_blur_xy(src_r, w, h, sigma_rx, sigma_ry)?;
    let glow_g = box_blur_xy(src_g, w, h, sigma_gx, sigma_gy)?;
    let glow_b = box_blur_xy(src_b, w, h, sigma_bx, sigma_by)?;

    // Apply brightness, color, and bias
    let mut out_r = filled(npx,0.0f32)?;
    let mut out_g = filled(npx,0.0f32)?;
    let mut out_b = filled(npx,0.0f32)?;

    for i in 0..npx {
        out_r[i] = glow_r[i] * brightness_r * bias_mult;
        out_g[i] = glow_g[i] * brightness_g * bias_mult;
        out_b[i] = glow_b[i] * brightness_b * bias_mult;
    }

    Ok((out_r, out_g, out_b))
}

/// AFTER_GLOW pass: screen-blend an additional glow layer onto existing glow.
/// Screen-blend an additional shaped glow layer onto the existing glow.
fn after_glow_pass(
    glow_r: &[f32], glow_g: &[f32], glow_b: &[f32],
    w: usize, h: usize,
    diag: f32,
    amount: f32,       // width multiplier
    stretch_x: f32,    // X stretch (>1 = wider horizontally)
    stretch_y: f32,    // Y stretch (>1 = wider vertically)
    ag_falloff: f32,   // falloff for this pass
    ag_brightness: f32, // brightness for this pass
    ag_color_r: f32, ag_color_g: f32, ag_color_b: f32,
) -> RenderResult<(Vec<f32>, Vec<f32>, Vec<f32>)> {
    if amount < 0.01 || ag_brightness < 0.001 {
        return Ok((copied(glow_r)?,copied(glow_g)?,copied(glow_b)?));
    }

    let npx = w * h;
    let ag_sigma = amount * diag * 0.1;
    let falloff_mult = ag_falloff.sqrt().max(0.1);
    let sigma_x = ag_sigma * stretch_x * falloff_mult;
    let sigma_y = ag_sigma * stretch_y * falloff_mult;

    let blur_r = box_blur_xy(glow_r, w, h, sigma_x, sigma_y)?;
    let blur_g = box_blur_xy(glow_g, w, h, sigma_x, sigma_y)?;
    let blur_b = box_blur_xy(glow_b, w, h, sigma_x, sigma_y)?;

    // Screen-blend the after glow onto the input glow
    let mut out_r = filled(npx,0.0f32)?;
    let mut out_g = filled(npx,0.0f32)?;
    let mut out_b = filled(npx,0.0f32)?;

    for i in 0..npx {
        let ar = blur_r[i] * ag_brightness * ag_color_r;
        let ag = blur_g[i] * ag_brightness * ag_color_g;
        let ab = blur_b[i] * ag_brightness * ag_color_b;
        out_r[i] = blend_screen(glow_r[i], ar);
        out_g[i] = blend_screen(glow_g[i], ag);
        out_b[i] = blend_screen(glow_b[i], ab);
    }

    Ok((out_r, out_g, out_b))
}

// ---- Main onmkGlow pipeline ----

pub fn ultra_glow<T: Channel>(gp: &GlowParams, src: &[T], w: usize, h: usize) -> RenderResult<Vec<T>> {
    if w==0 || h==0 || w>i32::MAX as usize || h>i32::MAX as usize {
        return Err(Error::BadCallbackParameter);
    }
    let npx=w.checked_mul(h).ok_or(Error::OutOfMemory)?;
    let channels=npx.checked_mul(4).ok_or(Error::OutOfMemory)?;
    if src.len()!=channels {return Err(Error::BadCallbackParameter);}
    let diagonal_squared=w.checked_mul(w).and_then(|v|h.checked_mul(h).and_then(|u|v.checked_add(u))).ok_or(Error::OutOfMemory)?;
    let diag = (diagonal_squared as f32).sqrt();

    // 1. Extract channels to f32 [0..1] and apply threshold
    let mut thr_r = filled(npx,0.0f32)?;
    let mut thr_g = filled(npx,0.0f32)?;
    let mut thr_b = filled(npx,0.0f32)?;

    let thresh = gp.threshold;
    // Apply a per-channel offset to the threshold.
    let thresh_r = thresh + gp.threshold_add_color_r;
    let thresh_g = thresh + gp.threshold_add_color_g;
    let thresh_b = thresh + gp.threshold_add_color_b;

    // Smoothstep width (fixed smooth region around threshold)
    let smooth = 0.1_f32;

    for i in 0..npx {
        let off = i * 4;
        let a = src[off].normalized();
        let r = src[off + 1].normalized();
        let g = src[off + 2].normalized();
        let b = src[off + 3].normalized();

        // Glow from alpha interpolation
        let luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;
        let test_val = luma * (1.0 - gp.glow_from_alpha) + a * gp.glow_from_alpha;

        // Per-channel smoothstep threshold
        let mask_r = smoothstep(thresh_r - smooth * 0.5, thresh_r + smooth * 0.5, test_val);
        let mask_g = smoothstep(thresh_g - smooth * 0.5, thresh_g + smooth * 0.5, test_val);
        let mask_b = smoothstep(thresh_b - smooth * 0.5, thresh_b + smooth * 0.5, test_val);

        thr_r[i] = r * mask_r;
        thr_g[i] = g * mask_g;
        thr_b[i] = b * mask_b;
    }

    // Show threshold preview
    if gp.show == 2 {
        let mut out = filled(channels,T::default())?;
        for i in 0..npx {
            let off = i * 4;
            out[off] = src[off]; // alpha
            out[off + 1] = T::rgb(thr_r[i]);
            out[off + 2] = T::rgb(thr_g[i]);
            out[off + 3] = T::rgb(thr_b[i]);
        }
        return Ok(out);
    }

    // 2. Core ultra_glow pass
    // Convert the artist-facing falloff control into an exponential power.
    let falloff_power = 10.0_f32.powf(gp.falloff);

    // Base sigma: width * normalize_scale(4) → width is fraction of diagonal/4
    let base_sigma = gp.width * diag * 0.25;

    let (glow_r, glow_g, glow_b) = glow_pass(
        &thr_r, &thr_g, &thr_b,
        w, h,
        base_sigma,
        gp.width_x, gp.width_y,
        gp.width_red, gp.width_green, gp.width_blue,
        gp.brightness * gp.color_r,
        gp.brightness * gp.color_g,
        gp.brightness * gp.color_b,
        falloff_power,
        gp.bias,
    )?;

    // 3. After-glow shaping pipeline
    // medium_amount = after_glow_width
    // huge_amount = 0.5 * after_glow_width^2
    let medium_amount = gp.after_glow_width;
    let huge_amount = 0.5 * gp.after_glow_width * gp.after_glow_width;

    // After Glow Medium pass
    let ag_med_stretch_x = 1.0 + gp.after_glow_stretch_x;
    let ag_med_stretch_y = 1.0 + gp.after_glow_stretch_y;
    let ag_med_falloff = 0.69_f32;
    let ag_med_brightness = (gp.brightness / 2.0)
        * (1.4 + gp.after_glow_stretch_x + gp.after_glow_stretch_y);

    let (glow_r, glow_g, glow_b) = after_glow_pass(
        &glow_r, &glow_g, &glow_b,
        w, h, diag,
        medium_amount,
        ag_med_stretch_x, ag_med_stretch_y,
        ag_med_falloff, ag_med_brightness,
        gp.after_glow_color_r, gp.after_glow_color_g, gp.after_glow_color_b,
    )?;

    // After Glow Huge pass
    let ag_huge_stretch_x = 1.0 + 2.0 * gp.after_glow_stretch_x;
    let ag_huge_stretch_y = 1.0 + 2.0 * gp.after_glow_stretch_y;
    let ag_huge_falloff = 3.0_f32;
    let ag_huge_brightness = (gp.brightness / 2.0)
        * (2.4 + 2.0 * gp.after_glow_stretch_x + 2.0 * gp.after_glow_stretch_y);

    let (glow_r, glow_g, glow_b) = after_glow_pass(
        &glow_r, &glow_g, &glow_b,
        w, h, diag,
        huge_amount,
        ag_huge_stretch_x, ag_huge_stretch_y,
        ag_huge_falloff, ag_huge_brightness,
        gp.after_glow_color_r, gp.after_glow_color_g, gp.after_glow_color_b,
    )?;

    // Vertical Streaks pass
    let (glow_r, glow_g, glow_b) = after_glow_pass(
        &glow_r, &glow_g, &glow_b,
        w, h, diag,
        gp.vertical_streaks,
        0.089, 3.182,   // very narrow X, very tall Y
        0.44,
        (gp.brightness / 2.0) * 0.714,
        gp.after_glow_color_r, gp.after_glow_color_g, gp.after_glow_color_b,
    )?;

    // Horizontal Streaks pass
    let (glow_r, glow_g, glow_b) = after_glow_pass(
        &glow_r, &glow_g, &glow_b,
        w, h, diag,
        gp.horizontal_streaks,
        3.058, 0.09,   // very wide X, very short Y
        0.44,
        (gp.brightness / 2.0) * 0.714,
        gp.after_glow_color_r, gp.after_glow_color_g, gp.after_glow_color_b,
    )?;

    // 4. Combine with source
    // Combine: 0=Mult, 1=Add, 2=Screen, 3=Difference, 4=Overlay
    let blend_fn = match gp.combine {
        1 => blend_multiply as fn(f32, f32) -> f32,
        2 => blend_add,
        3 => blend_screen,
        4 => blend_difference,
        5 => blend_overlay,
        _ => blend_screen, // default Screen
    };

    let src_opacity = gp.scale_source;
    let affect_alpha = gp.affect_alpha;
    let glow_under = gp.glow_under_source;
    let mix = gp.mix;
    let inv_mix = 1.0 - mix;

    let mut out = filled(channels,T::default())?;

    for i in 0..npx {
        let off = i * 4;
        let orig_a = src[off].normalized();
        let orig_r = src[off + 1].normalized();
        let orig_g = src[off + 2].normalized();
        let orig_b = src[off + 3].normalized();

        let gr = glow_r[i];
        let gg = glow_g[i];
        let gb = glow_b[i];

        // Background (source * scale_source)
        let bg_r = orig_r * src_opacity;
        let bg_g = orig_g * src_opacity;
        let bg_b = orig_b * src_opacity;

        // Blend glow with background
        let (mut out_r, mut out_g, mut out_b) = if glow_under > 0.5 {
            // Glow under source: glow first, then composite source on top
            let sa = orig_a * src_opacity;
            (
                gr * (1.0 - sa) + bg_r,
                gg * (1.0 - sa) + bg_g,
                gb * (1.0 - sa) + bg_b,
            )
        } else {
            // Normal: blend glow on top of source
            (
                blend_fn(bg_r, gr),
                blend_fn(bg_g, gg),
                blend_fn(bg_b, gb),
            )
        };

        // Affect alpha
        let glow_alpha = (gr.max(gg).max(gb)) * affect_alpha;
        let out_a = (orig_a + glow_alpha).min(1.0);

        // Mix with original
        out_r = orig_r * inv_mix + out_r * mix;
        out_g = orig_g * inv_mix + out_g * mix;
        out_b = orig_b * inv_mix + out_b * mix;
        let final_a = orig_a * inv_mix + out_a * mix;

        out[off] = T::alpha(final_a);
        out[off + 1] = T::rgb(out_r);
        out[off + 2] = T::rgb(out_g);
        out[off + 3] = T::rgb(out_b);
    }

    Ok(out)
}

#[inline]
fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0).max(0.001)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_blur_radius_is_rejected_instead_of_wrapping_or_disappearing() {
        for sigma in [f32::NAN,f32::INFINITY,f32::NEG_INFINITY,f32::MAX,1.0e19] {
            assert!(box_blur_xy(&[1.0],1,1,sigma,1.0).is_err(),"sigma={sigma}");
            assert!(box_blur_xy(&[1.0],1,1,1.0,sigma).is_err(),"sigma={sigma}");
        }
    }

    #[test]
    fn invalid_geometry_and_capacity_are_reported() {
        let gp=GlowParams::default();
        for (w,h) in [(0,1),(1,0),(usize::MAX,2),(i32::MAX as usize+1,1)] {
            assert_eq!(ultra_glow::<u8>(&gp,&[],w,h),Err(Error::BadCallbackParameter));
        }
        for len in [0,3,5] {
            assert_eq!(ultra_glow(&gp,&vec![0u16;len],1,1),Err(Error::BadCallbackParameter));
        }
        assert_eq!(filled::<f32>(usize::MAX,0.0),Err(Error::OutOfMemory));
    }

    fn reference_box(src: &[f32],w: usize,h: usize,r: usize,horizontal: bool) -> Vec<f32> {
        (0..w*h).map(|i| {
            let (x,y)=(i%w,i/w);
            let sum:f64=(-(r as isize)..=r as isize).map(|delta| {
                let sx=if horizontal {(x as isize+delta).clamp(0,w as isize-1) as usize} else {x};
                let sy=if horizontal {y} else {(y as isize+delta).clamp(0,h as isize-1) as usize};
                src[sy*w+sx] as f64
            }).sum();
            (sum/(2*r+1) as f64) as f32
        }).collect()
    }

    #[test]
    fn rolling_blur_matches_independent_clamped_tap_reference() {
        for (w,h) in [(1,1),(1,31),(17,1),(17,31),(9,7)] {
            let src:Vec<f32>=(0..w*h).map(|i| ((i*37+11)%97) as f32/16.0-1.25).collect();
            for radius in [0,1,2,8,33,65] {
                for horizontal in [false,true] {
                    let mut actual=vec![0.0;w*h];
                    if horizontal {box_blur_h(&src,&mut actual,w,h,radius)}
                    else {box_blur_v(&src,&mut actual,w,h,radius)}
                    let expected=reference_box(&src,w,h,radius,horizontal);
                    let error=actual.iter().zip(&expected).map(|(a,b)|(a-b).abs()).fold(0.0f32,f32::max);
                    assert!(error < 0.000005,"{w}x{h} radius={radius} horizontal={horizontal} error={error}");
                }
            }
            for (sx,sy) in [(0.0f32,0.0f32),(0.0,2.0),(2.0,0.0),(1.5,3.5),(40.0,65.0)] {
                let mut expected=src.clone();
                let bx=boxes_for_gauss(sx.max(0.5)).unwrap();
                let by=boxes_for_gauss(sy.max(0.5)).unwrap();
                for pass in 0..3 {
                    expected=reference_box(&expected,w,h,if sx>=0.5 {bx[pass]} else {0},true);
                    expected=reference_box(&expected,w,h,if sy>=0.5 {by[pass]} else {0},false);
                }
                let actual=box_blur_xy(&src,w,h,sx,sy).unwrap();
                let error=actual.iter().zip(&expected).map(|(a,b)|(a-b).abs()).fold(0.0f32,f32::max);
                assert!(error < 0.000005,"{w}x{h} sigma={sx},{sy} error={error}");
            }
        }
    }
    #[test]
    fn show_popup_matches_one_based_result_and_threshold() {
        let mut p = GlowParams { show: 1, threshold: 2.0, mix: 0.0, scale_source: 1.0, ..Default::default() };
        let src = [255u8, 200, 100, 50];
        assert_eq!(ultra_glow(&p, &src, 1, 1).unwrap(), src);
        p.show = 2;
        assert_eq!(ultra_glow(&p, &src, 1, 1).unwrap(), [255, 0, 0, 0]);
    }

    #[test]
    fn native_identity_preserves_low_bits_and_hdr() {
        let p = GlowParams { show: 1, mix: 0.0, ..Default::default() };
        let codes: Vec<u16> = (0..=32768).flat_map(|v| [32768, v, v, v]).collect();
        assert_eq!(ultra_glow(&p, &codes, 32769, 1).unwrap(), codes);
        let hdr = [0.375f32, -0.25, 4.125, 0.50001];
        assert_eq!(ultra_glow(&p, &hdr, 1, 1).unwrap(), hdr);
    }

    #[test]
    fn active_glow_retains_sub_byte_levels() {
        let p = GlowParams { show: 1, mix: 1.0, combine: 2, brightness: 1.0,
            color_r: 1.0, color_g: 1.0, color_b: 1.0,
            threshold: -1.0, ..Default::default() };
        let src = [32768u16, 1, 3, 17];
        assert_eq!(ultra_glow(&p, &src, 1, 1).unwrap(), src);
        let hdr = [1.0f32, 2.5, 4.25, 0.50001];
        assert_eq!(ultra_glow(&p, &hdr, 1, 1).unwrap(), hdr);
    }

    #[test]
    fn blur_spreads_sub_byte_glow_outside_source() {
        let p = GlowParams { show: 1, mix: 1.0, combine: 2, brightness: 0.001,
            color_r: 1.0, color_g: 1.0, color_b: 1.0,
            width: 2.0, width_x: 1.0, width_y: 1.0,
            width_red: 1.0, width_green: 1.0, width_blue: 1.0,
            threshold: 0.5, ..Default::default() };
        let src = [32768u16,0,0,0,32768,32768,32768,32768,32768,0,0,0];
        let out = ultra_glow(&p, &src, 3, 1).unwrap();
        assert!(out[1..4].iter().all(|&v| v > 0 && v < 128), "{out:?}");
        assert_eq!(&out[1..4], &out[9..12]);
    }
}
