//! GPU smoke test and CPU comparison. Core only: this is NOT an AE host render.
//!
//! cargo run --release --offline --example smoke -- [--backend both|cpu|gpu|auto]
//!     [--width 1920] [--height 1080] [--count 220] [--samples 8|16] [--frame 0]
//!     [--out out/smoke]
//!
//! Writes `<out>/<backend>.ppm` (straight output composited over white),
//! `<out>/<backend>_alpha.pgm` and, with `both`, `diff_x16.pgm`.
//! Exit code 2 if a forced GPU run fails.

use focus_lines::backend::{self, Backend, Coverage, Used};
use focus_lines::layout::{self, Mapping, Settings};
use focus_lines::pixel::{self, Shading};
use focus_lines::raster::PixelRect;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Instant;

struct Args {
    backend: String,
    width: u32,
    height: u32,
    count: Option<f64>,
    samples: u32,
    frame: i64,
    out: PathBuf,
}

fn parse() -> Args {
    let mut a = Args {
        backend: "both".into(),
        width: 1920,
        height: 1080,
        count: None,
        samples: 8,
        frame: 0,
        out: PathBuf::from("out/smoke"),
    };
    let mut it = std::env::args().skip(1);
    while let Some(k) = it.next() {
        let v = it.next().unwrap_or_else(|| panic!("missing value for {k}"));
        match k.as_str() {
            "--backend" => a.backend = v,
            "--width" => a.width = v.parse().expect("width"),
            "--height" => a.height = v.parse().expect("height"),
            "--count" => a.count = Some(v.parse().expect("count")),
            "--samples" => a.samples = v.parse().expect("samples"),
            "--frame" => a.frame = v.parse().expect("frame"),
            "--out" => a.out = PathBuf::from(v),
            _ => panic!("unknown argument {k}"),
        }
    }
    a
}

fn write_ppm(path: &Path, w: u32, h: u32, rgb: &[[f32; 3]]) -> std::io::Result<()> {
    let mut f = std::io::BufWriter::new(std::fs::File::create(path)?);
    write!(f, "P6\n{w} {h}\n255\n")?;
    for p in rgb {
        f.write_all(&p.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8))?;
    }
    Ok(())
}

fn write_pgm(path: &Path, w: u32, h: u32, v: &[f32]) -> std::io::Result<()> {
    let mut f = std::io::BufWriter::new(std::fs::File::create(path)?);
    write!(f, "P5\n{w} {h}\n255\n")?;
    let bytes: Vec<u8> = v
        .iter()
        .map(|x| (x.clamp(0.0, 1.0) * 255.0).round() as u8)
        .collect();
    f.write_all(&bytes)
}

fn report(name: &str, c: &Coverage, layout_ms: f64) {
    println!("[{name}] actual backend: {:?}", c.used);
    if let Some(r) = &c.fallback_reason {
        println!("[{name}] Auto fell back to CPU: {r}");
    }
    println!(
        "[{name}] layout {layout_ms:.2} ms, binning {:.2} ms, coverage {:.2} ms",
        c.timing.bin_ms, c.timing.raster_ms
    );
    if let Some(g) = c.timing.gpu {
        println!(
            "[{name}] gpu: upload {:.2} ms ({} B), dispatch+download {:.2} ms ({} B, {} strip(s)), total {:.2} ms",
            g.upload_ms, g.bytes_up, g.dispatch_download_ms, g.bytes_down, g.strips, g.total_ms
        );
    }
}

fn main() {
    let a = parse();
    std::fs::create_dir_all(&a.out).expect("create output dir");
    let mut s = Settings {
        samples: a.samples,
        ..Settings::default()
    };
    if let Some(c) = a.count {
        s.line_count = c;
    }
    let m = Mapping::new(f64::from(a.width), f64::from(a.height), 1.0, 1.0, 1.0);
    let rect = PixelRect {
        x0: 0,
        y0: 0,
        w: a.width,
        h: a.height,
    };
    let t = Instant::now();
    let (scene, stats) = layout::build_scene(&s, &m, a.frame);
    let layout_ms = t.elapsed().as_secs_f64() * 1e3;
    println!(
        "scene {}x{}: {} lines, {} primitives, {} capped, samples {}",
        a.width,
        a.height,
        stats.lines,
        stats.prims,
        stats.capped_lines,
        s.sanitized().samples
    );
    let wants: Vec<(&str, Backend)> = match a.backend.as_str() {
        "cpu" => vec![("cpu", Backend::Cpu)],
        "gpu" => vec![("gpu", Backend::Gpu)],
        "auto" => vec![("auto", Backend::Auto)],
        _ => vec![("cpu", Backend::Cpu), ("gpu", Backend::Gpu)],
    };
    if wants.iter().any(|(_, b)| *b != Backend::Cpu) {
        let t = Instant::now();
        match focus_lines::gpu::adapter_info() {
            Ok(info) => println!(
                "adapter: {info} (init {:.1} ms)",
                t.elapsed().as_secs_f64() * 1e3
            ),
            Err(e) => println!("adapter: unavailable: {e}"),
        }
    }
    let shading = Shading::new(s.color, s.intensity, s.opacity);
    let mut results: Vec<(&str, Vec<f32>)> = Vec::new();
    let mut failed = false;
    for (name, b) in wants {
        // Second run measures warm (reused scratch) timing.
        for pass in ["cold", "warm"] {
            match backend::coverage(&scene, rect, s.sanitized().samples, b) {
                Ok(c) => {
                    report(&format!("{name}/{pass}"), &c, layout_ms);
                    if pass == "warm" {
                        let rgb: Vec<[f32; 3]> = c
                            .values
                            .iter()
                            .map(|&v| pixel::composite_on(pixel::lines_only(v, &shading), [1.0; 3]))
                            .collect();
                        let alpha: Vec<f32> = c
                            .values
                            .iter()
                            .map(|&v| pixel::line_alpha(v, &shading))
                            .collect();
                        write_ppm(&a.out.join(format!("{name}.ppm")), a.width, a.height, &rgb)
                            .unwrap();
                        write_pgm(
                            &a.out.join(format!("{name}_alpha.pgm")),
                            a.width,
                            a.height,
                            &alpha,
                        )
                        .unwrap();
                        if name != "cpu" && c.used == Used::Cpu {
                            println!("[{name}] NOTE: output was produced by the CPU backend");
                        }
                        results.push((name, c.values));
                    }
                }
                Err(e) => {
                    println!("[{name}/{pass}] FAILED: {e}");
                    failed = true;
                    break;
                }
            }
        }
    }
    if let [(_, cpu), (_, gpu)] = results.as_slice() {
        let mut sum = 0.0f64;
        let mut max = 0.0f32;
        let mut over = 0usize;
        let diff: Vec<f32> = cpu
            .iter()
            .zip(gpu)
            .map(|(x, y)| {
                let d = (x - y).abs();
                sum += f64::from(d);
                max = max.max(d);
                if d > 1e-3 {
                    over += 1;
                }
                d * 16.0
            })
            .collect();
        println!(
            "cpu vs gpu: mean |d| {:.3e}, max |d| {:.3e}, pixels |d|>1e-3: {} of {}",
            sum / cpu.len().max(1) as f64,
            max,
            over,
            cpu.len()
        );
        write_pgm(&a.out.join("diff_x16.pgm"), a.width, a.height, &diff).unwrap();
    }
    println!("images written to {}", a.out.display());
    if failed {
        std::process::exit(2);
    }
}
