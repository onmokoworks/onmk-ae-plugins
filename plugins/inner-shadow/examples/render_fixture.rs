use inner_shadow::{
    engine::{render, Image, Route},
    spec::{GpuMode, Plan, Scale, ShadowParams},
};
use std::{fs, io::Write, path::PathBuf, time::Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dest = PathBuf::from(
        std::env::args()
            .nth(1)
            .unwrap_or_else(|| "verification".into()),
    );
    fs::create_dir_all(&dest)?;
    let (w, h) = (640usize, 360usize);
    let mut pixels = Vec::with_capacity(w * h);
    for y in 0..h {
        for x in 0..w {
            let circle = 130.0 - ((x as f64 - 180.0).powi(2) + (y as f64 - 180.0).powi(2)).sqrt();
            let rect = (x as f64 - 360.0)
                .min(580.0 - x as f64)
                .min(y as f64 - 60.0)
                .min(300.0 - y as f64);
            let hole = ((x as f64 - 470.0).powi(2) + (y as f64 - 180.0).powi(2)).sqrt() - 55.0;
            let a = circle.max(rect.min(hole)).clamp(0.0, 1.0) as f32;
            pixels.push([0.92, 0.64, 0.28, a]);
        }
    }
    let image = Image::new(w, h, 0, 0, pixels).map_err(|e| format!("{e:?}"))?;
    let params = ShadowParams {
        distance: 16.0,
        size: 24.0,
        choke: 0.2,
        opacity: 0.85,
        ..Default::default()
    };
    let plan = Plan::new(&params, &Scale::FULL).map_err(|e| format!("{e:?}"))?;
    save(&dest.join("input"), w, h, &image.pixels)?;
    for (label, mode) in [("cpu", GpuMode::Cpu), ("gpu", GpuMode::Gpu)] {
        let start = Instant::now();
        let (px, route) =
            render(&image, &image.rect(), &plan, mode).map_err(|e| format!("{e:?}"))?;
        if mode == GpuMode::Gpu && !matches!(route, Route::Gpu(_)) {
            return Err("GPU did not run".into());
        }
        println!(
            "{label}: {:.2} ms; {route:?}",
            start.elapsed().as_secs_f64() * 1000.0
        );
        save(&dest.join(label), w, h, &px)?;
    }
    let hard = ShadowParams {
        distance: 0.0,
        size: 10.0,
        choke: 1.0,
        opacity: 1.0,
        ..Default::default()
    };
    let hard_plan = Plan::new(&hard, &Scale::FULL).map_err(|e| format!("{e:?}"))?;
    let (hard_pixels, hard_route) =
        render(&image, &image.rect(), &hard_plan, GpuMode::Gpu).map_err(|e| format!("{e:?}"))?;
    println!("hard inner edge: {hard_route:?}");
    save(&dest.join("hard-edge"), w, h, &hard_pixels)?;
    Ok(())
}

fn save(base: &std::path::Path, w: usize, h: usize, px: &[[f32; 4]]) -> std::io::Result<()> {
    let mut raw = fs::File::create(base.with_extension("rgba32f"))?;
    let mut ppm = fs::File::create(base.with_extension("ppm"))?;
    write!(ppm, "P6\n{w} {h}\n255\n")?;
    for (i, p) in px.iter().enumerate() {
        for v in p {
            raw.write_all(&v.to_le_bytes())?;
        }
        let bg = if ((i % w) / 16 + (i / w) / 16) % 2 == 0 {
            0.85
        } else {
            0.65
        };
        let rgb = [0, 1, 2]
            .map(|c| ((p[c] * p[3] + bg * (1.0 - p[3])).clamp(0.0, 1.0) * 255.0).round() as u8);
        ppm.write_all(&rgb)?;
    }
    Ok(())
}
