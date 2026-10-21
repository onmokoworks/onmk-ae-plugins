//! Reproducible pastel concentration-lines preview using the actual GPU core.
use focus_lines::backend::{self, Backend, Used};
use focus_lines::layout::{self, Mapping, Settings};
use focus_lines::pixel::{self, Shading};
use focus_lines::raster::PixelRect;
use std::io::Write;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let out = std::path::Path::new("out/cute-short-range");
    std::fs::create_dir_all(out)?;
    println!("{}", focus_lines::gpu::adapter_info()?);
    // Design coordinates remain 1920x1080; only the preview is downsampled.
    let mapping = Mapping::new(1920.0, 1080.0, 1.0, 0.4, 0.4);
    let rect = PixelRect {
        x0: 0,
        y0: 0,
        w: 768,
        h: 432,
    };
    for frame in 0..72 {
        let settings = Settings {
            focus: Some([960.0, 540.0]),
            center_size: 105.0,
            center_aspect: 1.22,
            line_count: 120.0,
            line_width: 60.0,
            width_random: 75.0,
            tip_width: 35.0,
            round_tip: true,
            taper_length: 50.0,
            taper_shape: 1.0,
            length_random: 50.0,
            short_min: 30.0,
            short_max: 120.0,
            short_mix: 40.0,
            seed: 12,
            angle_jitter: 8.0,
            clustering: 0.0,
            density_variation: 0.0,
            color: [236.0 / 255.0, 161.0 / 255.0, 182.0 / 255.0],
            opacity: 90.0,
            evolution: layout::Evolution::HoldFrames,
            hold_frames: 3,
            change_amount: 100.0,
            stagger: 0.0,
            samples: 16,
            ..Settings::default()
        };
        let (scene, _) = layout::build_scene(&settings, &mapping, i64::from(frame));
        let cov = backend::coverage(&scene, rect, settings.samples, Backend::Gpu)?;
        assert_eq!(cov.used, Used::Gpu);
        let shading = Shading::new(settings.color, settings.intensity, settings.opacity);
        let mut file =
            std::io::BufWriter::new(std::fs::File::create(out.join(format!("{frame:03}.ppm")))?);
        write!(file, "P6\n{} {}\n255\n", rect.w, rect.h)?;
        for value in cov.values {
            let rgb = pixel::composite_on(
                pixel::lines_only(value, &shading),
                [1.0, 249.0 / 255.0, 245.0 / 255.0],
            );
            file.write_all(&rgb.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8))?;
        }
    }
    println!("72 GPU frames, 768x432, 24 fps, 3 second random hold-frame animation");
    Ok(())
}
