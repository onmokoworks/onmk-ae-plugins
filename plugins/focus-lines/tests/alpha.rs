//! Alpha contract (docs/alpha-contract.md of the monorepo): straight RGB at the
//! AE boundary, 16-bit white = 32768, unclamped float HDR. Includes negative
//! controls that a premultiplied generator must fail.

use focus_lines::pixel::{self, Argb, Shading};

const YELLOW: [f32; 3] = [1.0, 0.866_666_7, 0.2];
const COVERAGES: [f32; 6] = [0.0, 1.0 / 255.0, 0.125, 0.5, 0.75, 1.0];

fn lines(color: [f32; 3], intensity: f32, opacity: f32) -> Vec<Argb> {
    let shading = Shading::new(color, intensity, opacity);
    COVERAGES
        .iter()
        .map(|&c| pixel::lines_only(c, &shading))
        .collect()
}

fn premultiplied_mutant(pixels: &[Argb]) -> Vec<Argb> {
    pixels
        .iter()
        .map(|p| [p[0], p[1] * p[0], p[2] * p[0], p[3] * p[0]])
        .collect()
}

#[test]
fn lines_only_is_straight_constant_with_coverage_in_alpha_only() {
    for opacity in [100.0, 50.0, 12.5] {
        let px = lines(YELLOW, 1.0, opacity);
        pixel::check_straight_constant(&px, YELLOW, 0.0).unwrap();
        for (p, &c) in px.iter().zip(&COVERAGES) {
            assert!((p[0] - c * opacity / 100.0).abs() < 1e-7, "{p:?}");
        }
        // Transparent pixels still carry the line colour.
        assert_eq!(&px[0][1..], &YELLOW);
    }
}

#[test]
fn intentionally_premultiplied_output_is_rejected() {
    let px = lines(YELLOW, 1.0, 100.0);
    let mutant = premultiplied_mutant(&px);
    let err = pixel::check_straight_constant(&mutant, YELLOW, 1.0 / 255.0).unwrap_err();
    assert!(err.contains("expected"), "{err}");
    // Quantized mutant also fails at 8 and 16 bit.
    let q8: Vec<Argb> = mutant
        .iter()
        .map(|&p| pixel::decode8(pixel::encode8(p)))
        .collect();
    assert!(pixel::check_straight_constant(&q8, YELLOW, 1.0 / 255.0).is_err());
    let q16: Vec<Argb> = mutant
        .iter()
        .map(|&p| pixel::decode16(pixel::encode16(p)))
        .collect();
    assert!(pixel::check_straight_constant(&q16, YELLOW, 1.0 / 32768.0).is_err());
}

#[test]
fn acceptance_check_is_not_vacuous() {
    let opaque = vec![[1.0, 1.0, 0.0, 0.0]; 4];
    assert!(pixel::check_straight_constant(&opaque, [1.0, 0.0, 0.0], 0.0).is_err());
    assert!(pixel::check_straight_constant(&[], [1.0, 0.0, 0.0], 0.0).is_err());
}

#[test]
fn white_lines_on_white_do_not_darken_but_premultiplied_does() {
    let px = lines([1.0, 1.0, 1.0], 1.0, 100.0);
    for p in &px {
        let c = pixel::composite_on(*p, [1.0; 3]);
        assert!(c.iter().all(|&v| (v - 1.0).abs() < 1e-6), "{c:?}");
    }
    let darker = premultiplied_mutant(&px)
        .iter()
        .map(|p| pixel::composite_on(*p, [1.0; 3])[0])
        .fold(1.0f32, f32::min);
    assert!(darker < 0.8, "mutant must darken on white: {darker}");
}

#[test]
fn encodings_use_255_32768_and_unclamped_float() {
    let white = pixel::lines_only(1.0, &Shading::new([1.0; 3], 1.0, 100.0));
    assert_eq!(pixel::encode8(white), [255; 4]);
    assert_eq!(pixel::encode16(white), [32768; 4]);
    let half = pixel::lines_only(0.5, &Shading::new([1.0; 3], 1.0, 100.0));
    assert_eq!(pixel::encode16(half), [16384, 32768, 32768, 32768]);
    let hdr = pixel::lines_only(0.25, &Shading::new([1.0, 0.5, 0.0], 4.0, 100.0));
    assert_eq!(pixel::encode32(hdr), [0.25, 4.0, 2.0, 0.0]);
    assert_eq!(pixel::encode8(hdr), [64, 255, 255, 0]);
    assert_eq!(pixel::encode16(hdr)[1], 32768);
    assert_eq!(
        pixel::encode32([f32::NAN, f32::INFINITY, -1.5, 0.5]),
        [0.0, 0.0, -1.5, 0.5]
    );
    assert_eq!(
        pixel::decode16([32768, 40000, 0, 16384]),
        [1.0, 1.0, 0.0, 0.5]
    );
}

/// Independent straight-over reference written from the definition.
fn expected_over(line_rgb: [f32; 3], la: f32, src: Argb) -> Argb {
    let sa = src[0];
    let oa = la + sa * (1.0 - la);
    let mut out = [oa, 0.0, 0.0, 0.0];
    for c in 0..3 {
        out[c + 1] = (line_rgb[c] * la + src[c + 1] * sa * (1.0 - la)) / oa;
    }
    out
}

#[test]
fn over_source_matches_independent_formula_and_passes_through_at_zero() {
    let sources: [Argb; 7] = [
        [0.0, 0.3, 0.6, 0.9],
        [1.0 / 255.0, 1.0, 0.0, 0.0],
        [0.125, 0.0, 1.0, 0.0],
        [0.5, 1.0, 1.0, 1.0],
        [1.0, 0.2, 0.4, 0.6],
        [0.5, 2.5, -0.1, 0.0],
        [1.0, 2.5, -0.1, 7.0],
    ];
    let shading = Shading::new(YELLOW, 1.0, 50.0);
    for src in sources {
        assert_eq!(
            pixel::over_source(0.0, &shading, src),
            None,
            "zero line alpha passes through"
        );
        for cov in [1.0 / 255.0, 0.5, 1.0] {
            let got = pixel::over_source(cov, &shading, src).unwrap();
            let want = expected_over(YELLOW, cov * 0.5, src);
            for c in 0..4 {
                assert!(
                    (got[c] - want[c]).abs() < 1e-6,
                    "src {src:?} cov {cov}: {got:?} vs {want:?}"
                );
            }
        }
    }
    // Fully transparent source: output colour is exactly the line colour.
    let got = pixel::over_source(0.5, &shading, [0.0, 0.3, 0.6, 0.9]).unwrap();
    assert!((got[1] - YELLOW[0]).abs() < 1e-6 && (got[2] - YELLOW[1]).abs() < 1e-6);
    // HDR source survives under partial line coverage in float.
    let got = pixel::over_source(0.5, &shading, [1.0, 2.5, -0.1, 7.0]).unwrap();
    assert!(got[1] > 1.0 && got[2] < 0.5 && got[3] > 1.0, "{got:?}");
}
