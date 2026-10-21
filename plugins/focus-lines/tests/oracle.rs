//! Coverage and layout checks against expectations computed independently of
//! the production scanline code (closed forms and dense point sampling).

use focus_lines::layout::{self, Evolution, Mapping, Settings};
use focus_lines::raster::{self, Hole, PixelRect, Prim, Scene};

fn quad(x0: f64, y0: f64, x1: f64, y1: f64) -> Prim {
    Prim::from_points(&[[x0, y0], [x1, y0], [x1, y1], [x0, y1]]).unwrap()
}

fn render(scene: &Scene, rect: PixelRect, samples: u32) -> Vec<f32> {
    raster::cpu_render(scene, rect, samples).unwrap()
}

fn full(w: u32, h: u32) -> PixelRect {
    PixelRect { x0: 0, y0: 0, w, h }
}

/// Independent point-in-convex-polygon test (sign of cross products).
fn inside_convex(p: [f64; 2], poly: &[[f64; 2]]) -> bool {
    let mut sign = 0.0f64;
    for i in 0..poly.len() {
        let a = poly[i];
        let b = poly[(i + 1) % poly.len()];
        let c = (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]);
        if c != 0.0 {
            if sign == 0.0 {
                sign = c.signum();
            } else if c.signum() != sign {
                return false;
            }
        }
    }
    true
}

/// Dense 2D point-sampling oracle: union of polygons minus optional hole.
fn oracle(polys: &[Vec<[f64; 2]>], hole: Option<&Hole>, rect: PixelRect, n: usize) -> Vec<f64> {
    let mut out = vec![0.0; (rect.w * rect.h) as usize];
    for j in 0..rect.h as usize {
        for i in 0..rect.w as usize {
            let mut hit = 0usize;
            for sy in 0..n {
                for sx in 0..n {
                    let p = [
                        f64::from(rect.x0) + i as f64 + (sx as f64 + 0.5) / n as f64,
                        f64::from(rect.y0) + j as f64 + (sy as f64 + 0.5) / n as f64,
                    ];
                    if hole.is_some_and(|h| h.contains(p)) {
                        continue;
                    }
                    if polys.iter().any(|poly| inside_convex(p, poly)) {
                        hit += 1;
                    }
                }
            }
            out[j * rect.w as usize + i] = hit as f64 / (n * n) as f64;
        }
    }
    out
}

fn shoelace(poly: &[[f64; 2]]) -> f64 {
    let mut a = 0.0;
    for i in 0..poly.len() {
        let p = poly[i];
        let q = poly[(i + 1) % poly.len()];
        a += p[0] * q[1] - q[0] * p[1];
    }
    a.abs() * 0.5
}

#[test]
fn axis_aligned_rect_with_fractional_x_is_exact() {
    let scene = Scene {
        prims: vec![quad(10.3, 5.0, 20.6, 9.0)],
        hole: None,
    };
    let r = full(32, 16);
    let cov = render(&scene, r, 8);
    for y in 0..16 {
        for x in 0..32 {
            let ex = ((x as f64 + 1.0).min(20.6) - (x as f64).max(10.3)).max(0.0);
            let e = if (5..9).contains(&y) { ex } else { 0.0 };
            let got = f64::from(cov[y * 32 + x]);
            assert!((got - e).abs() < 1e-6, "({x},{y}) {got} vs {e}");
        }
    }
}

#[test]
fn identical_overlaps_are_a_union_not_a_product() {
    let p = quad(10.0, 0.0, 10.5, 4.0);
    let one = render(
        &Scene {
            prims: vec![p],
            hole: None,
        },
        full(16, 4),
        8,
    );
    let three = render(
        &Scene {
            prims: vec![p, p, p],
            hole: None,
        },
        full(16, 4),
        8,
    );
    assert_eq!(one, three);
    assert!((one[10] - 0.5).abs() < 1e-6, "{}", one[10]);
}

#[test]
fn overlapping_rects_integrate_to_union_area() {
    let scene = Scene {
        prims: vec![quad(2.0, 2.0, 10.0, 10.0), quad(6.0, 6.0, 14.0, 14.0)],
        hole: None,
    };
    let total: f64 = render(&scene, full(16, 16), 8)
        .iter()
        .map(|&v| f64::from(v))
        .sum();
    assert!((total - 112.0).abs() < 1e-4, "{total}");
}

#[test]
fn tapered_triangle_matches_point_sampling_oracle() {
    let tri = vec![[3.2, 2.7], [28.9, 9.1], [3.9, 13.4]];
    let scene = Scene {
        prims: vec![Prim::from_points(&tri).unwrap()],
        hole: None,
    };
    let r = full(32, 16);
    let expected = oracle(&[tri.clone()], None, r, 64);
    let area = shoelace(&tri);
    for (samples, tol) in [(8u32, 0.08), (16, 0.045)] {
        let cov = render(&scene, r, samples);
        let mut total = 0.0;
        for (i, (&c, &e)) in cov.iter().zip(&expected).enumerate() {
            assert!(
                (f64::from(c) - e).abs() <= tol,
                "S={samples} pixel {i}: {c} vs {e}"
            );
            total += f64::from(c);
        }
        assert!(
            (total - area).abs() / area < 0.01,
            "S={samples}: {total} vs {area}"
        );
    }
}

#[test]
fn hole_is_empty_and_area_matches() {
    let hole = Hole {
        c: [32.0, 32.0],
        m: [[1.0 / 12.0, 0.0], [0.0, 1.0 / 12.0]],
    };
    let scene = Scene {
        prims: vec![quad(0.0, 0.0, 64.0, 64.0)],
        hole: Some(hole),
    };
    let cov = render(&scene, full(64, 64), 8);
    for y in 0..64 {
        for x in 0..64 {
            let corners = [[x, y], [x + 1, y], [x, y + 1], [x + 1, y + 1]];
            if corners
                .iter()
                .all(|c| hole.contains([f64::from(c[0]), f64::from(c[1])]))
            {
                assert_eq!(cov[(y * 64 + x) as usize], 0.0, "({x},{y}) inside hole");
            }
        }
    }
    let total: f64 = cov.iter().map(|&v| f64::from(v)).sum();
    let expected = 4096.0 - std::f64::consts::PI * 144.0;
    assert!(
        (total - expected).abs() / expected < 0.005,
        "{total} vs {expected}"
    );
}

fn small_mapping() -> Mapping {
    Mapping::new(480.0, 270.0, 1.0, 1.0, 1.0)
}

fn small_settings() -> Settings {
    // Defaults are tuned for 1080p; scale widths for the 480x270 fixture.
    Settings {
        line_width: 4.0,
        ..Settings::default()
    }
}

#[test]
fn default_scene_matches_point_oracle_and_center_is_empty() {
    let m = small_mapping();
    let (scene, stats) = layout::build_scene(&small_settings(), &m, 0);
    assert!(stats.lines > 100 && stats.prims >= stats.lines, "{stats:?}");
    assert_eq!(stats.capped_lines, 0);
    let r = PixelRect {
        x0: 0,
        y0: 0,
        w: 480,
        h: 270,
    };
    let cov = render(&scene, r, 16);
    assert_eq!(
        cov[135 * 480 + 240],
        0.0,
        "focus is inside the empty centre"
    );
    let partial = cov.iter().filter(|&&v| v > 0.0 && v < 1.0).count();
    let solid = cov.iter().filter(|&&v| v >= 1.0).count();
    assert!(
        partial > 1000 && solid > 1000,
        "partial {partial} solid {solid}"
    );
    // Oracle on a sub-window near the frame edge.
    let sub = PixelRect {
        x0: 400,
        y0: 20,
        w: 40,
        h: 40,
    };
    let polys: Vec<Vec<[f64; 2]>> = scene.prims.iter().map(|p| p.verts().to_vec()).collect();
    let expected = oracle(&polys, scene.hole.as_ref(), sub, 32);
    let got = render(&scene, sub, 16);
    let mut err = 0.0;
    for (&g, &e) in got.iter().zip(&expected) {
        assert!((f64::from(g) - e).abs() <= 0.08, "{g} vs {e}");
        err += (f64::from(g) - e).abs();
    }
    assert!(
        err / expected.len() as f64 <= 0.01,
        "mean error {}",
        err / expected.len() as f64
    );
}

#[test]
fn roi_and_tiling_are_bitwise_identical() {
    let m = small_mapping();
    let (scene, _) = layout::build_scene(&small_settings(), &m, 0);
    let whole = PixelRect {
        x0: -8,
        y0: -5,
        w: 500,
        h: 290,
    };
    let reference = render(&scene, whole, 8);
    for sub in [
        PixelRect {
            x0: 100,
            y0: 50,
            w: 37,
            h: 61,
        },
        PixelRect {
            x0: -8,
            y0: -5,
            w: 17,
            h: 3,
        },
        PixelRect {
            x0: 470,
            y0: 260,
            w: 22,
            h: 25,
        },
    ] {
        let got = render(&scene, sub, 8);
        for y in 0..sub.h as i32 {
            for x in 0..sub.w as i32 {
                let a = got[(y * sub.w as i32 + x) as usize];
                let gx = sub.x0 + x - whole.x0;
                let gy = sub.y0 + y - whole.y0;
                let b = reference[(gy * whole.w as i32 + gx) as usize];
                assert_eq!(a.to_bits(), b.to_bits(), "({}, {})", sub.x0 + x, sub.y0 + y);
            }
        }
    }
}

#[test]
fn hole_stays_empty_with_tip_width_extreme_aspect_and_offset() {
    let m = small_mapping();
    let s = Settings {
        tip_width: 60.0,
        line_width: 30.0,
        center_aspect: 4.0,
        center_size: 30.0,
        center_offset: [90.0, 40.0],
        center_rotation: 25.0,
        length_random: 0.0,
        density_variation: 0.0,
        ..Settings::default()
    };
    let (scene, _) = layout::build_scene(&s, &m, 0);
    let hole = scene.hole.expect("hole");
    let cov = render(
        &scene,
        PixelRect {
            x0: 0,
            y0: 0,
            w: 480,
            h: 270,
        },
        8,
    );
    let mut inside = 0;
    for y in 0..270 {
        for x in 0..480 {
            let corners = [[x, y], [x + 1, y], [x, y + 1], [x + 1, y + 1]];
            if corners
                .iter()
                .all(|c| hole.contains([f64::from(c[0]), f64::from(c[1])]))
            {
                inside += 1;
                assert_eq!(cov[(y * 480 + x) as usize], 0.0, "({x},{y})");
            }
        }
    }
    assert!(inside > 1000, "{inside}");
    // Lines with a wide tip reach the hole boundary somewhere.
    assert!(cov.iter().any(|&v| v > 0.0));
}

#[test]
fn zero_degenerate_and_extreme_values_are_safe() {
    let m = small_mapping();
    let r = PixelRect {
        x0: 0,
        y0: 0,
        w: 480,
        h: 270,
    };
    let cases = [
        Settings {
            line_count: 0.0,
            ..small_settings()
        },
        Settings {
            line_width: 0.0,
            ..small_settings()
        },
        Settings {
            opacity: 0.0,
            ..small_settings()
        },
        Settings {
            center_size: 1000.0,
            ..small_settings()
        },
        Settings {
            focus: Some([1.0e7, -1.0e7]),
            ..small_settings()
        },
        Settings {
            focus: Some([-3000.0, 135.0]),
            ..small_settings()
        },
        Settings {
            focus: Some([0.0, 0.0]),
            center_size: 0.0,
            ..small_settings()
        },
        Settings {
            center_aspect: 0.1,
            ..small_settings()
        },
        Settings {
            center_aspect: 10.0,
            ..small_settings()
        },
        Settings {
            tip_width: 100.0,
            ..small_settings()
        },
        Settings {
            taper_length: 0.0,
            ..small_settings()
        },
        Settings {
            taper_shape: 0.25,
            line_width: 500.0,
            line_count: 20.0,
            ..small_settings()
        },
        Settings {
            line_count: f64::NAN,
            line_width: f64::INFINITY,
            center_size: f64::NAN,
            ..small_settings()
        },
        Settings {
            line_count: 5000.0,
            clustering: 100.0,
            cluster_spread: 0.0,
            ..small_settings()
        },
    ];
    for (i, s) in cases.iter().enumerate() {
        let (scene, _) = layout::build_scene(s, &m, 0);
        let cov = render(&scene, r, 8);
        assert!(
            cov.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v)),
            "case {i}"
        );
        if i <= 3 {
            assert!(
                cov.iter().all(|&v| v == 0.0),
                "case {i} must be transparent"
            );
        }
    }
    // Zero-size layer and zero-size output.
    let (scene, _) =
        layout::build_scene(&small_settings(), &Mapping::new(0.0, 0.0, 1.0, 1.0, 1.0), 0);
    assert!(scene.prims.is_empty());
    assert!(render(
        &scene,
        PixelRect {
            x0: 0,
            y0: 0,
            w: 0,
            h: 0
        },
        8
    )
    .is_empty());
}

#[test]
fn gap_width_edge_cases() {
    let m = small_mapping();
    let base = layout::layout_lines(&small_settings(), &m, 0);
    let zero = layout::layout_lines(
        &Settings {
            gap_width: 0.0,
            gap_softness: 50.0,
            ..small_settings()
        },
        &m,
        0,
    );
    assert_eq!(base, zero);
    let all = layout::layout_lines(
        &Settings {
            gap_width: 360.0,
            ..small_settings()
        },
        &m,
        0,
    );
    assert!(all.is_empty());
    let half = layout::layout_lines(
        &Settings {
            gap_width: 180.0,
            gap_softness: 0.0,
            ..small_settings()
        },
        &m,
        0,
    );
    assert!(
        half.len() < base.len() * 2 / 3,
        "{} vs {}",
        half.len(),
        base.len()
    );
}

#[test]
fn hold_frames_use_floor_and_static_ignores_time() {
    let m = small_mapping();
    let s = Settings {
        evolution: Evolution::HoldFrames,
        hold_frames: 2,
        ..small_settings()
    };
    let at = |f: i64| layout::layout_lines(&s, &m, f);
    assert_eq!(at(0), at(1));
    assert_ne!(at(1), at(2));
    assert_eq!(at(2), at(3));
    assert_eq!(at(-2), at(-1));
    assert_ne!(at(-1), at(0));
    let stat = small_settings();
    assert_eq!(
        layout::layout_lines(&stat, &m, 0),
        layout::layout_lines(&stat, &m, 100)
    );
    // Change Amount 0 keeps the lines identical across epochs.
    let frozen = Settings {
        change_amount: 0.0,
        ..s
    };
    assert_eq!(
        layout::layout_lines(&frozen, &m, 0),
        layout::layout_lines(&frozen, &m, 7)
    );
    // 23.976 fps rational time: step 1001 at scale 24000.
    for k in -5i64..50 {
        assert_eq!(layout::frame_index(k * 1001, 1001), k);
        assert_eq!(layout::frame_index(k * 1001 + 1000, 1001), k);
    }
}

#[test]
fn fractional_count_fades_only_the_last_line() {
    let m = small_mapping();
    let s = Settings {
        line_count: 10.5,
        width_random: 0.0,
        length_random: 0.0,
        density_variation: 0.0,
        ..small_settings()
    };
    let lines = layout::layout_lines(&s, &m, 0);
    let last = lines.iter().find(|l| l.id == 10).expect("line 10");
    let first = lines.iter().find(|l| l.id == 0).expect("line 0");
    assert!((last.width - first.width * 0.5).abs() < 1e-9);
}

#[test]
fn downsampled_render_is_consistent_with_full_resolution() {
    let s = Settings {
        line_count: 1.0,
        line_width: 20.0,
        focus: Some([100.0, 150.0]),
        center_size: 10.0,
        length_random: 0.0,
        density_variation: 0.0,
        ..Settings::default()
    };
    let full_m = Mapping::new(400.0, 300.0, 1.0, 1.0, 1.0);
    let half_m = Mapping::new(400.0, 300.0, 1.0, 0.5, 0.5);
    let (a, _) = layout::build_scene(&s, &full_m, 0);
    let (b, _) = layout::build_scene(&s, &half_m, 0);
    let ta: f64 = render(&a, full(400, 300), 16)
        .iter()
        .map(|&v| f64::from(v))
        .sum();
    let tb: f64 = render(&b, full(200, 150), 16)
        .iter()
        .map(|&v| f64::from(v))
        .sum();
    assert!(ta > 100.0);
    assert!((ta / 4.0 - tb).abs() / tb < 0.02, "{} vs {}", ta / 4.0, tb);
}

#[test]
fn pixel_aspect_keeps_the_centre_round_in_display_space() {
    // PAR 2: one layer pixel is two display units wide, so a display circle is
    // half as wide in layer/buffer pixels as it is tall.
    let s = Settings {
        center_aspect: 1.0,
        center_size: 40.0,
        ..Settings::default()
    };
    let m = Mapping::new(400.0, 300.0, 2.0, 1.0, 1.0);
    let (scene, _) = layout::build_scene(&s, &m, 0);
    let hole = scene.hole.unwrap();
    let (l, r) = hole.scan(hole.c[1]).unwrap();
    let half_width = (r - l) * 0.5;
    let half_height = 0.4 * 150.0;
    assert!(
        (half_width - half_height / 2.0).abs() < 1e-6,
        "{half_width}"
    );
    assert!((hole.c[0] - 200.0).abs() < 1e-9 && (hole.c[1] - 150.0).abs() < 1e-9);
}

#[test]
fn seed_changes_every_line() {
    let m = small_mapping();
    let a = layout::layout_lines(&small_settings(), &m, 0);
    let b = layout::layout_lines(
        &Settings {
            seed: 2,
            ..small_settings()
        },
        &m,
        0,
    );
    let same = a.iter().filter(|l| b.iter().any(|o| o == *l)).count();
    assert_eq!(same, 0);
}

#[test]
fn thick_diagonal_lines_reach_the_canvas_edge_without_endcap_gaps() {
    let m = Mapping::new(64.0, 48.0, 1.0, 1.0, 1.0);
    let s = Settings {
        line_count: 1.0,
        line_width: 20.0,
        width_random: 0.0,
        center_size: 0.0,
        length_random: 0.0,
        density_variation: 0.0,
        taper_length: 0.0,
        ..Settings::default()
    };
    let line = layout::layout_lines(&s, &m, 0)[0];
    let dir = [line.theta.sin(), -line.theta.cos()];
    let normal = [dir[1], -dir[0]];
    let (scene, _) = layout::build_scene(&s, &m, 0);
    let pixels = render(&scene, full(64, 48), 16);
    let mut checked = 0;
    for y in 0..48usize {
        for x in 0..64usize {
            if x != 0 && x != 63 && y != 0 && y != 47 {
                continue;
            }
            let corners = [
                [x as f64, y as f64],
                [x as f64 + 1.0, y as f64],
                [x as f64, y as f64 + 1.0],
                [x as f64 + 1.0, y as f64 + 1.0],
            ];
            if corners.iter().all(|p| {
                let d = [p[0] - 32.0, p[1] - 24.0];
                d[0] * dir[0] + d[1] * dir[1] > 0.0
                    && (d[0] * normal[0] + d[1] * normal[1]).abs() < 9.9
            }) {
                checked += 1;
                assert!(
                    pixels[y * 64 + x] > 0.999,
                    "missing line at edge {x},{y}: {}",
                    pixels[y * 64 + x]
                );
            }
        }
    }
    assert!(checked > 3);
}

#[test]
fn short_line_range_mix_and_time_are_independent_of_normal_lengths() {
    let m = Mapping::new(1920.0, 1080.0, 1.0, 1.0, 1.0);
    let base = Settings {
        center_size: 0.0,
        length_random: 0.0,
        density_variation: 0.0,
        line_count: 1000.0,
        clustering: 0.0,
        short_min: 30.0,
        short_max: 120.0,
        evolution: Evolution::HoldFrames,
        ..Settings::default()
    };
    let normal = layout::layout_lines(&base, &m, 0);
    let short = Settings {
        short_mix: 100.0,
        ..base
    };
    for frame in [0, 2, 17] {
        let lines = layout::layout_lines(&short, &m, frame);
        assert_eq!(lines.len(), 1000);
        assert!(lines
            .iter()
            .all(|l| (29.999..120.001).contains(&(l.r1 - l.r0))));
    }
    let reverse = Settings {
        short_min: 120.0,
        short_max: 30.0,
        ..short
    };
    assert_eq!(
        layout::layout_lines(&short, &m, 0),
        layout::layout_lines(&reverse, &m, 0)
    );
    let mixed = layout::layout_lines(
        &Settings {
            short_mix: 40.0,
            ..base
        },
        &m,
        0,
    );
    let selected = mixed.iter().filter(|l| l.r1 - l.r0 < 121.0).count();
    assert!((340..460).contains(&selected));
    for (a, b) in mixed.iter().zip(&normal) {
        if a.r1 - a.r0 > 121.0 {
            assert_eq!(a, b);
        }
    }
    let zero = Settings {
        short_min: 900.0,
        short_max: 1000.0,
        ..base
    };
    assert_eq!(normal, layout::layout_lines(&zero, &m, 0));
    assert_ne!(
        layout::layout_lines(&short, &m, 0),
        layout::layout_lines(&short, &m, 2)
    );
}
