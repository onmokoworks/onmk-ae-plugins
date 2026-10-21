#[derive(Clone, Copy, Debug)]
pub struct EdgeParams {
    pub border: f32,
    pub roughness: f32,
    pub scale: f32,
    pub detail: u32,
    pub balance: f32,
    pub sharpness: f32,
    pub stretch: f32,
    pub angle: f32,
    pub evolution: f32,
    pub seed: u32,
    pub preserve: f32,
    pub mix: f32,
}

impl Default for EdgeParams {
    fn default() -> Self {
        Self {
            border: 0.0,
            roughness: 12.0,
            scale: 48.0,
            detail: 4,
            balance: 0.0,
            sharpness: 70.0,
            stretch: 0.0,
            angle: 0.0,
            evolution: 0.0,
            seed: 0,
            preserve: 50.0,
            mix: 1.0,
        }
    }
}

#[derive(Clone, Copy)]
struct Seed {
    x: i32,
    y: i32,
}

const INF: f32 = 1.0e20;

fn distance_field(
    alpha: &[f32],
    w: usize,
    h: usize,
    feature_inside: bool,
) -> (Vec<f32>, Vec<Seed>) {
    let mut d = vec![INF; w * h];
    let mut seed = vec![Seed { x: -1, y: -1 }; w * h];
    fill_distance_field(|i|alpha[i],w,h,feature_inside,&mut d,&mut seed);
    (d,seed)
}

fn fill_distance_field(alpha:impl Fn(usize)->f32,w:usize,h:usize,feature_inside:bool,d:&mut[f32],seed:&mut[Seed]) {
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            if (alpha(i) >= 0.5) == feature_inside {
                d[i] = 0.0;
                seed[i] = Seed {
                    x: x as i32,
                    y: y as i32,
                };
            }
        }
    }
    let diag = std::f32::consts::SQRT_2;
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            for (dx, dy, c) in [(-1, 0, 1.0), (0, -1, 1.0), (-1, -1, diag), (1, -1, diag)] {
                let nx = x as i32 + dx;
                let ny = y as i32 + dy;
                if nx >= 0 && ny >= 0 && nx < w as i32 && ny < h as i32 {
                    let n = ny as usize * w + nx as usize;
                    if d[n] + c < d[i] {
                        d[i] = d[n] + c;
                        seed[i] = seed[n];
                    }
                }
            }
        }
    }
    for y in (0..h).rev() {
        for x in (0..w).rev() {
            let i = y * w + x;
            for (dx, dy, c) in [(1, 0, 1.0), (0, 1, 1.0), (1, 1, diag), (-1, 1, diag)] {
                let nx = x as i32 + dx;
                let ny = y as i32 + dy;
                if nx >= 0 && ny >= 0 && nx < w as i32 && ny < h as i32 {
                    let n = ny as usize * w + nx as usize;
                    if d[n] + c < d[i] {
                        d[i] = d[n] + c;
                        seed[i] = seed[n];
                    }
                }
            }
        }
    }
}

/// Preserve the CPU's scan-order tie selection without changing GPU alpha/noise.
pub(crate) fn restore_propagated_rgb(input:&[f32],w:usize,h:usize,out:&mut[f32])->Result<(),String> {
    let n=w.checked_mul(h).ok_or("color field dimensions overflow")?;
    let len=n.checked_mul(4).ok_or("color field channels overflow")?;
    if input.len()!=len || out.len()!=len || w>i32::MAX as usize || h>i32::MAX as usize {
        return Err("invalid color field shape".into());
    }
    if n==0 || !input.chunks_exact(4).any(|p|p[3]<=1e-5) {return Ok(());}
    fn plane<T:Clone>(n:usize,value:T)->Result<Vec<T>,String> {
        let mut v=Vec::new();v.try_reserve_exact(n).map_err(|_|"color field allocation failed")?;
        v.resize(n,value);Ok(v)
    }
    let mut d=plane(n,INF)?;
    let mut seed=plane(n,Seed{x:-1,y:-1})?;
    fill_distance_field(|i|input[i*4+3].clamp(0.0,1.0),w,h,true,&mut d,&mut seed);
    for i in 0..n {
        if input[i*4+3].clamp(0.0,1.0)<=1e-5 {
            let s=seed[i];
            let source=if s.x>=0 {s.y as usize*w+s.x as usize} else {i};
            out[i*4..i*4+3].copy_from_slice(&input[source*4..source*4+3]);
        }
    }
    Ok(())
}

fn hash(mut x: u32) -> f32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    x as f32 / u32::MAX as f32
}

fn value_noise(x: f32, y: f32, seed: u32) -> f32 {
    let ix = x.floor() as i32;
    let iy = y.floor() as i32;
    let fx = x - ix as f32;
    let fy = y - iy as f32;
    let u = fx * fx * (3.0 - 2.0 * fx);
    let v = fy * fy * (3.0 - 2.0 * fy);
    let h = |dx: i32, dy: i32| hash((ix + dx) as u32 ^ ((iy + dy) as u32).rotate_left(16) ^ seed);
    let a = h(0, 0) + (h(1, 0) - h(0, 0)) * u;
    let b = h(0, 1) + (h(1, 1) - h(0, 1)) * u;
    (a + (b - a) * v) * 2.0 - 1.0
}

fn fbm(x: f32, y: f32, p: &EdgeParams) -> f32 {
    let r = p.angle.to_radians();
    let cs = r.cos();
    let sn = r.sin();
    let mut px = (x * cs - y * sn) / p.scale.max(1.0);
    let mut py = (x * sn + y * cs) / p.scale.max(1.0)
        * (1.0 - p.stretch.clamp(-99.0, 99.0) / 100.0).max(0.01);
    let mut sum = 0.0;
    let mut amp = 0.5;
    let mut norm = 0.0;
    for o in 0..p.detail.clamp(1, 8) {
        sum += value_noise(
            px + p.evolution * 0.01,
            py + p.evolution * 0.006,
            p.seed.wrapping_add(o * 1013),
        ) * amp;
        norm += amp;
        px *= 2.0;
        py *= 2.0;
        amp *= 0.5;
    }
    sum / norm.max(1.0e-6)
}

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a).max(1.0e-6)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub fn render_rgba(input: &[f32], w: usize, h: usize, p: &EdgeParams) -> Vec<f32> {
    if w == 0 || h == 0 || input.len() < w * h * 4 {
        return Vec::new();
    }
    let alpha: Vec<f32> = (0..w * h)
        .map(|i| input[i * 4 + 3].clamp(0.0, 1.0))
        .collect();
    let (to_inside, inside_seed) = distance_field(&alpha, w, h, true);
    let (to_outside, _) = distance_field(&alpha, w, h, false);
    let mut out = vec![0.0; w * h * 4];
    let softness = (1.0 - p.sharpness.clamp(0.0, 100.0) / 100.0) * 3.5 + 0.35;
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let src = i * 4;
            let signed = if alpha[i] >= 0.5 {
                to_outside[i]
            } else {
                -to_inside[i]
            };
            let n = fbm(x as f32, y as f32, p);
            let biased = if n >= 0.0 {
                n * (1.0 + p.balance / 100.0)
            } else {
                n * (1.0 - p.balance / 100.0)
            };
            let protect = (signed.abs() / (p.preserve * 0.25 + 1.0)).clamp(0.0, 1.0);
            let shaped = signed + p.border + biased * p.roughness * (1.0 - protect * 0.85);
            let edge_alpha = smoothstep(-softness, softness, shaped);
            let final_alpha = alpha[i] + (edge_alpha - alpha[i]) * p.mix;
            let color_i = if alpha[i] > 1.0e-5 {
                i
            } else {
                let s = inside_seed[i];
                if s.x >= 0 {
                    s.y as usize * w + s.x as usize
                } else {
                    i
                }
            } * 4;
            out[src] = input[color_i];
            out[src + 1] = input[color_i + 1];
            out[src + 2] = input[color_i + 2];
            out[src + 3] = final_alpha.clamp(0.0, 1.0);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "explicit 1080p correction timing; not an end-to-end benchmark"]
    fn correction_1080p_timing() {
        let (w,h)=(1920,1080);
        let input:Vec<f32>=(0..w*h).flat_map(|i| {
            let (x,y)=(i%w,i/w);
            [0.2,0.4,0.8,if (320..1600).contains(&x)&&(180..900).contains(&y){1.0}else{0.0}]
        }).collect();
        let mut output=input.clone();
        restore_propagated_rgb(&input,w,h,&mut output).unwrap();
        let start=std::time::Instant::now();
        for _ in 0..5 {restore_propagated_rgb(std::hint::black_box(&input),w,h,std::hint::black_box(&mut output)).unwrap();}
        eprintln!("1080p RGB correction mean_ms={:.3}",start.elapsed().as_secs_f64()*1000.0/5.0);
    }
    #[test]
    fn propagated_color_correction_preserves_alpha_and_checks_shape() {
        let mut out=vec![0.37;4];
        assert!(restore_propagated_rgb(&[1.;3],1,1,&mut out).is_err());
        assert_eq!(out,[0.37;4]);
        for seed in 1..=4u32 {
            let (w,h)=(7,5);let mut rng=seed;
            let input:Vec<f32>=(0..w*h).flat_map(|i| {
                rng=rng.wrapping_mul(1664525).wrapping_add(1013904223);
                [i as f32,-(i as f32),0.8,if rng%4==0 {1.0} else {0.0}]
            }).collect();
            let reference=render_rgba(&input,w,h,&EdgeParams::default());
            let mut output=input.clone();
            for p in output.chunks_exact_mut(4){p[3]=0.37;}
            restore_propagated_rgb(&input,w,h,&mut output).unwrap();
            for (p,r) in output.chunks_exact(4).zip(reference.chunks_exact(4)) {
                assert_eq!(&p[..3],&r[..3]);assert_eq!(p[3],0.37);
            }
        }
    }
    #[test]
    fn deterministic() {
        let p = EdgeParams::default();
        assert_eq!(fbm(12.0, 9.0, &p), fbm(12.0, 9.0, &p));
    }
    #[test]
    fn border_expands_alpha() {
        let mut src = vec![0.0; 9 * 9 * 4];
        for y in 3..6 {
            for x in 3..6 {
                src[(y * 9 + x) * 4 + 3] = 1.0;
            }
        }
        let base: f32 = render_rgba(
            &src,
            9,
            9,
            &EdgeParams {
                roughness: 0.0,
                ..Default::default()
            },
        )
        .iter()
        .skip(3)
        .step_by(4)
        .sum();
        let grown: f32 = render_rgba(
            &src,
            9,
            9,
            &EdgeParams {
                border: 2.0,
                roughness: 0.0,
                ..Default::default()
            },
        )
        .iter()
        .skip(3)
        .step_by(4)
        .sum();
        assert!(grown > base);
    }
    #[test]
    fn zero_mix_preserves_alpha() {
        let src = vec![0.25; 4 * 4 * 4];
        let out = render_rgba(
            &src,
            4,
            4,
            &EdgeParams {
                mix: 0.0,
                ..Default::default()
            },
        );
        for i in 0..16 {
            assert_eq!(out[i * 4 + 3], 0.25);
        }
    }
}
