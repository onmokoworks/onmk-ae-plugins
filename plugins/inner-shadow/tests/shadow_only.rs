use inner_shadow::{
    engine::{render, render_with, Image, Route},
    gpu,
    spec::{Blend, GpuMode, Plan, Scale, ShadowParams},
    world::{encode, Depth},
};

fn check_shadow(mode: GpuMode) {
    let alphas = [0.0, 1.0 / 255.0, 0.125, 0.5, 1.0];
    let input = Image::new(
        5,
        1,
        0,
        0,
        alphas.iter().map(|&a| [0.2, 0.7, 0.4, a]).collect(),
    )
    .unwrap();
    for blend in [Blend::Normal, Blend::Multiply] {
        let mut plan = Plan::new(
            &ShadowParams {
                color: [1.0, 0.75, 0.25],
                opacity: 0.8,
                distance: 0.0,
                size: 0.0,
                blend,
                ..Default::default()
            },
            &Scale::FULL,
        )
        .unwrap();
        assert!(plan.keep_source);
        plan.keep_source = false;
        let (out, route) = render(&input, &input.rect(), &plan, mode).unwrap();
        if mode == GpuMode::Gpu {
            assert!(matches!(route, Route::Gpu(_)));
        }
        for (p, a) in out.iter().zip(alphas) {
            let alpha = a * (1.0 - a) * 0.8;
            assert_eq!(
                &p[..3],
                &[1.0, 0.75, 0.25],
                "straight colour must not be alpha-weighted"
            );
            assert!((p[3] - alpha).abs() < 1e-6);
            for (depth, scale, bytes) in [(Depth::U8, 255.0, 1), (Depth::U16, 32768.0, 2)] {
                let mut encoded = vec![0u8; 4 * bytes];
                encode(*p, depth, &mut encoded);
                let read = |i| {
                    if bytes == 1 {
                        encoded[i] as u16
                    } else {
                        u16::from_ne_bytes([encoded[2 * i], encoded[2 * i + 1]])
                    }
                };
                assert_eq!(read(0), (alpha * scale).round() as u16);
                assert_eq!(read(1), scale as u16);
            }
        }
        // A premultiplied generator would fail the independent colour expectation.
        assert!((out[3][0] * out[3][3] - 1.0).abs() > 0.5);
        // Shadow-only is independent of source RGB and blend mode.
        let mut other = input.clone();
        for p in &mut other.pixels {
            p[..3].copy_from_slice(&[4.0, -2.0, 8.0]);
        }
        assert_eq!(out, render(&other, &other.rect(), &plan, mode).unwrap().0);
    }
    // Closed-form shifted opaque rectangle: top two inner rows contain shadow.
    let input = Image::new(
        12,
        12,
        0,
        0,
        (0..144)
            .map(|i| {
                [
                    0.8,
                    0.6,
                    0.2,
                    if (3..9).contains(&(i % 12)) && (3..9).contains(&(i / 12)) {
                        1.0
                    } else {
                        0.0
                    },
                ]
            })
            .collect(),
    )
    .unwrap();
    let mut plan = Plan::new(
        &ShadowParams {
            distance: 2.0,
            size: 0.0,
            angle_deg: 90.0,
            opacity: 0.7,
            ..Default::default()
        },
        &Scale::FULL,
    )
    .unwrap();
    plan.keep_source = false;
    let out = render(&input, &input.rect(), &plan, mode).unwrap().0;
    for (i, p) in out.iter().enumerate() {
        let expected = if (3..9).contains(&(i % 12)) && (3..5).contains(&(i / 12)) {
            0.7
        } else {
            0.0
        };
        assert!((p[3] - expected).abs() < 1e-6);
    }
}

#[test]
fn shadow_only_cpu_matches_independent_expectations() {
    check_shadow(GpuMode::Cpu);
}
#[test]
fn shadow_only_hardware_gpu_matches_independent_expectations() {
    if let Err(e) = gpu::adapter_identity() {
        assert_ne!(
            std::env::var("INNER_SHADOW_REQUIRE_GPU").as_deref(),
            Ok("1"),
            "{e}"
        );
        return;
    }
    check_shadow(GpuMode::Gpu);
}
#[test]
fn zero_opacity_shadow_only_is_transparent_without_gpu() {
    let input = Image::new(1, 1, 0, 0, vec![[1.0, 0.4, 0.2, 1.0]]).unwrap();
    let mut plan = Plan::new(
        &ShadowParams {
            opacity: 0.0,
            ..Default::default()
        },
        &Scale::FULL,
    )
    .unwrap();
    plan.keep_source = false;
    let (out, route) = render_with(&input, &input.rect(), &plan, GpuMode::Gpu, |_, _| {
        panic!("GPU must not initialise")
    })
    .unwrap();
    assert_eq!(out[0][3], 0.0);
    assert_eq!(route, Route::Transparent);
}
