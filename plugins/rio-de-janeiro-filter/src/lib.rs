//! Rio Grade - the original bold magenta-to-orange spatial grade.

use after_effects as ae;

#[derive(Eq, PartialEq, Hash, Clone, Copy, Debug)]
enum Params { Strength, Warmth, ShadowCyan, Saturation, Contrast, Fade, Grain, Vignette }

#[derive(Default)]
struct Plugin;
ae::define_effect!(Plugin, (), Params);

impl AdobePluginGlobal for Plugin {
    fn params_setup(&self, params: &mut ae::Parameters<Params>, _in: ae::InData, _out: ae::OutData) -> Result<(), ae::Error> {
        slider(params, Params::Strength, "Strength", 0.0, 100.0, 100.0)?;
        slider(params, Params::Warmth, "Purple / Pink", -100.0, 100.0, 92.0)?;
        slider(params, Params::ShadowCyan, "Warm Bottom", 0.0, 100.0, 78.0)?;
        slider(params, Params::Saturation, "Saturation", 0.0, 200.0, 110.0)?;
        slider(params, Params::Contrast, "Contrast", 0.0, 200.0, 88.0)?;
        slider(params, Params::Fade, "Color Fade", 0.0, 100.0, 24.0)?;
        slider(params, Params::Grain, "Film Grain", 0.0, 100.0, 6.0)?;
        slider(params, Params::Vignette, "Vignette", 0.0, 100.0, 8.0)?;
        Ok(())
    }

    fn handle_command(&mut self, cmd: ae::Command, _in: ae::InData, mut out: ae::OutData, params: &mut ae::Parameters<Params>) -> Result<(), ae::Error> {
        match cmd {
            ae::Command::About => out.set_return_msg("Rio Grade v1.1\rBold purple-pink to orange gradient with a faded retro finish.\rWritten in Rust."),
            ae::Command::Render { in_layer, mut out_layer } => {
                let p = GradeParams::from_ae(params)?;
                let (mut dst, w, h) = layer_to_flat(&in_layer)?;
                grade_in_place(&mut dst, w, h, p)?;
                flat_to_layer(&dst, &mut out_layer, w, h)?;
            }
            _ => {}
        }
        Ok(())
    }
}

fn slider(params: &mut ae::Parameters<Params>, id: Params, name: &str, min: f64, max: f64, default: f64) -> Result<(), ae::Error> {
    params.add(id, name, ae::FloatSliderDef::setup(|f| {
        f.set_valid_min(min as f32); f.set_valid_max(max as f32); f.set_slider_min(min as f32); f.set_slider_max(max as f32);
        f.set_default(default); f.set_precision(1);
    }))
}

#[derive(Clone, Copy)]
struct GradeParams { strength: f32, warmth: f32, shadow_cyan: f32, saturation: f32, contrast: f32, fade: f32, grain: f32, vignette: f32 }

impl GradeParams {
    fn from_ae(p: &ae::Parameters<Params>) -> Result<Self, ae::Error> {
        let get = |id| -> Result<f32, ae::Error> { Ok(p.get(id)?.as_float_slider()?.value() as f32) };
        Ok(Self { strength: get(Params::Strength)? / 100.0, warmth: get(Params::Warmth)? / 100.0,
            shadow_cyan: get(Params::ShadowCyan)? / 100.0, saturation: get(Params::Saturation)? / 100.0,
            contrast: get(Params::Contrast)? / 100.0, fade: get(Params::Fade)? / 100.0,
            grain: get(Params::Grain)? / 100.0, vignette: get(Params::Vignette)? / 100.0 })
    }
}

fn checked_extent(w: usize, h: usize, stride: usize) -> Result<usize, ae::Error> {
    if w == 0 || h == 0 { return Ok(0); }
    let row = w.checked_mul(4).ok_or(ae::Error::OutOfMemory)?;
    if stride < row { return Err(ae::Error::BadCallbackParameter); }
    (h-1).checked_mul(stride).and_then(|v|v.checked_add(row)).ok_or(ae::Error::OutOfMemory)
}

fn copy_rows(src: &[u8], dst: &mut [u8], w: usize, h: usize,
    src_stride: usize, dst_stride: usize, src_reverse: bool, dst_reverse: bool) -> Result<(), ae::Error> {
    let src_extent = checked_extent(w,h,src_stride)?;
    let dst_extent = checked_extent(w,h,dst_stride)?;
    if src.len()<src_extent || dst.len()<dst_extent { return Err(ae::Error::BadCallbackParameter); }
    if w==0 || h==0 { return Ok(()); }
    let row=w*4;
    for y in 0..h {
        let s=(if src_reverse {h-1-y} else {y})*src_stride;
        let d=(if dst_reverse {h-1-y} else {y})*dst_stride;
        dst[d..d+row].copy_from_slice(&src[s..s+row]);
    }
    Ok(())
}

fn validate_world(layer: &ae::Layer) -> Result<(), ae::Error> {
    if layer.width()>i32::MAX as usize || layer.height()>i32::MAX as usize
        || layer.row_bytes()==i32::MIN as isize { return Err(ae::Error::BadCallbackParameter); }
    if layer.width()==0 || layer.height()==0 { return Ok(()); }
    checked_extent(layer.width(),layer.height(),layer.buffer_stride())?;
    // Layer::buffer constructs a slice covering stride * height.
    let len=layer.buffer_stride().checked_mul(layer.height()).ok_or(ae::Error::OutOfMemory)?;
    if len>isize::MAX as usize || unsafe {layer.data_ptr()}.is_null()
        || layer.pixel_format()?!=ae::PixelFormat::Argb32 { return Err(ae::Error::BadCallbackParameter); }
    Ok(())
}

fn layer_to_flat(layer: &ae::Layer) -> Result<(Vec<u8>, usize, usize), ae::Error> {
    validate_world(layer)?;
    let w = layer.width() as usize; let h = layer.height() as usize; let stride = layer.buffer_stride();
    if w==0 || h==0 { return Ok((Vec::new(),w,h)); }
    let row=w.checked_mul(4).ok_or(ae::Error::OutOfMemory)?;
    let count=row.checked_mul(h).ok_or(ae::Error::OutOfMemory)?;
    let mut flat=Vec::new();
    flat.try_reserve_exact(count).map_err(|_|ae::Error::OutOfMemory)?;
    flat.resize(count,0);
    copy_rows(layer.buffer(),&mut flat,w,h,stride,row,layer.row_bytes()<0,false)?;
    Ok((flat, w, h))
}

fn flat_to_layer(flat: &[u8], layer: &mut ae::Layer, w: usize, h: usize) -> Result<(), ae::Error> {
    if (w,h)!=(layer.width(),layer.height()) { return Err(ae::Error::BadCallbackParameter); }
    validate_world(layer)?;
    if w==0 || h==0 { return Ok(()); }
    let stride=layer.buffer_stride(); let reverse=layer.row_bytes()<0;
    let row=w.checked_mul(4).ok_or(ae::Error::OutOfMemory)?;
    copy_rows(flat,layer.buffer_mut(),w,h,row,stride,false,reverse)
}

#[inline] fn clamp(v: f32) -> f32 { v.clamp(0.0, 1.0) }
#[inline] fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
#[inline] fn noise(x: usize, y: usize) -> f32 {
    let mut n = (x as u32).wrapping_mul(374761393).wrapping_add((y as u32).wrapping_mul(668265263));
    n = (n ^ (n >> 13)).wrapping_mul(1274126177); ((n ^ (n >> 16)) as f32 / u32::MAX as f32) * 2.0 - 1.0
}

fn grade_in_place(out: &mut [u8], w: usize, h: usize, p: GradeParams) -> Result<(), ae::Error> {
    let count=w.checked_mul(h).and_then(|v|v.checked_mul(4)).ok_or(ae::Error::OutOfMemory)?;
    if out.len()!=count { return Err(ae::Error::BadCallbackParameter); }
    if p.strength == 0.0 || count==0 { return Ok(()); }
    let cx = w as f32 * 0.5; let cy = h as f32 * 0.5;
    let radius = (cx * cx + cy * cy).sqrt();
    for y in 0..h { for x in 0..w {
        let i = (y * w + x) * 4; let a = out[i];
        if a == 0 { out[i..i+4].fill(0); continue; }
        // AE worlds contain premultiplied RGB. Grade straight colour, then
        // premultiply the result before blending with the original pixels.
        let (mut r, mut g, mut b) = (out[i+1] as f32 / a as f32, out[i+2] as f32 / a as f32, out[i+3] as f32 / a as f32);
        let luma = 0.2126*r + 0.7152*g + 0.0722*b;
        let shadow = 1.0 - smoothstep(0.08, 0.58, luma);

        // Restored original 100% look.  The coefficients were recovered from
        // the preserved input/output reference pair.  This compact model
        // represents its colour matrix, broad vertical dye, coloured toe and
        // vignette without introducing the later blue band.
        let vertical = if h > 1 { y as f32 / (h - 1) as f32 } else { 0.5 };
        let dx = (x as f32-cx)/radius; let dy = (y as f32-cy)/radius;
        let radial = dx*dx + dy*dy;
        let y2 = vertical*vertical; let y3 = y2*vertical; let y4 = y3*vertical;
        let rr = r; let gg = g; let bb = b;
        let restored_r = 0.437541813 + rr*0.780399680 - gg*0.027349813 - bb*0.198127657
            - shadow*0.094211638 - vertical*0.282727361 + y2*1.399030209
            - y3*2.177565336 + y4*0.953084826 - radial*0.081704237
            + rr*radial*0.236932233 - gg*radial*0.794637084 + bb*radial*0.963907659
            + rr*vertical*0.123846494 - gg*vertical*0.140694022
            + bb*vertical*0.082859136 + shadow*vertical*0.018165352;
        let restored_g = -0.039543424 - rr*0.022702133 + gg*0.907627821 - bb*0.009704745
            - shadow*0.001277533 + vertical*0.148391321 - y2*1.038223028
            + y3*1.984744787 - y4*1.005540848 - radial*0.015998440
            + rr*radial*0.018451100 - gg*radial*0.050442450 - bb*radial*0.015374047
            + rr*vertical*0.004221440 - gg*vertical*0.010895689
            + bb*vertical*0.012180086 + shadow*vertical*0.001786224;
        let restored_b = 0.272763342 + rr*0.006014489 - gg*0.104726806 + bb*0.945190847
            - shadow*0.001567904 - vertical*0.251018077 + y2*1.455159664
            - y3*3.369062662 + y4*1.881772876 - radial*0.019803241
            - rr*radial*0.097894102 + gg*radial*0.088282846 - bb*radial*0.027242940
            - rr*vertical*0.005795930 + gg*vertical*0.034121886
            + bb*vertical*0.012009994 + shadow*vertical*0.019313062;

        // Keep the existing controls useful while making their defaults the
        // exact restored state. Colour controls alter the restored delta.
        let colour_scale = ((p.warmth / 0.92) * 0.55 + (p.shadow_cyan / 0.78) * 0.25
            + (p.fade / 0.24) * 0.20).max(0.0);
        r = rr + (restored_r-rr)*colour_scale;
        g = gg + (restored_g-gg)*colour_scale;
        b = bb + (restored_b-bb)*colour_scale;
        let l = 0.2126*r + 0.7152*g + 0.0722*b;
        r = l + (r-l)*(p.saturation/1.10); g = l + (g-l)*(p.saturation/1.10); b = l + (b-l)*(p.saturation/1.10);
        let c = p.contrast/0.88;
        r = 0.5 + (r-0.5)*c; g = 0.5 + (g-0.5)*c; b = 0.5 + (b-0.5)*c;
        let vig_adjust = (1.0-(radial*(p.vignette-0.08)*0.35)).clamp(0.65, 1.0);
        let n = noise(x,y)*p.grain*0.035;
        r = clamp(r*vig_adjust+n); g = clamp(g*vig_adjust+n); b = clamp(b*vig_adjust+n); let m = p.strength;
        let alpha = a as f32 / 255.0;
        r *= alpha; g *= alpha; b *= alpha;
        out[i] = a; out[i+1] = ((out[i+1] as f32/255.0*(1.0-m)+r*m)*255.0+0.5) as u8;
        out[i+2] = ((out[i+2] as f32/255.0*(1.0-m)+g*m)*255.0+0.5) as u8;
        out[i+3] = ((out[i+3] as f32/255.0*(1.0-m)+b*m)*255.0+0.5) as u8;
    }} Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grade(src: &[u8], w: usize, h: usize, p: GradeParams) -> Vec<u8> {
        let mut out=src.to_vec();
        grade_in_place(&mut out,w,h,p).unwrap();
        out
    }

    #[test]
    fn in_place_grade_rejects_invalid_extents_without_mutation() {
        let mut pixel=[255,40,80,120];
        for (w,h) in [(1,2),(usize::MAX,2),(0,1)] {
            assert!(grade_in_place(&mut pixel,w,h,defaults()).is_err());
            assert_eq!(pixel,[255,40,80,120]);
        }
        grade_in_place(&mut [],0,5,defaults()).unwrap();
    }

    #[test]
    fn malformed_rows_are_rejected_before_any_write() {
        let src=[1,2,3,4,5,6,7,8];
        let mut short=[99;7];
        // The former loop wrote the first row and silently skipped the second.
        let mut former=short;
        for y in 0..2 { let d=y*4; if d+4<=former.len() { former[d..d+4].copy_from_slice(&src[d..d+4]); } }
        assert_eq!(former,[1,2,3,4,99,99,99]);
        assert!(copy_rows(&src,&mut short,1,2,4,4,false,false).is_err());
        assert_eq!(short,[99;7]);
        let mut dst=[99;8];
        for (input,w,h,ss,ds) in [(&src[..7],1,2,4,4),(&src[..],2,1,4,8),(&src[..],1,2,4,3),(&src[..],usize::MAX,2,4,4),(&src[..],1,usize::MAX,4,4)] {
            assert!(copy_rows(input,&mut dst,w,h,ss,ds,false,false).is_err());
            assert_eq!(dst,[99;8]);
        }
    }

    #[test]
    fn padded_reversed_rows_round_trip_without_touching_padding() {
        let src=[1,2,3,4,99,99,5,6,7,8];
        let mut flat=[0;8];
        copy_rows(&src,&mut flat,1,2,6,4,true,false).unwrap();
        assert_eq!(flat,[5,6,7,8,1,2,3,4]);
        let mut dst=[99;10];
        copy_rows(&flat,&mut dst,1,2,4,6,false,true).unwrap();
        assert_eq!(dst,src);
        copy_rows(&[],&mut [],0,4,0,0,false,false).unwrap();
    }

    fn defaults() -> GradeParams {
        GradeParams { strength: 1.0, warmth: 0.92, shadow_cyan: 0.78,
            saturation: 1.10, contrast: 0.88, fade: 0.24, grain: 0.0, vignette: 0.0 }
    }

    fn vertical_triplet(value: u8) -> ([u8; 4], [u8; 4], [u8; 4]) {
        let src = [255, value, value, value, 255, value, value, value,
            255, value, value, value];
        let out = grade(&src, 1, 3, defaults());
        ([out[0], out[1], out[2], out[3]], [out[4], out[5], out[6], out[7]],
            [out[8], out[9], out[10], out[11]])
    }

    #[test]
    fn black_toe_is_lifted_and_violet_magenta() {
        let (_, p, _) = vertical_triplet(0);
        assert!(p[1] > p[2]);
        assert!(p[3] > p[2]);
        assert!(p[1] > 20 && p[3] > 20);
    }

    #[test]
    fn top_receives_the_original_magenta_cast() {
        let (top, _, _) = vertical_triplet(128);
        assert!(top[1] > top[2]);
        assert!(top[3] > top[2]);
    }

    #[test]
    fn middle_receives_a_strong_purple_magenta_cast() {
        let (_, middle, _) = vertical_triplet(128);
        assert!(middle[1] >= middle[2] + 20);
        assert!(middle[3] >= middle[2] + 15);
    }

    #[test]
    fn bottom_is_warm_orange() {
        let (_, _, bottom) = vertical_triplet(128);
        assert!(bottom[1] > bottom[3]);
        assert!(bottom[2] > bottom[3]);
    }

    #[test]
    fn partial_alpha_matches_opaque_grade_then_premultiply() {
        let opaque = [255, 255, 0, 255];
        let graded = grade(&opaque, 1, 1, defaults());
        for alpha in [0u8, 1, 64, 128, 254, 255] {
            let src = [alpha, alpha, 0, alpha];
            let output = grade(&src, 1, 1, defaults());
            assert_eq!(output[0], alpha);
            for channel in 1..4 {
                let expected = (graded[channel] as u32 * alpha as u32 + 127) / 255;
                assert!((output[channel] as i32 - expected as i32).abs() <= 1,
                    "alpha={alpha}, channel={channel}: {} vs {expected}", output[channel]);
                assert!(output[channel] <= alpha);
            }
        }
    }

    #[test]
    fn zero_strength_is_byte_exact() {
        let src = [191, 40, 100, 220];
        let mut params = defaults();
        params.strength = 0.0;
        assert_eq!(grade(&src, 1, 1, params), src);
    }
}
