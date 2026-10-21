//! CPU backend against closed-form fixtures and the independent oracle.
//! Arithmetic/host-free evidence only; this is not After Effects validation.

mod common;

use common::*;
use inner_shadow::engine::{render_with, Image, Route};
use inner_shadow::spec::{Blend, GpuMode, Plan, Rect64, Scale, ShadowParams};

fn cpu(img: &Image, out: &Rect64, p: &ShadowParams, s: &Scale) -> Vec<[f32; 4]> {
    let plan = Plan::new(p, s).unwrap();
    let (px, route) = render_with(img, out, &plan, GpuMode::Cpu, no_gpu).unwrap();
    assert!(matches!(route, Route::Cpu | Route::Identity));
    px
}

fn hard(p: impl FnOnce(&mut ShadowParams)) -> ShadowParams {
    let mut s = ShadowParams {
        blend: Blend::Normal,
        color: [0.0; 3],
        opacity: 1.0,
        angle_deg: 90.0,
        distance: 3.0,
        choke: 0.0,
        size: 0.0,
    };
    p(&mut s);
    s
}

fn square(w: usize, h: usize, x0: i64, y0: i64, x1: i64, y1: i64, alpha: f32) -> Image {
    image(w, h, 0, 0, |x, y| {
        if x >= x0 && x < x1 && y >= y0 && y < y1 {
            [1.0, 1.0, 1.0, alpha]
        } else {
            HIDDEN
        }
    })
}

#[test]
fn angle_direction_selects_the_lit_inner_edge() {
    let img = square(32, 32, 8, 8, 24, 24, 1.0);
    let inside = |x: i64, y: i64| (8..24).contains(&x) && (8..24).contains(&y);
    for (angle, (ox, oy)) in [
        (0.0, (-3, 0)),
        (90.0, (0, 3)),
        (180.0, (3, 0)),
        (270.0, (0, -3)),
    ] {
        let out = cpu(
            &img,
            &img.rect(),
            &hard(|p| p.angle_deg = angle),
            &Scale::FULL,
        );
        for y in 0..32 {
            for x in 0..32 {
                let got = out[(y * 32 + x) as usize];
                if !inside(x, y) {
                    assert_eq!(got, HIDDEN, "hidden RGB must survive at ({x},{y})");
                    continue;
                }
                let shadowed = !inside(x - ox, y - oy);
                let v = if shadowed { 0.0 } else { 1.0 };
                assert_eq!(got, [v, v, v, 1.0], "angle {angle} at ({x},{y})");
            }
        }
    }
}

#[test]
fn fractional_distance_is_bilinear() {
    let img = square(32, 32, 8, 8, 24, 24, 1.0);
    let out = cpu(&img, &img.rect(), &hard(|p| p.distance = 2.5), &Scale::FULL);
    for (y, v) in [(8, 0.0), (9, 0.0), (10, 0.5), (11, 1.0), (20, 1.0)] {
        assert_eq!(out[y * 32 + 16], [v, v, v, 1.0], "row {y}");
    }
}

#[test]
fn partial_alpha_ladder_follows_inverse_alpha_rule() {
    // Distance = Size = 0 keeps the consistent formula M = 1 - alpha.
    let alphas = [0.0f32, 1.0 / 255.0, 0.125, 0.5, 1.0, 1.0 / 32768.0];
    let img = image(alphas.len(), 1, 0, 0, |x, _| {
        let a = alphas[x as usize];
        if a == 0.0 {
            HIDDEN
        } else {
            [1.0, 0.8666667, 0.2, a]
        }
    });
    for opacity in [1.0, 0.5] {
        let p = hard(|p| {
            p.distance = 0.0;
            p.opacity = opacity;
        });
        let out = cpu(&img, &img.rect(), &p, &Scale::FULL);
        let mut expected = Vec::new();
        for (i, &a) in alphas.iter().enumerate() {
            let src = img.pixels[i];
            if a == 0.0 {
                assert_eq!(
                    out[i].map(f32::to_bits),
                    src.map(f32::to_bits),
                    "hidden RGB bit-exact"
                );
                expected.push(src.map(|c| c as f64));
                continue;
            }
            let t = opacity * (1.0 - a as f64);
            let e = [
                src[0] as f64 * (1.0 - t),
                src[1] as f64 * (1.0 - t),
                src[2] as f64 * (1.0 - t),
                a as f64,
            ];
            assert_eq!(out[i][3].to_bits(), a.to_bits(), "alpha unchanged");
            expected.push(e);
        }
        check(&out, &expected, 2e-6).unwrap();
        assert!(
            check(&premultiplied(&out), &expected, 2e-6).is_err(),
            "premultiplied output must fail"
        );
    }
}

#[test]
fn semi_transparent_interior_stays_straight_and_rejects_premultiplied() {
    let img = square(24, 24, 4, 4, 20, 20, 0.5);
    let p = hard(|p| {
        p.blend = Blend::Multiply;
        p.distance = 4.0;
    });
    let out = cpu(&img, &img.rect(), &p, &Scale::FULL);
    let mut expected = Vec::new();
    for y in 0..24i64 {
        for x in 0..24i64 {
            let src = img.get(x, y).unwrap();
            let inside = |x: i64, y: i64| (4..20).contains(&x) && (4..20).contains(&y);
            if !inside(x, y) {
                expected.push(src.map(|c| c as f64));
                continue;
            }
            // Source of the shadow: 1 - alpha at p - o (alpha 0.5 inside).
            let t = if inside(x, y - 4) { 0.5 } else { 1.0 };
            let v = 1.0 - t;
            expected.push([v, v, v, 0.5]);
        }
    }
    assert_eq!(check(&out, &expected, 0.0).unwrap(), 0.0);
    let err = check(&premultiplied(&out), &expected, 1e-3).unwrap_err();
    assert!(err.contains("expected"), "{err}");
}

#[test]
fn holes_cast_shadow_onto_the_ring_below_them() {
    let ring = |x: i64, y: i64| {
        (4..28).contains(&x)
            && (4..28).contains(&y)
            && !((12..20).contains(&x) && (12..20).contains(&y))
    };
    let img = image(
        32,
        32,
        0,
        0,
        |x, y| if ring(x, y) { [1.0; 4] } else { HIDDEN },
    );
    let out = cpu(&img, &img.rect(), &hard(|_| {}), &Scale::FULL);
    for y in 0..32i64 {
        for x in 0..32i64 {
            if !ring(x, y) {
                continue;
            }
            let v = if ring(x, y - 3) { 1.0 } else { 0.0 };
            assert_eq!(out[(y * 32 + x) as usize][0], v, "({x},{y})");
        }
    }
    // The band under the hole exists.
    assert_eq!(out[(21 * 32 + 15) as usize][0], 0.0);
    assert_eq!(out[(23 * 32 + 15) as usize][0], 1.0);
}

#[test]
fn image_exterior_is_transparent_for_full_frame_layers() {
    for (left, top) in [(0, 0), (-5, 7)] {
        let img = image(16, 16, left, top, |_, _| [0.4, 0.6, 0.8, 1.0]);
        let out = cpu(&img, &img.rect(), &hard(|p| p.distance = 2.0), &Scale::FULL);
        for y in 0..16 {
            let v = if y < 2 {
                [0.0, 0.0, 0.0, 1.0]
            } else {
                [0.4, 0.6, 0.8, 1.0]
            };
            assert_eq!(out[y * 16 + 5], v, "row {y} origin ({left},{top})");
        }
    }
}

#[test]
fn opacity_zero_is_bit_exact_identity_including_hdr_and_hidden_rgb() {
    let specials = [
        [4.75f32, -3.5, 0.25, 0.5],
        [f32::from_bits(0x7fc0_0001), 1e-40, -0.0, 1.0],
        [
            1.0 / 32768.0,
            2.0 / 32768.0,
            32767.0 / 32768.0,
            1.0 / 32768.0,
        ],
        [0.3, 0.2, 0.1, 0.0],
        [0.5, 0.5, 0.5, f32::from_bits(0x7fc0_0002)],
    ];
    let img = image(5, 1, 3, -2, |x, _| specials[(x - 3) as usize]);
    let mut plan = Plan::new(
        &ShadowParams {
            opacity: 0.0,
            ..Default::default()
        },
        &Scale::FULL,
    )
    .unwrap();
    for mode in [GpuMode::Auto, GpuMode::Gpu, GpuMode::Cpu] {
        let (out, route) = render_with(
            &img,
            &img.rect(),
            &plan,
            mode,
            |_: &_, _: &_| -> Result<_, String> { panic!("identity must not initialise the GPU") },
        )
        .unwrap();
        assert_eq!(route, Route::Identity);
        for (a, b) in out.iter().zip(&img.pixels) {
            assert_eq!(a.map(f32::to_bits), b.map(f32::to_bits));
        }
    }
    // With an actual shadow, untouched pixels and alpha still keep their bits.
    plan = Plan::new(
        &ShadowParams {
            opacity: 1.0,
            distance: 0.0,
            size: 0.0,
            ..Default::default()
        },
        &Scale::FULL,
    )
    .unwrap();
    let (out, _) = render_with(&img, &img.rect(), &plan, GpuMode::Cpu, no_gpu).unwrap();
    for (a, b) in out.iter().zip(&img.pixels) {
        assert_eq!(a[3].to_bits(), b[3].to_bits(), "alpha bits");
    }
    assert_eq!(
        out[3].map(f32::to_bits),
        img.pixels[3].map(f32::to_bits),
        "alpha 0 keeps hidden RGB"
    );
    assert_eq!(
        out[4].map(f32::to_bits),
        img.pixels[4].map(f32::to_bits),
        "NaN alpha treated as transparent"
    );
}

#[test]
fn hdr_and_negative_rgb_are_not_clamped() {
    let img = image(8, 8, 0, 0, |_, _| [4.0, -0.5, 0.25, 1.0]);
    for (blend, expected) in [
        (Blend::Multiply, [4.0, -0.1, 0.15]),
        (Blend::Normal, [1.6, -0.1, 0.45]),
    ] {
        let p = ShadowParams {
            blend,
            color: [1.0, 0.0, 0.5],
            opacity: 0.8,
            angle_deg: 90.0,
            distance: 100.0,
            choke: 0.0,
            size: 0.0,
        };
        let out = cpu(&img, &img.rect(), &p, &Scale::FULL);
        for px in &out {
            for c in 0..3 {
                assert!(
                    (px[c] - expected[c]).abs() < 1e-6,
                    "{blend:?} channel {c}: {}",
                    px[c]
                );
            }
            assert_eq!(px[3], 1.0);
        }
    }
}

#[test]
fn choke_is_euclidean_matte_erosion_with_fractional_radius() {
    let img = square(40, 40, 6, 6, 34, 34, 1.0);
    let axis = |x: i64, y: i64| (x - 5).min(34 - x).min(y - 5).min(34 - y);
    let out = cpu(
        &img,
        &img.rect(),
        &hard(|p| {
            p.distance = 0.0;
            p.size = 6.0;
            p.choke = 1.0;
        }),
        &Scale::FULL,
    );
    for y in 6..34i64 {
        for x in 6..34i64 {
            let v = if axis(x, y) <= 6 { 0.0 } else { 1.0 };
            assert_eq!(out[(y * 40 + x) as usize][0], v, "({x},{y})");
        }
    }
    let out = cpu(
        &img,
        &img.rect(),
        &hard(|p| {
            p.distance = 0.0;
            p.size = 2.5;
            p.choke = 1.0;
        }),
        &Scale::FULL,
    );
    // Axis distance to the exterior column x = 5: 2 -> both radii hit,
    // 3 -> only radius 3 (mix 0.5), 4 -> neither.
    for (x, v) in [(7, 0.0), (8, 0.5), (9, 1.0)] {
        assert_eq!(out[(20 * 40 + x) as usize][0], v, "x={x}");
    }
}

#[test]
fn blur_matches_discrete_gaussian_cdf() {
    let img = image(80, 80, 0, 0, |x, _| if x >= 40 { [1.0; 4] } else { HIDDEN });
    let out = cpu(
        &img,
        &img.rect(),
        &hard(|p| {
            p.distance = 0.0;
            p.size = 9.0;
        }),
        &Scale::FULL,
    );
    let sigma = 3.0f64;
    let w: Vec<f64> = (-9..=9)
        .map(|i: i64| (-((i * i) as f64) / (2.0 * sigma * sigma)).exp())
        .collect();
    let sum: f64 = w.iter().sum();
    for y in 30..50usize {
        for x in 40..60i64 {
            let m: f64 = (0..19)
                .filter(|&i| x + i - 9 < 40)
                .map(|i| w[i as usize] / sum)
                .sum();
            let got = out[y * 80 + x as usize][0] as f64;
            assert!(
                (got - (1.0 - m)).abs() < 2e-6,
                "({x},{y}) got {got} expected {}",
                1.0 - m
            );
        }
    }
}

#[test]
fn cpu_matches_independent_oracle_across_params_scales_and_tiles() {
    let img = soft_fixture(-7, 5);
    let tiles = [
        img.rect(),
        Rect64::new(-3, 9, 17, 25),
        Rect64::new(-12, 0, 0, 40),
    ];
    let mut worst = 0.0f64;
    for p in valid_cases() {
        for s in scales() {
            for out_rect in tiles {
                let actual = cpu(&img, &out_rect, &p, &s);
                let expected = Oracle::new(&img, p, s).render(&out_rect);
                match check(&actual, &expected, 2e-5) {
                    Ok(e) => worst = worst.max(e),
                    Err(e) if e.contains("partial alpha") => continue,
                    Err(e) => panic!("{p:?} {s:?} {out_rect:?}: {e}"),
                }
                if expected
                    .iter()
                    .zip(&actual)
                    .any(|(e, a)| e[3] > 0.01 && e[3] < 0.99 && a[0] > 0.05)
                {
                    assert!(
                        check(&premultiplied(&actual), &expected, 2e-5).is_err(),
                        "premultiplied must fail: {p:?}"
                    );
                }
            }
        }
    }
    eprintln!("CPU vs oracle worst |error| = {worst:e}");
}

#[test]
fn tiles_and_input_crops_equal_the_whole_render_bit_exactly() {
    let img = soft_fixture(-13, 7);
    let p = valid_cases()[1];
    for s in scales() {
        let whole = img.rect();
        let full = cpu(&img, &whole, &p, &s);
        let at =
            |x: i64, y: i64| full[((y - whole.top) * whole.width() + (x - whole.left)) as usize];
        for tile in [
            Rect64::new(-13, 7, 0, 20),
            Rect64::new(0, 20, 23, 35),
            Rect64::new(5, 9, 6, 10),
            Rect64::new(-20, 0, -12, 9),
        ] {
            let part = cpu(&img, &tile, &p, &s);
            for y in tile.top..tile.bottom {
                for x in tile.left..tile.right {
                    let got = part[((y - tile.top) * tile.width() + (x - tile.left)) as usize];
                    let want = img.get(x, y).map(|_| at(x, y)).unwrap_or([0.0; 4]);
                    assert_eq!(
                        got.map(f32::to_bits),
                        want.map(f32::to_bits),
                        "{s:?} tile {tile:?} ({x},{y})"
                    );
                }
            }
        }
        // A host may return only the non-transparent bounds of the input.
        let (mut l, mut t, mut r, mut b) = (i64::MAX, i64::MAX, i64::MIN, i64::MIN);
        for y in whole.top..whole.bottom {
            for x in whole.left..whole.right {
                if img.get(x, y).unwrap()[3] > 0.0 {
                    (l, t, r, b) = (l.min(x), t.min(y), r.max(x + 1), b.max(y + 1));
                }
            }
        }
        let crop = image((r - l) as usize, (b - t) as usize, l, t, |x, y| {
            img.get(x, y).unwrap()
        });
        let cropped = cpu(&crop, &whole, &p, &s);
        for (i, (a, c)) in full.iter().zip(&cropped).enumerate() {
            if a[3] > 0.0 {
                assert_eq!(
                    a.map(f32::to_bits),
                    c.map(f32::to_bits),
                    "crop mismatch at {i} {s:?}"
                );
            }
        }
    }
}

#[test]
fn downsample_and_pixel_aspect_scale_buffer_distances() {
    let img = square(40, 20, 8, 4, 32, 16, 1.0);
    let out = cpu(
        &img,
        &img.rect(),
        &hard(|p| {
            p.angle_deg = 0.0;
            p.distance = 8.0;
        }),
        &Scale::new(1.0, 1.0, 2.0).unwrap(),
    );
    for x in 8..32usize {
        let v = if x >= 28 { 0.0 } else { 1.0 };
        assert_eq!(out[10 * 40 + x][0], v, "PAR 2 x={x}");
    }
    let out = cpu(
        &img,
        &img.rect(),
        &hard(|p| p.distance = 8.0),
        &Scale::new(0.5, 0.5, 1.0).unwrap(),
    );
    for y in 4..16usize {
        let v = if y < 8 { 0.0 } else { 1.0 };
        assert_eq!(out[y * 40 + 20][0], v, "downsample 1/2 y={y}");
    }
    // Approximate consistency: half-resolution render of a 2x-downsampled
    // input vs. the full-resolution render box-filtered (not exact).
    let big = square(64, 64, 16, 16, 48, 48, 1.0);
    let small = square(32, 32, 8, 8, 24, 24, 1.0);
    let p = ShadowParams {
        blend: Blend::Normal,
        color: [0.0; 3],
        opacity: 1.0,
        angle_deg: 120.0,
        distance: 6.0,
        choke: 0.25,
        size: 8.0,
    };
    let full = cpu(&big, &big.rect(), &p, &Scale::FULL);
    let half = cpu(
        &small,
        &small.rect(),
        &p,
        &Scale::new(0.5, 0.5, 1.0).unwrap(),
    );
    let mut err = 0.0f64;
    for y in 0..32usize {
        for x in 0..32usize {
            let avg: f64 = [(0, 0), (1, 0), (0, 1), (1, 1)]
                .iter()
                .map(|(i, j)| full[(2 * y + j) * 64 + 2 * x + i][0] as f64)
                .sum::<f64>()
                / 4.0;
            err += (avg - half[y * 32 + x][0] as f64).abs();
        }
    }
    let mean = err / 1024.0;
    eprintln!("downsample consistency mean |error| = {mean:.4}");
    assert!(mean < 0.05, "half-resolution result drifted: {mean}");
}

#[test]
fn degenerate_inputs() {
    let p = ShadowParams {
        opacity: 1.0,
        ..Default::default()
    };
    let img = image(1, 1, 0, 0, |_, _| [1.0; 4]);
    let (v, route) = render_with(
        &img,
        &Rect64::new(4, 4, 4, 9),
        &Plan::new(&p, &Scale::FULL).unwrap(),
        GpuMode::Auto,
        no_gpu,
    )
    .unwrap();
    assert!(v.is_empty());
    assert_eq!(route, Route::Identity);
    let out = cpu(&img, &img.rect(), &hard(|p| p.distance = 0.0), &Scale::FULL);
    assert_eq!(out, vec![[1.0; 4]]);
    let out = cpu(&img, &img.rect(), &hard(|p| p.distance = 1.0), &Scale::FULL);
    assert_eq!(out, vec![[0.0, 0.0, 0.0, 1.0]]);
    let empty = Image::new(0, 0, 0, 0, Vec::new()).unwrap();
    assert_eq!(
        cpu(&empty, &Rect64::new(0, 0, 3, 3), &p, &Scale::FULL),
        vec![[0.0; 4]; 9]
    );
    let big = image(12, 12, 0, 0, |_, _| [1.0; 4]);
    let out = cpu(
        &big,
        &big.rect(),
        &hard(|p| p.distance = 1000.0),
        &Scale::FULL,
    );
    assert!(out.iter().all(|px| *px == [0.0, 0.0, 0.0, 1.0]));
    let out = cpu(&big, &big.rect(), &hard(|p| p.size = 60.0), &Scale::FULL);
    assert!(out
        .iter()
        .all(|px| px[0] >= 0.0 && px[0] < 1.0 && px[3] == 1.0));
}

#[test]
fn concurrent_cpu_renders_are_deterministic() {
    let img = soft_fixture(2, -3);
    let p = valid_cases()[2];
    let reference = cpu(&img, &img.rect(), &p, &Scale::FULL);
    std::thread::scope(|scope| {
        let handles: Vec<_> = (0..8)
            .map(|_| scope.spawn(|| cpu(&img, &img.rect(), &p, &Scale::FULL)))
            .collect();
        for h in handles {
            assert_eq!(h.join().unwrap(), reference);
        }
    });
}
