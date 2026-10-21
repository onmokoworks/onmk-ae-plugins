//! Opt-in rich looks. Geometric stop clipping and band-sampled dispersion are
//! separate from the analytic (not wave-optics) source/diffraction model.
use crate::{physical::{self, BrightSource, Config}, EffectParams};
use std::f64::consts::PI;

pub fn enabled(ep:&EffectParams)->bool {(18..=20).contains(&ep.style_preset)}

pub fn ghosts(ep:&EffectParams,sources:&[BrightSource],w:usize,h:usize,mut cfg:Config)->Vec<[f32;3]> {
    let mut lens=physical::bundled_lens(ep.lens_preset);
    for s in &mut lens.surfaces {
        if s.is_stop {
            s.blades=ep.aperture_blades.clamp(3,16);
            s.roundness=ep.aperture_roundness.clamp(0.,1.);
            s.rotation=ep.aperture_rotation.to_radians();
            s.edge_bias=ep.pupil_edge_bias.clamp(0.,1.);
        }
    }
    // Average band samples, keeping neutral input energy independent of sample
    // count. This is an RGB-band approximation, not CIE spectral integration.
    let offsets:&[f64]=match ep.spectral_quality {1=>&[0.],3=>&[-40.,-20.,0.,20.,40.],_=>&[-30.,0.,30.]};
    let mut out=vec![[0.0;3];w*h];
    cfg.gain/=offsets.len() as f32;
    for offset in offsets {
        cfg.wavelengths=[650.+offset,550.+offset,450.+offset];
        let band=physical::render(&lens,sources,w,h,cfg);
        for (dst,src) in out.iter_mut().zip(band) {for c in 0..3 {dst[c]+=src[c];}}
    }
    out
}

pub fn source(ep:&EffectParams,sources:&[BrightSource],w:usize,h:usize)->Vec<[f32;3]> {
    let mut out=vec![[0.;3];w*h];
    if w==0 || h==0 {return out;}
    let fov=50_f64.to_radians();
    let vfov=2.*((h as f64/w as f64)*(fov*0.5).tan()).atan();
    let diagonal=(w as f64).hypot(h as f64);
    let unit=(diagonal/367.151).max(0.05);
    let scale=ep.global_scale.max(0.1);
    let blades=ep.aperture_blades.clamp(3,16);
    let rays=if blades%2==0 {blades} else {2*blades};
    let rotation=(ep.aperture_rotation+ep.flare_angle).to_radians();
    let roundness=ep.aperture_roundness.clamp(0.,1.);
    let workers=std::thread::available_parallelism().map_or(1,|v|v.get().min(4));
    let rows=h.div_ceil(workers).max(1);
    for src in sources.iter().take(8) {
        let sx=(0.5+src.angle_x/fov)*w as f64;
        let sy=(0.5+src.angle_y/vfov)*h as f64;
        let axis=(w as f64*0.5-sx,h as f64*0.5-sy);
        let norm=(axis.0*axis.0+axis.1*axis.1+(diagonal*0.03).powi(2)).sqrt();
        let axis=(axis.0/norm,axis.1/norm);
        std::thread::scope(|scope| {
            for (job,chunk) in out.chunks_mut(rows*w).enumerate() {
                scope.spawn(move || {
                    for (i,pixel) in chunk.iter_mut().enumerate() {
                        let dx=(i%w) as f64+0.5-sx;
                        let dy=(job*rows+i/w) as f64+0.5-sy;
                        let r=dx.hypot(dy);
                        let theta=dy.atan2(dx)-rotation;
                        let sector=2.*PI/rays as f64;
                        let a=(theta+sector*0.5).rem_euclid(sector)-sector*0.5;
                        let across=r*a.sin();
                        let along=dx*axis.0+dy*axis.1;
                        let radius=(diagonal*0.23*scale*(ep.glow_radius/100.).max(0.05)).max(1.);
                        let q=(r*r-0.35*along*along)/(radius*radius);
                        let haze=(0.008*(-q*2.).exp()+0.0015*(-q*0.22).exp())*ep.glow_intensity;
                        for c in 0..3 {
                            let wave=[650.,550.,450.][c]/550.;
                            let core_radius=(unit*1.1*scale*(ep.hotspot_size/8.).max(0.05)).max(0.65);
                            let core=(6.*(-0.5*(r/core_radius).powi(2)).exp()
                                +0.18/(1.+(r/(core_radius*3.)).powi(4)))*ep.hotspot_intensity;
                            let width=(unit*0.65+0.009*r)*wave*(1.+roundness*0.7);
                            // Energy spreads along the ray as well as across
                            // it: a long Gaussian stripe otherwise reads as
                            // a drawn spoke after the display transform.
                            let envelope=(-r/(diagonal*0.16*scale)).exp()
                                /(1.+(r/(unit*12.*scale).max(1.)).powf(1.4));
                            let needle=(-0.5*(across/width.max(0.7)).powi(2)).exp();
                            let skirt=(-0.5*(across/(width*3.).max(1.)).powi(2)).exp();
                            let phase=PI*r/(core_radius*3.*wave).max(2.);
                            let airy=if phase.abs()<1e-6 {1.} else {(phase.sin()/phase).powi(2)};
                            let diffraction=ep.starburst_intensity*(0.28*needle+0.045*skirt+0.10*airy)
                                *envelope*(0.3+ep.unified_diffraction.clamp(0.,1.));
                            pixel[c]+=(src.rgb[c] as f64*(core+diffraction*ep.starburst_color[c]
                                +haze*ep.glow_color[c])) as f32;
                        }
                    }
                });
            }
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    fn params(style:i32,w:usize,h:usize)->EffectParams {
        let fov=50_f64.to_radians();
        let vfov=2.*((h as f64/w as f64)*(fov*0.5).tan()).atan();
        let mut ep=EffectParams {style_preset:style,light_x:(0.5-0.16/fov)*w as f64,
            light_y:(0.5-0.07/vfov)*h as f64,ray_grid:64,physical_gain:3000.,
            ghost_normalize:true,max_area_boost:8.,physical_ghost_blur:0.005,physical_blur_passes:3,
            bloom_strength:0.6,bloom_radius:0.018,bloom_passes:3,bloom_octaves:4,bloom_chromatic:true,
            ghost_intensity:1.,global_brightness:1.,global_scale:1.,flare_opacity:1.,transfer_mode:4,
            hotspot_intensity:1.6,hotspot_size:8.,glow_intensity:0.85,glow_radius:100.,
            glow_color:[1.,0.8,0.55],starburst_intensity:0.18,starburst_color:[0.85,0.92,1.],
            unified_diffraction:0.18,aperture_blades:7,aperture_roundness:0.35,pupil_edge_bias:0.45,
            spectral_quality:2,..Default::default()};
        crate::apply_style_preset(&mut ep); ep
    }
    #[test]
    fn aperture_controls_and_spectral_bands_are_live() {
        let mut ep=params(18,128,72);
        let src=[BrightSource {angle_x:-0.16,angle_y:-0.07,rgb:[1.,0.92,0.78]}];
        let cfg=Config {ray_grid:16,gain:120000.,..Config::default()};
        let baseline=ghosts(&ep,&src,128,72,cfg);
        for option in 0..5 {
            let mut p=ep.clone();
            match option {0=>p.aperture_blades=4,1=>p.aperture_roundness=1.,2=>p.aperture_rotation=23.,
                3=>p.pupil_edge_bias=0.,_=>p.spectral_quality=1};
            let result=ghosts(&p,&src,128,72,cfg);
            assert_ne!(baseline,result,"control {option} has no effect");
            assert!(result.iter().flatten().all(|v|v.is_finite() && *v>=0.));
        }
        assert_eq!(baseline,ghosts(&ep,&src,128,72,cfg));
        ep.hotspot_intensity=0.;ep.glow_intensity=0.;ep.starburst_intensity=0.;
        assert!(source(&ep,&src,128,72).iter().flatten().all(|v|*v==0.));
    }
    #[test]
    fn circular_stop_and_neutral_pupil_preserve_reference() {
        let mut ep=params(18,97,55);
        ep.aperture_roundness=1.;ep.pupil_edge_bias=0.;ep.spectral_quality=1;
        let src=[BrightSource{angle_x:-0.16,angle_y:-0.07,rgb:[1.;3]}];
        let cfg=Config{ray_grid:16,..Config::default()};
        assert_eq!(ghosts(&ep,&src,97,55,cfg),physical::render(&physical::bundled_lens(0),&src,97,55,cfg));
    }
    #[test]
    #[ignore="native 640x360 rich/reference composite and CPU/GPU comparison"]
    fn write_rich_preview() {
        let root=std::env::var("ONMK_RICH_OUT").unwrap();
        std::fs::create_dir_all(&root).unwrap();
        let (w,h)=(640,360);
        let mut src=vec![0.0_f32;w*h*4];for p in src.chunks_exact_mut(4) {p[0]=1.;}
        let gpu=crate::gpu::GpuProcessor::new();
        let mut canvas=image::RgbImage::new((w*2) as u32,(h*3) as u32);
        let mut report=String::new();
        for row in 0..3 {
            for column in 0..2 {
                let style=15+row as i32+3*column as i32;
                let ep=params(style,w,h);
                let start=std::time::Instant::now();
                let output=gpu.process_pixels(&ep,&src,w,h);
                let elapsed=start.elapsed();
                let cpu=crate::flare::process_pixels(&ep,&src,w,h);
                let error=cpu.iter().zip(&output).map(|(a,b)|(a-b).abs()).fold(0.0_f32,f32::max);
                assert!(error<2./32768.,"style {style} CPU/GPU {error}");
                assert!(gpu.is_healthy());
                let mut image=image::RgbImage::new(w as u32,h as u32);
                for (i,p) in output.chunks_exact(4).enumerate() {
                    let rgb=image::Rgb([p[1],p[2],p[3]].map(|v|(v.clamp(0.,1.)*255.).round() as u8));
                    image.put_pixel((i%w) as u32,(i/w) as u32,rgb);
                    canvas.put_pixel((column*w+i%w) as u32,(row*h+i/w) as u32,rgb);
                }
                image.save(format!("{root}/look-{style}.png")).unwrap();
                let line=format!("style {style}: {elapsed:?}, CPU/GPU max {error}\n");
                eprint!("{line}");report.push_str(&line);
            }
        }
        canvas.save(format!("{root}/reference-vs-rich.png")).unwrap();
        std::fs::write(format!("{root}/metrics.txt"),report).unwrap();
    }
    #[test]
    #[ignore="9 moving frames per rich lens, deterministic 9-band source and ghost composite"]
    fn write_rich_motion() {
        let root=std::env::var("ONMK_RICH_OUT").unwrap();
        std::fs::create_dir_all(&root).unwrap();
        let (w,h)=(320,180);
        let mut src=vec![0.0_f32;w*h*4];for p in src.chunks_exact_mut(4) {p[0]=1.;}
        let gpu=crate::gpu::GpuProcessor::new();
        let mut gif=image::codecs::gif::GifEncoder::new(std::fs::File::create(format!("{root}/motion.gif")).unwrap());
        gif.set_repeat(image::codecs::gif::Repeat::Infinite).unwrap();
        let mut frames:Vec<Vec<Vec<f32>>>=vec![Vec::new();3];
        for f in 0..9 {
            let mut canvas=image::RgbaImage::new(w as u32,(h*3) as u32);
            for row in 0..3 {
                let mut ep=params(18+row as i32,w,h);
                ep.light_x+=(f as f64-4.)*0.5;
                let output=gpu.process_pixels(&ep,&src,w,h);
                assert!(output.iter().all(|v|v.is_finite()));
                if f==4 {assert_eq!(output,gpu.process_pixels(&ep,&src,w,h));}
                for (i,p) in output.chunks_exact(4).enumerate() {
                    canvas.put_pixel((i%w) as u32,(row*h+i/w) as u32,image::Rgba([p[1],p[2],p[3],1.].map(|v|(v.clamp(0.,1.)*255.).round() as u8)));
                }
                frames[row].push(output);
            }
            gif.encode_frame(image::Frame::from_parts(canvas,0,0,image::Delay::from_numer_denom_ms(100,1))).unwrap();
            eprintln!("rich motion {f}/8");
        }
        let mut report=String::from("9 frames per lens, .5px light motion per frame, 320x180, grid64, nine bands. Temporal second difference (includes real motion):\n");
        for (i,sequence) in frames.iter().enumerate() {
            let mut sum=0.;for window in sequence.windows(3) {for p in 0..w*h*4 {sum+=(window[2][p]-2.*window[1][p]+window[0][p]).abs() as f64;}}
            report.push_str(&format!("{}: {}\n",18+i,sum/(7*w*h*4) as f64));
        }
        std::fs::write(format!("{root}/motion-metrics.txt"),&report).unwrap(); eprintln!("{report}");
    }
}
