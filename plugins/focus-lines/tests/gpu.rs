//! GPU backend checks. Each GPU result is compared with an independent
//! expectation, not only with the CPU backend. Without an adapter the tests
//! print SKIPPED (not a pass); set FOCUSLINES_REQUIRE_GPU=1 to make that fatal.

use focus_lines::backend::{self, Backend, Used};
use focus_lines::layout::{self, Mapping, Settings};
use focus_lines::raster::{self, Hole, PixelRect, Prim, Scene};

fn gpu_or_skip(test: &str) -> bool {
    match focus_lines::gpu::adapter_info() {
        Ok(info) => {
            eprintln!("{test}: GPU {info}");
            true
        }
        Err(e) => {
            if std::env::var("FOCUSLINES_REQUIRE_GPU").as_deref() == Ok("1") {
                panic!("{test}: GPU required but unavailable: {e}");
            }
            eprintln!("{test}: SKIPPED (no GPU): {e}");
            false
        }
    }
}

fn gpu(scene: &Scene, rect: PixelRect, samples: u32) -> Vec<f32> {
    let c = backend::coverage(scene, rect, samples, Backend::Gpu)
        .expect("forced GPU must not fail silently");
    assert_eq!(c.used, Used::Gpu);
    c.values
}

fn quad(x0: f64, y0: f64, x1: f64, y1: f64) -> Prim {
    Prim::from_points(&[[x0, y0], [x1, y0], [x1, y1], [x0, y1]]).unwrap()
}

#[test]
fn gpu_exact_rect_union_and_hole() {
    if !gpu_or_skip("gpu_exact_rect_union_and_hole") {
        return;
    }
    // Closed-form expectation, independent of both rasterizers.
    let p = quad(10.25, 5.0, 20.5, 9.0);
    let scene = Scene {
        prims: vec![p, p, p],
        hole: None,
    };
    let r = PixelRect {
        x0: 0,
        y0: 0,
        w: 32,
        h: 16,
    };
    let cov = gpu(&scene, r, 8);
    for y in 0..16 {
        for x in 0..32 {
            let ex = ((x as f64 + 1.0).min(20.5) - (x as f64).max(10.25)).max(0.0);
            let e = if (5..9).contains(&y) { ex } else { 0.0 };
            assert!((f64::from(cov[y * 32 + x]) - e).abs() < 1e-5, "({x},{y})");
        }
    }
    let hole = Hole {
        c: [32.0, 32.0],
        m: [[1.0 / 12.0, 0.0], [0.0, 1.0 / 12.0]],
    };
    let scene = Scene {
        prims: vec![quad(0.0, 0.0, 64.0, 64.0)],
        hole: Some(hole),
    };
    let cov = gpu(
        &scene,
        PixelRect {
            x0: 0,
            y0: 0,
            w: 64,
            h: 64,
        },
        8,
    );
    let total: f64 = cov.iter().map(|&v| f64::from(v)).sum();
    let expected = 4096.0 - std::f64::consts::PI * 144.0;
    assert!((total - expected).abs() / expected < 0.005, "{total}");
    assert_eq!(cov[32 * 64 + 32], 0.0);
}

#[test]
fn gpu_matches_cpu_on_default_scene_and_rois() {
    if !gpu_or_skip("gpu_matches_cpu_on_default_scene_and_rois") {
        return;
    }
    let m = Mapping::new(960.0, 540.0, 1.0, 1.0, 1.0);
    let s = Settings {
        line_width: 6.0,
        ..Settings::default()
    };
    let (scene, _) = layout::build_scene(&s, &m, 0);
    for (rect, samples) in [
        (
            PixelRect {
                x0: 0,
                y0: 0,
                w: 960,
                h: 540,
            },
            8,
        ),
        (
            PixelRect {
                x0: -13,
                y0: -7,
                w: 333,
                h: 211,
            },
            16,
        ),
        (
            PixelRect {
                x0: 700,
                y0: 400,
                w: 260,
                h: 140,
            },
            8,
        ),
    ] {
        let cpu = raster::cpu_render(&scene, rect, samples).unwrap();
        let g = gpu(&scene, rect, samples);
        assert_eq!(cpu.len(), g.len());
        let mut sum = 0.0f64;
        let mut max = 0.0f32;
        let mut over = 0usize;
        for (a, b) in cpu.iter().zip(&g) {
            let d = (a - b).abs();
            sum += f64::from(d);
            max = max.max(d);
            if d > 1e-3 {
                over += 1;
            }
        }
        let mean = sum / cpu.len() as f64;
        eprintln!(
            "{rect:?} S={samples}: mean {mean:.3e} max {max:.3e} >1e-3: {over}/{}",
            cpu.len()
        );
        assert!(mean <= 1e-4, "mean {mean}");
        assert!(over as f64 <= cpu.len() as f64 * 1e-3, "{over}");
        assert!(max <= 1.0 / samples as f32 + 1e-3, "max {max}");
    }
}

#[test]
fn gpu_empty_scene_renders_zero() {
    if !gpu_or_skip("gpu_empty_scene_renders_zero") {
        return;
    }
    let g = gpu(
        &Scene::default(),
        PixelRect {
            x0: 5,
            y0: 5,
            w: 40,
            h: 20,
        },
        8,
    );
    assert!(g.iter().all(|&v| v == 0.0));
}

#[test]
fn gpu_preserves_gaps_with_more_than_eight_subpixel_intervals() {
    if !gpu_or_skip("many_subpixel_intervals") {
        return;
    }
    let prims = (0..12)
        .map(|i| {
            let x = i as f64 / 12.0;
            quad(x, 0.0, x + 0.025, 1.0)
        })
        .collect();
    let scene = Scene { prims, hole: None };
    let rect = PixelRect {
        x0: 0,
        y0: 0,
        w: 1,
        h: 1,
    };
    let value = gpu(&scene, rect, 8)[0];
    assert!(
        (value - 0.3).abs() < 1e-5,
        "12 disjoint slivers must cover 0.3, got {value}"
    );
}

#[test]
fn round_tips_match_cpu_with_opaque_interiors() {
    if !gpu_or_skip("round_tips") {
        return;
    }
    let s = Settings {
        line_count: 12.0,
        line_width: 20.0,
        tip_width: 100.0,
        round_tip: true,
        taper_length: 0.0,
        ..Settings::default()
    };
    let m = Mapping::new(320.0, 180.0, 1.0, 1.0, 1.0);
    let (scene, _) = layout::build_scene(&s, &m, 0);
    let rect = PixelRect {
        x0: 0,
        y0: 0,
        w: 320,
        h: 180,
    };
    let cpu = raster::cpu_render(&scene, rect, 16).unwrap();
    let g = gpu(&scene, rect, 16);
    assert!(g.iter().any(|&a| a > 0.99));
    assert_eq!(g[90 * 320 + 160], 0.0);
    let max = cpu
        .iter()
        .zip(g)
        .map(|(a, b)| (a - b).abs())
        .fold(0.0f32, f32::max);
    assert!(max < 0.002, "max {max}");
}
