use inner_shadow::{
    engine::{render, Image, Route},
    spec::{Blend, GpuMode, Plan, Scale, ShadowParams},
};
use std::{fs, io::Write, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("provide verification directory")?,
    );
    let data = fs::read(root.join("input.rgba32f"))?;
    let px: Vec<[f32; 4]> = data
        .chunks_exact(16)
        .map(|p| {
            std::array::from_fn(|i| f32::from_le_bytes(p[i * 4..i * 4 + 4].try_into().unwrap()))
        })
        .collect();
    let img = Image::new(640, 360, 0, 0, px).map_err(|e| format!("{e:?}"))?;
    let zero_offset = std::env::args().nth(2).as_deref() == Some("d0c0");
    let out = root.join(if zero_offset {
        "d0c0-gallery"
    } else {
        "random-gallery"
    });
    fs::create_dir_all(&out)?;
    let mut manifest = fs::File::create(out.join("parameters.tsv"))?;
    writeln!(
        manifest,
        "id\tangle\tdistance\tsize\tchoke\topacity\tblend\tr\tg\tb"
    )?;
    let mut seed = 20261021u32;
    let mut random = || {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        (seed >> 8) as f64 / 16777216.0
    };
    let colors = [
        [0.0, 0.0, 0.0],
        [0.12, 0.04, 0.3],
        [0.02, 0.26, 0.3],
        [1.0, 0.93, 0.7],
        [0.5, 0.04, 0.06],
    ];
    for id in 1..=if zero_offset { 6 } else { 9 } {
        let angle = (random() * 360.0).round();
        let distance = (random() * 36.0 + 2.0).round();
        let size = (random() * 46.0 + 4.0).round();
        let choke = (random() * 85.0).round() / 100.0;
        let opacity = (random() * 45.0 + 55.0).round() / 100.0;
        let color = colors[(random() * colors.len() as f64) as usize];
        let mut p = ShadowParams {
            angle_deg: angle,
            distance: if id <= 2 { 0.0 } else { distance },
            size: if id <= 2 { (size / 3.0).round() } else { size },
            choke: if id <= 3 { 1.0 } else { choke },
            opacity,
            color,
            blend: if id % 2 == 0 {
                Blend::Normal
            } else {
                Blend::Multiply
            },
        };
        if zero_offset {
            p.distance = 0.0;
            p.choke = 0.0;
            p.size = [0.0, 5.0, 10.0, 20.0, 40.0, 80.0][id - 1];
            p.opacity = 1.0;
            p.color = [0.0; 3];
            p.blend = Blend::Multiply;
        }
        let plan = Plan::new(&p, &Scale::FULL).map_err(|e| format!("{e:?}"))?;
        let (pixels, route) =
            render(&img, &img.rect(), &plan, GpuMode::Gpu).map_err(|e| format!("{e:?}"))?;
        if !matches!(route, Route::Gpu(_)) {
            return Err("GPU did not execute".into());
        }
        let mut ppm = fs::File::create(out.join(format!("{id:02}.ppm")))?;
        write!(ppm, "P6\n640 360\n255\n")?;
        for (i, p) in pixels.iter().enumerate() {
            let bg = if ((i % 640) / 16 + (i / 640) / 16) % 2 == 0 {
                0.85
            } else {
                0.65
            };
            ppm.write_all(&[0, 1, 2].map(|c| {
                ((p[c] * p[3] + bg * (1.0 - p[3])).clamp(0.0, 1.0) * 255.0).round() as u8
            }))?;
        }
        writeln!(
            manifest,
            "{id}\t{}\t{}\t{}\t{}\t{}\t{:?}\t{}\t{}\t{}",
            p.angle_deg,
            p.distance,
            p.size,
            p.choke * 100.0,
            p.opacity * 100.0,
            p.blend,
            p.color[0],
            p.color[1],
            p.color[2]
        )?;
        println!("{id:02}: {route:?}");
    }
    Ok(())
}
