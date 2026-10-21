//! Squared Euclidean distance transform (Felzenszwalb & Huttenlocher), separable 1D passes.

const INF: f32 = 1e20;

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invalid_geometry_leaves_output_untouched() {
        let mut out=[9.0;4];
        assert!(euclidean_dt(&[1.0;3],2,2,&mut out).is_err());
        assert_eq!(out,[9.0;4]);
        assert!(euclidean_dt(&[],usize::MAX,2,&mut out).is_err());
        assert_eq!(out,[9.0;4]);
        assert!(euclidean_dt(&[1.0;4],2,2,&mut out[..3]).is_err());
        assert_eq!(out,[9.0;4]);
        assert!(euclidean_dt(&[],0,7,&mut []).is_ok());
        assert!(scratch::<f32>(usize::MAX).is_err());
    }
    #[test]
    fn distances_match_exhaustive_nearest_seed() {
        for (w,h) in [(1,9),(9,1),(7,9)] {
            for seed in 0..8 {
                let mask:Vec<f32>=(0..w*h).map(|i|if (i*17+seed*13)%19<3 {1.0}else{0.0}).collect();
                let mut out=vec![0.0;w*h];
                euclidean_dt(&mask,w,h,&mut out).unwrap();
                for y in 0..h {for x in 0..w {
                    let mut expected=INF;
                    for sy in 0..h {for sx in 0..w {if mask[sy*w+sx]>0.5 {
                        let dx=x as f32-sx as f32;let dy=y as f32-sy as f32;
                        expected=expected.min((dx*dx+dy*dy).sqrt());
                    }}}
                    assert!((out[y*w+x]-expected).abs()<1e-5,"{w}x{h} seed{seed} at{x},{y}: {} != {expected}",out[y*w+x]);
                }}
            }
        }
    }
}

fn edt_1d_squared(f: &mut [f32], v: &mut [i32], z: &mut [f32], original: &mut [f32]) {
    let n = f.len();
    if n == 0 {
        return;
    }
    original[..n].copy_from_slice(f);
    let mut k = 0;
    v[0] = 0;
    z[0] = -INF;
    z[1] = INF;

    for q in 1..n {
        let fq = f[q];
        let mut s = ((fq + (q as f32).powi(2)) - (f[v[k] as usize] + (v[k] as f32).powi(2)))
            / (2.0 * q as f32 - 2.0 * v[k] as f32);
        while s <= z[k] {
            k -= 1;
            s = ((fq + (q as f32).powi(2)) - (f[v[k] as usize] + (v[k] as f32).powi(2)))
                / (2.0 * q as f32 - 2.0 * v[k] as f32);
        }
        k += 1;
        v[k] = q as i32;
        z[k] = s;
        z[k + 1] = INF;
    }

    k = 0;
    for q in 0..n {
        while z[k + 1] < q as f32 {
            k += 1;
        }
        let d = q as f32 - v[k] as f32;
        f[q] = d * d + original[v[k] as usize];
    }
}

/// `mask` values > 0.5 are foreground seeds. Writes Euclidean distance (pixels) into `dist`.
pub fn euclidean_dt(mask: &[f32], width: usize, height: usize, dist: &mut [f32]) -> Result<(),after_effects::Error> {
    use after_effects::Error;
    let count=width.checked_mul(height).ok_or(Error::OutOfMemory)?;
    if width>i32::MAX as usize || height>i32::MAX as usize || mask.len()!=count || dist.len()!=count {
        return Err(Error::BadCallbackParameter);
    }
    if width == 0 || height == 0 {
        return Ok(());
    }

    let m = width.max(height);
    let mut horiz = scratch::<f32>(count)?;
    let mut v = scratch::<i32>(m)?;
    let mut z = scratch::<f32>(m.checked_add(1).ok_or(Error::OutOfMemory)?)?;
    let mut col = scratch::<f32>(height)?;
    let mut original = scratch::<f32>(m)?;

    for y in 0..height {
        let row = &mut horiz[y * width..(y + 1) * width];
        for x in 0..width {
            let msk = mask[y * width + x];
            row[x] = if msk > 0.5 { 0.0 } else { INF };
        }
        edt_1d_squared(row, &mut v[..width], &mut z[..width + 1], &mut original);
    }

    for x in 0..width {
        for y in 0..height {
            col[y] = horiz[y * width + x];
        }
        edt_1d_squared(&mut col, &mut v[..height], &mut z[..height + 1], &mut original);
        for y in 0..height {
            let d2 = col[y];
            dist[y * width + x] = if d2 >= INF * 0.5 {
                INF
            } else {
                d2.max(0.0).sqrt()
            };
        }
    }
    Ok(())
}

fn scratch<T:Default+Clone>(count:usize)->Result<Vec<T>,after_effects::Error> {
    let mut data=Vec::new();
    data.try_reserve_exact(count).map_err(|_|after_effects::Error::OutOfMemory)?;
    data.resize(count,T::default());
    Ok(data)
}
