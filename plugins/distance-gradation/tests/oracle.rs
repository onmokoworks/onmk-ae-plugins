// SPDX-License-Identifier: Apache-2.0
// onmk implementation (2026); upstream attribution and changes: see package NOTICE.
use distance_gradation::core::*;
fn image(w: usize, h: usize, alpha: impl Fn(usize, usize) -> f32) -> Image {
    Image::new(
        w,
        h,
        0,
        0,
        (0..h)
            .flat_map(|y| (0..w).map(move |x| (x, y)))
            .map(|(x, y)| [0.2, 0.6, 1.0, alpha(x, y)])
            .collect(),
    )
    .unwrap()
}
fn close(a: f32, b: f32) {
    assert!((a - b).abs() < 2e-5, "{a} != {b}");
}
#[test]
fn distance_matches_exhaustive_search_with_anisotropic_pixels() {
    for aa in [false, true] {
        let img = image(9, 7, |x, y| {
            [0.0, 1.0 / 255.0, 0.125, 0.5, 0.75, 1.0][(x * 11 + y * 7) % 6]
        });
        let s = Settings {
            antialias: aa,
            scale: [0.5, 0.25],
            ..Default::default()
        };
        let actual = signed_distance(&img, &s).unwrap();
        for y in 0..7 {
            for x in 0..9 {
                let mut di = f64::INFINITY;
                let mut dout = f64::INFINITY;
                for yy in 0..7 {
                    for xx in 0..9 {
                        let a = img.pixels[yy * 9 + xx][3] as f64;
                        let spatial = ((x as f64 - xx as f64) * 2.0).powi(2)
                            + ((y as f64 - yy as f64) * 4.0).powi(2);
                        if aa && a > 0.0 && a < 1.0 {
                            dout = dout.min(spatial + ((0.5 - a).max(0.0) * 2.0).powi(2));
                            di = di.min(spatial + ((a - 0.5).max(0.0) * 2.0).powi(2));
                        } else if a >= 0.5 {
                            dout = dout.min(spatial);
                        } else {
                            di = di.min(spatial);
                        }
                    }
                }
                close(actual[y * 9 + x], (dout.sqrt() - di.sqrt()) as f32);
            }
        }
    }
}
#[test]
fn original_and_underlying_background_survive() {
    let img = image(21, 21, |x, y| {
        if (7..14).contains(&x) && (7..14).contains(&y) {
            1.0
        } else {
            0.0
        }
    });
    for side in 1..=3 {
        let s = Settings {
            side,
            inside: 2.0,
            outside: 3.0,
            ..Default::default()
        };
        let result = render(&img, &s).unwrap();
        assert_eq!(
            result[10 * 21 + 10],
            img.pixels[10 * 21 + 10],
            "unaffected original center"
        );
        assert_eq!(result[0][3], 0.0, "lower layer must remain visible");
        for (p, src) in result.iter().zip(&img.pixels) {
            assert!(p[3] >= src[3]);
        }
    }
}
#[test]
fn over_has_analytic_straight_result_and_preserves_hdr() {
    let p = over([1.0, 0.0, 0.0, 0.25], [0.0, 0.0, 1.0, 0.5]);
    for (a, b) in p.into_iter().zip([0.4, 0.0, 0.6, 0.625]) {
        close(a, b);
    }
    assert_eq!(
        over([0.0; 4], [4.0, -1.0, 2.0, 0.125]),
        [4.0, -1.0, 2.0, 0.125]
    );
    let p = over([1.0; 4], [2.0, -1.0, 0.0, 0.5]);
    assert_eq!(p, [1.0; 4]);
}
fn accepts_white(p: &[[f32; 4]]) -> bool {
    p.iter()
        .filter(|p| p[3] > 0.0 && p[3] < 1.0)
        .all(|p| p[..3].iter().all(|c| (*c - 1.0).abs() < 1e-6))
}
#[test]
fn partial_alpha_has_straight_color_and_rejects_premult_mutation() {
    let img = image(9, 3, |x, _| {
        [
            0.0,
            1.0 / 255.0,
            0.125,
            0.5,
            1.0,
            0.5,
            0.125,
            1.0 / 255.0,
            0.0,
        ][x]
    });
    let s = Settings {
        color: [1.0; 3],
        composite: false,
        side: 3,
        inside: 10.0,
        outside: 10.0,
        ..Default::default()
    };
    let result = render(&img, &s).unwrap();
    assert!(result.iter().any(|p| p[3] > 0.0 && p[3] < 1.0));
    assert!(accepts_white(&result));
    let bad: Vec<_> = result
        .iter()
        .map(|p| [p[0] * p[3], p[1] * p[3], p[2] * p[3], p[3]])
        .collect();
    assert!(!accepts_white(&bad));
    for p in result {
        for c in 0..3 {
            close(p[c] * p[3] + 1.0 - p[3], 1.0);
        }
    }
}
#[test]
fn antialias_retains_fractional_contours() {
    let img = image(9, 3, |x, _| {
        if x < 3 {
            0.0
        } else if x == 3 {
            0.25
        } else {
            1.0
        }
    });
    let s = Settings {
        composite: false,
        inside: 4.0,
        ..Default::default()
    };
    let smooth = render(&img, &s).unwrap();
    let hard = render(
        &img,
        &Settings {
            antialias: false,
            ..s.clone()
        },
    )
    .unwrap();
    assert!(smooth[3][3] > 0.0 && smooth[3][3] < 0.25);
    assert_eq!(hard[3][3], 0.0);
    let da = signed_distance(&img, &s).unwrap();
    close(da[3], 0.25);
}
#[test]
fn opacity_zero_is_exact_identity() {
    let mut img = image(11, 7, |x, y| ((x + y) % 5) as f32 / 4.0);
    img.pixels[4] = [3.4, -0.7, 1.01, 0.125];
    assert_eq!(
        render(
            &img,
            &Settings {
                opacity: 0.0,
                ..Default::default()
            }
        )
        .unwrap(),
        img.pixels
    );
}
#[test]
fn empty_solid_and_tiny_images_are_finite() {
    for (w, h) in [(0, 0), (1, 1), (1, 7), (9, 1), (5, 7)] {
        for a in [0.0, 0.125, 0.5, 1.0] {
            let img = image(w, h, |_, _| a);
            for side in 1..=3 {
                for invert in [false, true] {
                    let p = render(
                        &img,
                        &Settings {
                            side,
                            invert,
                            ..Default::default()
                        },
                    )
                    .unwrap();
                    assert!(p.iter().flatten().all(|v| v.is_finite()));
                    if a == 0.0 {
                        assert!(p.iter().all(|p| p[3] == 0.0));
                    }
                }
            }
        }
    }
}
#[test]
fn blur_and_interpolation_do_not_create_opaque_canvas() {
    let img = image(31, 27, |x, y| {
        if (12..19).contains(&x) && (10..17).contains(&y) {
            0.5
        } else {
            0.0
        }
    });
    for blur in 1..=3 {
        for interpolation in 1..=4 {
            let p = render(
                &img,
                &Settings {
                    blur,
                    interpolation,
                    blur_size: 2.0,
                    inside: 3.0,
                    outside: 3.0,
                    side: 3,
                    ..Default::default()
                },
            )
            .unwrap();
            assert_eq!(p[0][3], 0.0);
            assert!(p.iter().flatten().all(|v| v.is_finite()));
            assert!(p.iter().zip(&img.pixels).all(|(a, b)| a[3] >= b[3]));
        }
    }
}
