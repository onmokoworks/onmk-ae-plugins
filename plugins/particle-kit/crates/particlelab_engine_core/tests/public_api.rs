use particlelab_engine_core::prelude::*;

#[test]
fn downstream_particle_effect_can_render_without_ae_host_types() {
    let mut engine = ParticleEngineConfig::classic_default();
    engine.render.apply_mode = ApplyMode::OnTransparent;
    engine.render.composite_on_original = false;
    engine.emitter.emit_all_at_start = true;
    engine.emitter.birth_rate = 24.0;

    let runtime = ParticleRuntimeInputs::new(64, 64, 0, 0, 0.25, 1.0 / 30.0).unwrap();
    let plan = ParticleRenderPlan::new(&engine, runtime);
    let mut output = vec![0_u8; 64 * 64 * RenderSurface::ARGB8_BYTES_PER_PIXEL];

    let particle_count =
        render_particle_engine_8bit::<()>(engine, plan, &mut output, None, || Ok(())).unwrap();

    assert!(particle_count > 0);
    assert!(output.chunks_exact(4).any(|pixel| pixel[0] > 0));
}

#[test]
fn downstream_effect_can_validate_argb_surface_before_rendering() {
    let surface = RenderSurface::argb8(16, 8).unwrap();
    let output = vec![0_u8; surface.len_bytes];

    assert_eq!(surface.width, 16);
    assert_eq!(surface.height, 8);
    assert_eq!(surface.row_bytes, 16 * RenderSurface::ARGB8_BYTES_PER_PIXEL);
    assert!(surface.fits_buffer(&output));
}

#[test]
fn downstream_particle_effect_can_compile_its_own_adapter_to_core() {
    struct SparkFieldConfig {
        birth_rate: f32,
        color: [f32; 4],
        surface_row_bytes: usize,
    }

    impl SparkFieldConfig {
        fn compile_to_engine(&self) -> ParticleEngineConfig {
            let mut engine = ParticleEngineConfig::classic_default();
            engine.emitter.position = glam::Vec3::new(16.0, 12.0, 0.0);
            engine.emitter.birth_rate = self.birth_rate;
            engine.emitter.emit_all_at_start = true;
            engine.emitter.initial_speed = 0.0;
            engine.emitter.initial_size = 6.0;
            engine.physics.gravity = glam::Vec3::ZERO;
            engine.physics.turbulence_strength = 0.0;
            engine.appearance.color_start = self.color;
            engine.appearance.color_end = self.color;
            engine.render.blend_mode = BlendMode::Add;
            engine.render.apply_mode = ApplyMode::OnTransparent;
            engine.render.composite_on_original = false;
            engine
        }

        fn compile_runtime(&self) -> ParticleRuntimeInputs {
            ParticleRuntimeInputs::argb8_with_row_bytes(
                32,
                24,
                self.surface_row_bytes,
                0,
                0,
                0.2,
                1.0 / 30.0,
            )
            .unwrap()
        }
    }

    let adapter = SparkFieldConfig {
        birth_rate: 16.0,
        color: [1.0, 0.8, 0.1, 1.0],
        surface_row_bytes: 160,
    };
    let engine = adapter.compile_to_engine();
    let runtime = adapter.compile_runtime();
    let plan = ParticleRenderPlan::new(&engine, runtime);
    let mut output = vec![0_u8; 24 * adapter.surface_row_bytes];

    let particle_count =
        render_particle_engine_8bit::<()>(engine, plan, &mut output, None, || Ok(())).unwrap();

    assert!(particle_count > 0);
    assert!(output.chunks_exact(4).any(|pixel| pixel[0] > 0));
}
