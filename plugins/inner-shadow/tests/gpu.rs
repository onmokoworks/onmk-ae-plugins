//! Hardware GPU backend against the same independent expectations as the CPU.
//! Set INNER_SHADOW_REQUIRE_GPU=1 to make an unavailable hardware adapter a
//! test failure instead of a reported skip.

mod common;

use common::*;
use inner_shadow::engine::{render, render_with, Image, Route};
use inner_shadow::gpu;
use inner_shadow::spec::{Blend, GpuMode, Plan, Rect64, Scale, ShadowParams};

fn gpu_available() -> bool {
    let required = std::env::var("INNER_SHADOW_REQUIRE_GPU")
        .map(|v| v == "1")
        .unwrap_or(false);
    match gpu::adapter_identity() {
        Ok(id) => {
            eprintln!("GPU adapter: {id}");
            true
        }
        Err(e) if required => {
            panic!("hardware GPU required (INNER_SHADOW_REQUIRE_GPU=1) but unavailable: {e}")
        }
        Err(e) => {
            eprintln!("GPU test NOT exercised (skip): {e}");
            false
        }
    }
}

/// Forced GPU render; asserts the GPU really ran.
fn on_gpu(img: &Image, out: &Rect64, p: &ShadowParams, s: &Scale) -> Vec<[f32; 4]> {
    let plan = Plan::new(p, s).unwrap();
    let (px, route) = render(img, out, &plan, GpuMode::Gpu).expect("forced GPU render failed");
    match route {
        Route::Gpu(run) => assert!(run.dispatches >= 3, "GPU route without dispatches: {run:?}"),
        other => panic!("expected GPU route, got {other:?}"),
    }
    px
}

fn on_cpu(img: &Image, out: &Rect64, p: &ShadowParams, s: &Scale) -> Vec<[f32; 4]> {
    let plan = Plan::new(p, s).unwrap();
    render_with(img, out, &plan, GpuMode::Cpu, no_gpu)
        .unwrap()
        .0
}

#[test]
fn gpu_closed_form_bands_fraction_and_straight_alpha() {
    if !gpu_available() {
        return;
    }
    let sq = |a: f32| {
        image(32, 32, 0, 0, move |x, y| {
            if (8..24).contains(&x) && (8..24).contains(&y) {
                [1.0, 1.0, 1.0, a]
            } else {
                HIDDEN
            }
        })
    };
    let hard = |angle: f64, distance: f64| ShadowParams {
        blend: Blend::Normal,
        color: [0.0; 3],
        opacity: 1.0,
        angle_deg: angle,
        distance,
        choke: 0.0,
        size: 0.0,
    };
    let img = sq(1.0);
    for (angle, (ox, oy)) in [
        (0.0, (-3i64, 0i64)),
        (90.0, (0, 3)),
        (180.0, (3, 0)),
        (270.0, (0, -3)),
    ] {
        let out = on_gpu(&img, &img.rect(), &hard(angle, 3.0), &Scale::FULL);
        for y in 0..32i64 {
            for x in 0..32i64 {
                let inside = |x: i64, y: i64| (8..24).contains(&x) && (8..24).contains(&y);
                let want = if !inside(x, y) {
                    HIDDEN
                } else if inside(x - ox, y - oy) {
                    [1.0; 4]
                } else {
                    [0.0, 0.0, 0.0, 1.0]
                };
                assert_eq!(
                    out[(y * 32 + x) as usize].map(f32::to_bits),
                    want.map(f32::to_bits),
                    "angle {angle} ({x},{y})"
                );
            }
        }
    }
    let out = on_gpu(&img, &img.rect(), &hard(90.0, 2.5), &Scale::FULL);
    assert_eq!(out[10 * 32 + 16], [0.5, 0.5, 0.5, 1.0]);
    // Semi-transparent straight interior: 0.5, never the premultiplied 0.25.
    let img = sq(0.5);
    let out = on_gpu(
        &img,
        &img.rect(),
        &ShadowParams {
            blend: Blend::Multiply,
            ..hard(90.0, 4.0)
        },
        &Scale::FULL,
    );
    assert_eq!(out[16 * 32 + 16], [0.5, 0.5, 0.5, 0.5]);
    assert_eq!(out[9 * 32 + 16], [0.0, 0.0, 0.0, 0.5]);
}

#[test]
fn gpu_matches_independent_oracle_and_cpu() {
    if !gpu_available() {
        return;
    }
    let img = soft_fixture(-7, 5);
    let tiles = [
        img.rect(),
        Rect64::new(-3, 9, 17, 25),
        Rect64::new(-12, 0, 0, 40),
    ];
    let (mut worst_oracle, mut worst_cpu) = (0.0f64, 0.0f64);
    for p in valid_cases() {
        for s in scales() {
            for out_rect in tiles {
                let g = on_gpu(&img, &out_rect, &p, &s);
                let c = on_cpu(&img, &out_rect, &p, &s);
                let expected = Oracle::new(&img, p, s).render(&out_rect);
                match check(&g, &expected, 5e-5) {
                    Ok(e) => worst_oracle = worst_oracle.max(e),
                    Err(e) if e.contains("partial alpha") => {}
                    Err(e) => panic!("GPU vs oracle {p:?} {s:?} {out_rect:?}: {e}"),
                }
                for (a, b) in g.iter().zip(&c) {
                    assert_eq!(
                        a[3].to_bits(),
                        b[3].to_bits(),
                        "alpha must be the input alpha"
                    );
                    for k in 0..3 {
                        worst_cpu = worst_cpu.max((a[k] - b[k]).abs() as f64);
                    }
                }
                assert!(
                    check(&premultiplied(&g), &expected, 5e-5).is_err()
                        || !expected.iter().any(|e| e[3] > 0.01 && e[3] < 0.99)
                );
            }
        }
    }
    eprintln!(
        "GPU vs oracle worst |error| = {worst_oracle:e}; GPU vs CPU worst |error| = {worst_cpu:e}"
    );
    assert!(worst_cpu < 5e-5);
}

#[test]
fn gpu_identity_hdr_and_hidden_rgb_bits() {
    if !gpu_available() {
        return;
    }
    let specials = [
        [4.75f32, -3.5, 0.25, 0.5],
        [f32::from_bits(0x7fc0_0001), 1e-40, -0.0, 1.0],
        [0.3, 0.2, 0.1, 0.0],
        [1.0 / 32768.0, 0.5, 0.5, 0.25],
    ];
    let img = image(4, 1, 0, 0, |x, _| specials[x as usize]);
    let p = ShadowParams {
        blend: Blend::Multiply,
        color: [1.0, 0.0, 0.5],
        opacity: 0.8,
        angle_deg: 90.0,
        distance: 100.0,
        choke: 0.0,
        size: 0.0,
    };
    let out = on_gpu(&img, &img.rect(), &p, &Scale::FULL);
    assert_eq!(
        out[2].map(f32::to_bits),
        specials[2].map(f32::to_bits),
        "alpha 0 hidden RGB"
    );
    assert_eq!(
        out[1][0].to_bits(),
        specials[1][0].to_bits(),
        "non-finite channel kept"
    );
    for (o, s) in out.iter().zip(&specials) {
        assert_eq!(o[3].to_bits(), s[3].to_bits());
    }
    assert!(
        (out[0][0] - 4.75).abs() < 1e-6 && (out[0][1] - (-3.5 * 0.2)).abs() < 1e-6,
        "{:?}",
        out[0]
    );
}

#[test]
fn gpu_concurrent_auto_renders_are_deterministic() {
    if !gpu_available() {
        return;
    }
    let img = soft_fixture(0, 0);
    let p = valid_cases()[1];
    let plan = Plan::new(&p, &Scale::FULL).unwrap();
    let (reference, route) = render(&img, &img.rect(), &plan, GpuMode::Gpu).unwrap();
    assert!(matches!(route, Route::Gpu(_)));
    std::thread::scope(|scope| {
        let handles: Vec<_> = (0..6)
            .map(|_| scope.spawn(|| render(&img, &img.rect(), &plan, GpuMode::Auto).unwrap()))
            .collect();
        for h in handles {
            let (px, route) = h.join().unwrap();
            assert!(
                matches!(route, Route::Gpu(_)),
                "Auto must prefer the hardware GPU: {route:?}"
            );
            assert_eq!(px, reference);
        }
    });
}

#[test]
fn gpu_large_frame_and_wide_kernel() {
    if !gpu_available() {
        return;
    }
    let img = image(640, 360, 0, 0, |x, y| {
        let d = (((x - 320) as f64).powi(2) + ((y - 180) as f64).powi(2)).sqrt();
        [0.9, 0.5, 0.2, (150.0 - d).clamp(0.0, 1.0) as f32]
    });
    let p = ShadowParams {
        blend: Blend::Multiply,
        color: [0.0; 3],
        opacity: 0.75,
        angle_deg: 120.0,
        distance: 20.0,
        choke: 0.35,
        size: 40.0,
    };
    let t = std::time::Instant::now();
    let g = on_gpu(&img, &img.rect(), &p, &Scale::FULL);
    let gpu_ms = t.elapsed().as_secs_f64() * 1e3;
    let t = std::time::Instant::now();
    let c = on_cpu(&img, &img.rect(), &p, &Scale::FULL);
    let cpu_ms = t.elapsed().as_secs_f64() * 1e3;
    let worst = g
        .iter()
        .zip(&c)
        .flat_map(|(a, b)| (0..3).map(move |k| (a[k] - b[k]).abs()))
        .fold(0.0f32, f32::max);
    eprintln!("640x360 S=40 choke=35%: GPU {gpu_ms:.1} ms (incl. first-use costs), CPU {cpu_ms:.1} ms (test profile), max |GPU-CPU| = {worst:e}");
    assert!(worst < 1e-4);
}
