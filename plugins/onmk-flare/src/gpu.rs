use parking_lot::RwLock;
use std::collections::HashMap;
#[cfg(test)]
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;
use wgpu::*;

use crate::{physical, EffectParams};

#[cfg(target_endian = "big")]
compile_error!("Direct ARGB8 GPU storage requires a little-endian AE host");

// Layout: scalar fields, six vec4 colors, then three arrays of twelve vec4s
// carrying the CPU-traced Cooke Triplet ghost profiles.
#[repr(C, align(16))]
#[derive(Clone, Copy)]
struct GpuParams {
    // Scalars (48 fields = 192 bytes, including explicit WGSL alignment pad)
    width: u32,
    height: u32,
    light_x: f32,
    light_y: f32,
    center_x: f32,
    center_y: f32,
    global_brightness: f32,
    global_scale: f32,
    flare_angle: f32,
    hotspot_intensity: f32,
    hotspot_size: f32,
    glow_intensity: f32,
    glow_radius: f32,
    glow_falloff: f32,
    streak_intensity: f32,
    streak_length: f32,
    streak_width: f32,
    streak_count: u32,
    streak_rotation: f32,
    stripe_intensity: f32,
    stripe_length: f32,
    stripe_width: f32,
    ring_intensity: f32,
    ring_radius: f32,
    ring_width: f32,
    ring_chromatic: f32,
    ring_spectrum: u32,
    starburst_intensity: f32,
    starburst_radius: f32,
    starburst_blades: u32,
    ghost_intensity: f32,
    ghost_count: u32,
    ghost_spread: f32,
    ghost_size: f32,
    ghost_chromatic: f32,
    edge_bright_mult: f32,
    edge_scale_mult: f32,
    atmosphere_amount: f32,
    atmosphere_scale: f32,
    chromatic_amount: f32,
    flicker_mod: f32,
    source_opacity: f32,
    flare_opacity: f32,
    transfer_mode: u32,
    source_mode: u32,
    _padding: [u32; 3],
    // Colors (6 x vec4 = 96 bytes, each 16-byte aligned)
    glow_color: [f32; 4],
    streak_color: [f32; 4],
    stripe_color: [f32; 4],
    ring_color: [f32; 4],
    starburst_color: [f32; 4],
    ghost_color: [f32; 4],
    cooke_ghost_a: [[f32; 4]; 12],
    cooke_ghost_b: [[f32; 4]; 12],
    cooke_ghost_c: [[f32; 4]; 12],
}

struct BufferState {
    width: u32,
    height: u32,
    source_buf: Buffer,
    output_buf: Buffer,
    staging_buf: Buffer,
    params_buf: Buffer,
    physical_buf: Buffer,
    bind_group: BindGroup,
    uploaded_optics: parking_lot::Mutex<std::sync::Weak<Vec<[f32; 4]>>>,
    resident_cache: parking_lot::Mutex<Option<(EffectParams, Vec<physical::BrightSource>)>>,
}

pub struct GpuProcessor {
    device: Device,
    queue: Queue,
    flare_pipeline: ComputePipeline,
    bind_group_layout: BindGroupLayout,
    state: RwLock<HashMap<usize, BufferState>>,
    healthy: Arc<std::sync::atomic::AtomicBool>,
    optical_cache: parking_lot::Mutex<Option<OpticalCache>>,
    source_gpu: Option<crate::source_gpu::SourceGpu>,
    bloom_gpu: Option<crate::bloom_gpu::BloomGpu>,
    optical_gpu_guard: parking_lot::Mutex<()>,
    resident_gpu: Option<crate::resident_gpu::ResidentGpu>,
    #[cfg(test)]
    optical_uploads: AtomicUsize,
}

struct OpticalCache {
    params: EffectParams,
    sources: Vec<physical::BrightSource>,
    dimensions: (usize, usize),
    pixels: Arc<Vec<[f32; 4]>>,
}

fn optical_key(ep: &EffectParams) -> EffectParams {
    let mut key = ep.clone();
    // These controls are consumed by the final GPU composite only. Keep all
    // other fields conservatively in the key so new optical controls cannot
    // silently reuse a stale image.
    key.global_brightness = 0.0;
    key.source_opacity = 0.0;
    key.flare_opacity = 0.0;
    key.transfer_mode = 0;
    key.flicker_amount = 0.0;
    key.flicker_phase = 0.0;
    key.atmosphere_amount = 0.0;
    key.atmosphere_scale = 0.0;
    key.edge_width = 0.0;
    key.edge_brightness = 0.0;
    key.edge_scale = 0.0;
    key
}

fn combine_optical_layers(
    ghosts: &[[f32; 3]],
    bloom: &[[f32; 3]],
    ocular: &[[f32; 3]],
    source: &[[f32; 3]],
    gain: f32,
    modern: f32,
    workers: usize,
) -> Vec<[f32; 4]> {
    let mut out = vec![[0.0; 4]; ghosts.len()];
    let workers = if out.len() >= 262144 {
        workers.clamp(1, 4)
    } else {
        1
    };
    let chunk_len = out.len().div_ceil(workers).max(1);
    let fill = |start: usize, chunk: &mut [[f32; 4]]| {
        for (offset, dst) in chunk.iter_mut().enumerate() {
            let i = start + offset;
            let (g, b, e, s) = (ghosts[i], bloom[i], ocular[i], source[i]);
            let safe = |v: f32| if v.is_finite() { v } else { 0.0 };
            *dst = [
                safe(g[0] + b[0]) + safe(e[0] + s[0] * modern) / gain.max(0.001),
                safe(g[1] + b[1]) + safe(e[1] + s[1] * modern) / gain.max(0.001),
                safe(g[2] + b[2]) + safe(e[2] + s[2] * modern) / gain.max(0.001),
                0.0,
            ];
        }
    };
    if workers == 1 {
        fill(0, &mut out);
    } else {
        std::thread::scope(|scope| {
            for (index, chunk) in out.chunks_mut(chunk_len).enumerate() {
                let fill = &fill;
                scope.spawn(move || fill(index * chunk_len, chunk));
            }
        });
    }
    out
}

#[cfg(test)]
mod cache_tests {
    use super::*;

    #[cfg(windows)]
    fn private_bytes() -> usize {
        #[repr(C)]
        struct Counters {
            size: u32,
            faults: u32,
            values: [usize; 9],
        }
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn K32GetProcessMemoryInfo(
                process: *mut std::ffi::c_void,
                counters: *mut Counters,
                size: u32,
            ) -> i32;
        }
        let mut c = Counters {
            size: std::mem::size_of::<Counters>() as u32,
            faults: 0,
            values: [0; 9],
        };
        assert_ne!(
            unsafe {
                K32GetProcessMemoryInfo(
                    -1isize as *mut _,
                    &mut c,
                    std::mem::size_of::<Counters>() as u32,
                )
            },
            0
        );
        c.values[8]
    }

    #[test]
    #[ignore = "1200 full-HD frames, resize/toggle churn, worker contention and actual device destruction"]
    fn memory_stability_soak() {
        use std::sync::atomic::Ordering::Relaxed;
        let mut gpu = GpuProcessor::new();
        let mut resident = crate::resident_gpu::ResidentGpu::new(&gpu.device).unwrap();
        resident.ghosts = Some(crate::ghost_gpu::GhostGpu::new(&gpu.device).unwrap());
        gpu.resident_gpu = Some(resident);
        gpu.source_gpu = None;
        gpu.bloom_gpu = None;
        let mut ep = EffectParams {
            light_x: 700.0,
            light_y: 400.0,
            source_mode: 1,
            global_scale: 1.0,
            global_brightness: 1.0,
            flare_opacity: 1.0,
            source_opacity: 1.0,
            transfer_mode: 4,
            physical_enabled: true,
            ray_grid: 12,
            physical_gain: 3000.0,
            max_area_boost: 8.0,
            ghost_intensity: 2.9,
            ghost_normalize: true,
            physical_ghost_blur: 0.005,
            physical_blur_passes: 3,
            bloom_strength: 0.6,
            bloom_radius: 0.018,
            bloom_passes: 3,
            bloom_octaves: 4,
            bloom_chromatic: true,
            hotspot_intensity: 1.6,
            ..Default::default()
        };
        let mut black = vec![0; 1920 * 1080 * 4];
        for pixel in black.chunks_exact_mut(4) {
            pixel[0] = 255;
        }
        for i in 0..40 {
            ep.light_x += 0.1;
            ep.lens_preset = i % 2;
            gpu.process(&ep, &black, 1920, 1080);
        }
        let stage = gpu.resident_gpu.as_ref().unwrap();
        let allocations = stage.scratch.lock().allocations;
        let frames = stage.frames.load(Relaxed);
        #[cfg(windows)]
        let memory_start = private_bytes();
        let mut times = Vec::new();
        for i in 0..1200 {
            ep.light_x = 600.0 + (i % 300) as f64;
            ep.lens_preset = (i / 100) % 2;
            let start = std::time::Instant::now();
            let out = gpu.process(&ep, &black, 1920, 1080);
            times.push(start.elapsed());
            assert_eq!(out.len(), black.len());
            assert!(gpu.is_healthy());
            assert_eq!(
                stage.frames.load(Relaxed),
                frames + i as usize + 1,
                "no silent fallback"
            );
            assert_eq!(gpu.state.read().len(), 1);
            assert_eq!(
                stage.scratch.lock().allocations,
                allocations,
                "large buffers must be reused"
            );
            assert!(stage.scratch.lock().bytes() <= 256 * 1024 * 1024);
            if i % 200 == 199 {
                #[cfg(windows)]
                eprintln!(
                    "soak frame={} private MiB={} scratch MiB={}",
                    i + 1,
                    private_bytes() / 1024 / 1024,
                    stage.scratch.lock().bytes() / 1024 / 1024
                );
            }
        }
        #[cfg(windows)]
        {
            let memory_end = private_bytes();
            eprintln!("process private bytes: start={memory_start} end={memory_end}");
            assert!(
                memory_end <= memory_start + 128 * 1024 * 1024,
                "unexpected sustained private-memory growth"
            );
        }
        times.sort();
        eprintln!(
            "1200 frames p50={:?} p95={:?} p99={:?} max={:?}",
            times[600], times[1140], times[1188], times[1199]
        );
        for i in 0..120 {
            let (w, h) = [(97, 61), (640, 360), (1280, 720), (1920, 1080)][i % 4];
            ep.physical_enabled = i % 3 != 0;
            ep.ocular_enabled = i % 7 == 0;
            ep.light_x = w as f64 * 0.4 + i as f64 * 0.01;
            ep.light_y = h as f64 * 0.4;
            ep.lens_preset = i % 4;
            gpu.process(&ep, &black[..w * h * 4], w, h);
            assert!(gpu.is_healthy());
            assert_eq!(gpu.state.read().len(), 1);
            if !ep.physical_enabled {
                assert_eq!(stage.scratch.lock().bytes(), (w * h * 16) as u64);
            }
        }
        ep.ocular_enabled = false;
        ep.physical_enabled = true;
        // New host worker IDs must not create per-worker frame caches.
        std::thread::scope(|scope| {
            for worker in 0..8 {
                let gpu = &gpu;
                let black = &black;
                let mut params = ep.clone();
                scope.spawn(move || {
                    for i in 0..8 {
                        params.light_x = 30.0 + (worker * 8 + i) as f64 * 0.1;
                        gpu.process(&params, &black[..97 * 61 * 4], 97, 61);
                    }
                });
            }
        });
        assert_eq!(gpu.state.read().len(), 1);
        // Failure after a GPU write must release scratch and restore CPU optics.
        stage.force_failure.store(true, Relaxed);
        ep.light_x += 1.0;
        let failed = gpu.process(&ep, &black[..97 * 61 * 4], 97, 61);
        assert_eq!(stage.scratch.lock().bytes(), 0);
        stage.force_failure.store(false, Relaxed);
        let count = stage.frames.load(Relaxed);
        ep.light_x += 1.0;
        gpu.process(&ep, &black[..97 * 61 * 4], 97, 61);
        assert_eq!(
            stage.frames.load(Relaxed),
            count,
            "failure retry must back off"
        );
        assert!(failed.iter().any(|v| *v != 0));
        gpu.release_caches();
        assert!(gpu.state.read().is_empty());
        gpu.process(&ep, &black[..97 * 61 * 4], 97, 61);
        assert_eq!(stage.frames.load(Relaxed), count + 1);
        // Deliberate wgpu device destruction exercises the real loss callback.
        gpu.device.destroy();
        let _ = gpu.device.poll(PollType::Wait);
        let actual = gpu.process(&ep, &black[..97 * 61 * 4], 97, 61);
        assert!(!gpu.is_healthy());
        assert!(gpu.state.read().is_empty());
        assert_eq!(stage.scratch.lock().bytes(), 0);
        assert_eq!(
            actual,
            crate::flare::process(&ep, &black[..97 * 61 * 4], 97, 61)
        );
    }

    #[test]
    #[ignore = "HDR ghost raster/blur CPU comparison at center, edge and outside"]
    fn ghost_gpu_qualification() {
        let gpu = GpuProcessor::new();
        let raster = crate::ghost_gpu::GhostGpu::new(&gpu.device).expect("ghost pipeline");
        let blur = crate::bloom_gpu::BloomGpu::new(&gpu.device).unwrap();
        let (w, h) = (321, 181);
        let mut exercised = 0;
        for lens in 0..4 {
            for angle in [0.0, 0.32, 0.7] {
                for radius in [0.0, 0.005, 0.04] {
                    let cfg = physical::Config {
                        ray_grid: 12,
                        ghost_blur: radius,
                        ..Default::default()
                    };
                    let sources = [physical::BrightSource {
                        angle_x: angle,
                        angle_y: 0.05,
                        rgb: [1.0, 0.7, 0.3],
                    }];
                    let lens = physical::bundled_lens(lens);
                    let Some(mesh) = physical::ghost_mesh(&lens, &sources, w, h, cfg) else {
                        continue;
                    };
                    let reference = physical::render(&lens, &sources, w, h, cfg);
                    let mut encoder = gpu
                        .device
                        .create_command_encoder(&CommandEncoderDescriptor::default());
                    let output = raster.encode(&gpu.device, &mut encoder, &mesh, w, h, cfg, &blur);
                    let readback = gpu.device.create_buffer(&BufferDescriptor {
                        label: None,
                        size: (w * h * 16) as u64,
                        usage: BufferUsages::MAP_READ | BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    });
                    encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, (w * h * 16) as u64);
                    gpu.queue.submit(Some(encoder.finish()));
                    let (tx, rx) = std::sync::mpsc::channel();
                    let slice = readback.slice(..);
                    slice.map_async(MapMode::Read, move |r| {
                        tx.send(r).unwrap();
                    });
                    gpu.device.poll(PollType::Wait).unwrap();
                    rx.recv().unwrap().unwrap();
                    let mapped = slice.get_mapped_range();
                    let mut max_error = 0.0_f32;
                    for (i, expected) in reference.iter().enumerate() {
                        for c in 0..3 {
                            let actual = f32::from_le_bytes(
                                mapped[i * 16 + c * 4..i * 16 + c * 4 + 4]
                                    .try_into()
                                    .unwrap(),
                            );
                            let error = (actual - expected[c]).abs();
                            max_error = max_error.max(error);
                            assert!(actual.is_finite() && error<=1e-6+expected[c].abs()*1e-4,"{} angle={angle} blur={radius} pixel={i} ch={c} CPU={} GPU={actual} error={error}",lens.name,expected[c]);
                        }
                    }
                    drop(mapped);
                    readback.unmap();
                    exercised += 1;
                    eprintln!(
                        "ghost HDR {} angle={angle} blur={radius} max={max_error}",
                        lens.name
                    );
                }
            }
        }
        assert!(
            exercised >= 24,
            "must exercise actual GPU output, not only fallbacks"
        );
    }

    #[test]
    #[ignore = "GPU resident pipeline: full-frame A/B, cache, fallback and timing"]
    fn resident_gpu_qualification() {
        use std::sync::atomic::Ordering::Relaxed;
        let mut gpu = GpuProcessor::new();
        gpu.source_gpu = None;
        gpu.bloom_gpu = None;
        gpu.resident_gpu = None;
        let mut stage =
            Some(crate::resident_gpu::ResidentGpu::new(&gpu.device).expect("resident pipeline"));
        if std::env::var("ONMK_GPU_GHOSTS").as_deref() == Ok("1") {
            assert!(
                stage.as_ref().unwrap().ghosts.is_some(),
                "requested ghost pipeline must initialize"
            );
        }
        let mut ep = EffectParams {
            light_x: 715.5,
            light_y: 465.0,
            global_scale: 1.0,
            global_brightness: 1.0,
            flare_opacity: 1.0,
            source_opacity: 1.0,
            source_mode: 1,
            transfer_mode: 4,
            physical_enabled: true,
            ray_grid: 26,
            physical_gain: 3000.0,
            max_area_boost: 8.0,
            ghost_intensity: 2.9,
            ghost_normalize: true,
            physical_ghost_blur: 0.005,
            physical_blur_passes: 3,
            bloom_strength: 0.6,
            bloom_radius: 0.018,
            bloom_passes: 3,
            bloom_octaves: 4,
            bloom_chromatic: true,
            starburst_blades: 6,
            unified_diffraction: 0.18,
            hotspot_intensity: 1.6,
            ..Default::default()
        };
        let compare = |a: &[u8], b: &[u8]| {
            assert_eq!(a.len(), b.len());
            let max = a.iter().zip(b).map(|(a, b)| a.abs_diff(*b)).max().unwrap();
            let changed = a.iter().zip(b).filter(|(a, b)| a != b).count();
            assert!(
                max <= 1 && changed <= a.len() / 1000 + 1,
                "max={max}, changed={changed}"
            );
            (max, changed)
        };
        for lens in 0..4 {
            let (w, h) = if lens < 2 { (1920, 1080) } else { (321, 181) };
            ep.lens_preset = lens;
            ep.ray_grid = if lens < 2 { 26 } else { 12 };
            ep.light_y = h as f64 * 0.43;
            let mut src = vec![0u8; w * h * 4];
            for p in src.chunks_exact_mut(4) {
                p[0] = 255;
            }
            let mut cpu_times = Vec::new();
            let mut gpu_times = Vec::new();
            for frame in 0..5 {
                ep.light_x = w as f64 * (0.37 + frame as f64 * 0.02);
                let start = std::time::Instant::now();
                let cpu = gpu.process(&ep, &src, w, h);
                cpu_times.push(start.elapsed());
                gpu.resident_gpu = stage.take();
                let count = gpu.resident_gpu.as_ref().unwrap().frames.load(Relaxed);
                let ghost_count = gpu
                    .resident_gpu
                    .as_ref()
                    .unwrap()
                    .ghost_frames
                    .load(Relaxed);
                let reads = gpu
                    .resident_gpu
                    .as_ref()
                    .unwrap()
                    .status_readback_bytes
                    .load(Relaxed);
                let start = std::time::Instant::now();
                let actual = gpu.process(&ep, &src, w, h);
                gpu_times.push(start.elapsed());
                assert_eq!(
                    count + 1,
                    gpu.resident_gpu.as_ref().unwrap().frames.load(Relaxed),
                    "must not silently fall back"
                );
                if lens < 2 && gpu.resident_gpu.as_ref().unwrap().ghosts.is_some() {
                    assert_eq!(
                        ghost_count + 1,
                        gpu.resident_gpu
                            .as_ref()
                            .unwrap()
                            .ghost_frames
                            .load(Relaxed),
                        "ghost stage must execute on GPU"
                    );
                }
                assert_eq!(
                    reads + 4,
                    gpu.resident_gpu
                        .as_ref()
                        .unwrap()
                        .status_readback_bytes
                        .load(Relaxed)
                );
                let (max, changed) = compare(&cpu, &actual);
                let start = std::time::Instant::now();
                assert_eq!(actual, gpu.process(&ep, &src, w, h));
                let warm = start.elapsed();
                assert_eq!(
                    count + 1,
                    gpu.resident_gpu.as_ref().unwrap().frames.load(Relaxed)
                );
                // Composite-only changes reuse the resident optical buffer.
                ep.global_brightness = 0.8;
                gpu.process(&ep, &src, w, h);
                ep.global_brightness = 1.0;
                assert_eq!(
                    count + 1,
                    gpu.resident_gpu.as_ref().unwrap().frames.load(Relaxed)
                );
                if lens == 1 && frame == 0 {
                    std::fs::create_dir_all("target").unwrap();
                    for (pixels, name) in [(&cpu, "cpu"), (&actual, "resident")] {
                        let rgba: Vec<_> = pixels
                            .chunks_exact(4)
                            .flat_map(|p| [p[1], p[2], p[3], p[0]])
                            .collect();
                        image::RgbaImage::from_raw(w as u32, h as u32, rgba)
                            .unwrap()
                            .save(format!("target/resident-{name}.png"))
                            .unwrap();
                    }
                }
                if frame == 4 {
                    // Test failure AFTER a GPU write, with a matching old CPU
                    // cache entry: destination must be restored, not reused.
                    gpu.resident_gpu
                        .as_ref()
                        .unwrap()
                        .force_failure
                        .store(true, Relaxed);
                    for s in gpu.state.write().values_mut() {
                        s.resident_cache.get_mut().take();
                    }
                    assert_eq!(cpu, gpu.process(&ep, &src, w, h));
                    gpu.resident_gpu
                        .as_ref()
                        .unwrap()
                        .force_failure
                        .store(false, Relaxed);
                    gpu.state.write().clear();
                    compare(&cpu, &gpu.process(&ep, &src, w, h));
                }
                stage = gpu.resident_gpu.take();
                eprintln!(
                    "resident lens={lens} frame={frame} max={max} changed={changed} warm={warm:?}"
                );
            }
            cpu_times.sort();
            gpu_times.sort();
            eprintln!(
                "resident {w}x{h} lens={lens}: CPU median={:?}, resident median={:?}",
                cpu_times[2], gpu_times[2]
            );
        }
    }

    #[test]
    #[ignore = "resident boundary, colored input, ocular, legacy, and fallback cases"]
    fn resident_edge_cases() {
        use std::sync::atomic::Ordering::Relaxed;
        let mut gpu = GpuProcessor::new();
        gpu.source_gpu = None;
        gpu.bloom_gpu = None;
        gpu.resident_gpu = None;
        let mut stage = Some(crate::resident_gpu::ResidentGpu::new(&gpu.device).unwrap());
        for case in 0..24 {
            let (w, h) = match case % 4 {
                0 => (1, 1),
                1 => (1, 31),
                2 => (31, 1),
                _ => (97, 61),
            };
            let ep = EffectParams {
                light_x: w as f64 * 0.6,
                light_y: h as f64 * 0.3,
                global_scale: 1.0,
                global_brightness: 1.0,
                flare_opacity: 1.0,
                source_opacity: 0.7,
                source_mode: case % 3 + 1,
                source_threshold: 0.5,
                source_downsample: 1,
                transfer_mode: case % 6,
                physical_enabled: case % 4 != 0,
                lens_preset: (case % 4) as usize,
                ray_grid: 8,
                physical_gain: 3000.0,
                ghost_intensity: 2.9,
                max_area_boost: 8.0,
                ghost_normalize: true,
                physical_ghost_blur: 0.005,
                physical_blur_passes: 3,
                bloom_strength: if case % 5 == 0 { 0.0 } else { 0.6 },
                bloom_radius: 0.018,
                bloom_passes: (case % 4) as usize,
                bloom_octaves: (case % 6 + 1) as usize,
                bloom_chromatic: case % 2 == 0,
                starburst_blades: 6,
                unified_diffraction: 0.18,
                hotspot_intensity: 1.6,
                style_preset: if case % 2 == 0 { 0 } else { 6 },
                ocular_enabled: case % 3 == 0,
                ocular_pupil_mm: 6.0,
                ocular_age: 30.0,
                ocular_seed: 17,
                ocular_corona: 0.75,
                ocular_corona_radius: 0.22,
                ocular_veil: 0.28,
                ocular_veil_radius: 0.65,
                ocular_halo: 0.18,
                ocular_halo_radius: 0.14,
                ..Default::default()
            };
            let src: Vec<_> = (0..w * h)
                .flat_map(|i| {
                    [
                        if i % 2 == 0 { 255 } else { 128 },
                        (i % 251) as u8,
                        ((i * 3) % 251) as u8,
                        ((i * 7) % 251) as u8,
                    ]
                })
                .collect();
            let cpu = gpu.process(&ep, &src, w, h);
            gpu.resident_gpu = stage.take();
            let count = gpu.resident_gpu.as_ref().unwrap().frames.load(Relaxed);
            let actual = gpu.process(&ep, &src, w, h);
            assert_eq!(
                count + 1,
                gpu.resident_gpu.as_ref().unwrap().frames.load(Relaxed)
            );
            let max = cpu
                .iter()
                .zip(&actual)
                .map(|(a, b)| a.abs_diff(*b))
                .max()
                .unwrap();
            assert!(max <= 1, "case={case} max={max}");
            assert_eq!(actual, gpu.process(&ep, &src, w, h));
            stage = gpu.resident_gpu.take();
        }
        // Unsupported workloads fail before allocation and before touching the
        // destination. Large images retain the established CPU implementation.
        let output = gpu.device.create_buffer(&BufferDescriptor {
            label: None,
            size: 16,
            usage: BufferUsages::STORAGE,
            mapped_at_creation: false,
        });
        let ep = EffectParams::default();
        let cfg = physical::Config::default();
        let r = stage.as_ref().unwrap();
        assert!(r
            .render(&gpu.device, &gpu.queue, &output, &ep, &[], 8192, 8192, cfg)
            .is_none());
        assert!(r
            .render(&gpu.device, &gpu.queue, &output, &ep, &[], 0, 0, cfg)
            .is_none());
        for strength in [f32::NAN, f32::INFINITY] {
            let cfg = physical::Config {
                bloom_strength: strength,
                ..cfg
            };
            assert!(r
                .render(&gpu.device, &gpu.queue, &output, &ep, &[], 1, 1, cfg)
                .is_none());
        }
        // Keep resident mode enabled across optical edits; each one must
        // invalidate the device-local cache, including highlight changes.
        let mut reference = GpuProcessor::new();
        reference.resident_gpu = None;
        reference.source_gpu = None;
        reference.bloom_gpu = None;
        gpu.resident_gpu = stage.take();
        let (w, h) = (97, 61);
        let mut src = vec![0u8; w * h * 4];
        for p in src.chunks_exact_mut(4) {
            p[0] = 255;
        }
        let mut ep = EffectParams {
            light_x: 33.0,
            light_y: 21.0,
            source_mode: 1,
            source_threshold: 0.5,
            source_downsample: 1,
            global_scale: 1.0,
            global_brightness: 1.0,
            flare_opacity: 1.0,
            physical_enabled: true,
            ray_grid: 8,
            physical_gain: 3000.0,
            ghost_intensity: 2.9,
            bloom_strength: 0.6,
            bloom_radius: 0.018,
            bloom_passes: 3,
            bloom_octaves: 4,
            starburst_blades: 6,
            hotspot_intensity: 1.6,
            ..Default::default()
        };
        for step in 0..7 {
            match step {
                1 => ep.light_x += 5.0,
                2 => ep.bloom_strength = 0.9,
                3 => ep.lens_preset = 1,
                4 => ep.source_mode = 3,
                5 => src[4 * 123 + 1..4 * 123 + 4].fill(255),
                6 => ep.style_preset = 6,
                _ => {}
            }
            let before = gpu.resident_gpu.as_ref().unwrap().frames.load(Relaxed);
            let actual = gpu.process(&ep, &src, w, h);
            assert_eq!(
                before + 1,
                gpu.resident_gpu.as_ref().unwrap().frames.load(Relaxed)
            );
            let cpu = reference.process(&ep, &src, w, h);
            assert!(actual.iter().zip(&cpu).all(|(a, b)| a.abs_diff(*b) <= 1));
        }
    }

    #[test]
    #[ignore = "GPU source optics: HDR tolerance, final pixels and timing"]
    fn gpu_source_qualification() {
        let mut gpu = GpuProcessor::new();
        gpu.source_gpu = None;
        let stage =
            crate::source_gpu::SourceGpu::new(&gpu.device).expect("source pipeline validation");
        let mut ep = EffectParams {
            unified_diffraction: 0.18,
            global_scale: 1.0,
            starburst_blades: 6,
            hotspot_intensity: 1.6,
            bloom_radius: 0.018,
            bloom_strength: 0.6,
            ..Default::default()
        };
        let sources = [
            physical::BrightSource {
                angle_x: -0.16,
                angle_y: 0.05,
                rgb: [1.0, 0.7, 0.3],
            },
            physical::BrightSource {
                angle_x: 0.0,
                angle_y: 0.0,
                rgb: [0.2, 0.8, 1.0],
            },
            physical::BrightSource {
                angle_x: 0.8,
                angle_y: -0.4,
                rgb: [3.0, 0.1, 0.5],
            },
        ];
        for (case, (w, h, scale, blades, angle)) in [
            (1, 1, 1.0, 6, 0.0),
            (31, 17, 0.1, 3, 0.0),
            (97, 61, 1.4, 7, 23.0),
            (257, 129, 8.0, 16, -179.0),
            (1920, 1080, 1.0, 6, 0.0),
        ]
        .into_iter()
        .enumerate()
        {
            ep.global_scale = scale;
            ep.starburst_blades = blades;
            ep.flare_angle = angle;
            let start = std::time::Instant::now();
            let cpu = crate::source_optics::render(&ep, &sources, w, h);
            let cpu_time = start.elapsed();
            let start = std::time::Instant::now();
            let actual = stage
                .render(&gpu.device, &gpu.queue, &ep, &sources, w, h)
                .expect("GPU render");
            let gpu_time = start.elapsed();
            let mut max_error = 0.0_f32;
            for (index, (a, b)) in cpu
                .iter()
                .flatten()
                .zip(actual.iter().flatten())
                .enumerate()
            {
                let error = (a - b).abs();
                max_error = max_error.max(error);
                assert!(
                    error <= 2e-4 + a.abs() * 1e-4,
                    "case={case} channel={index} CPU={a} GPU={b} error={error}"
                );
            }
            eprintln!("source qualification {w}x{h}: CPU={cpu_time:?} GPU={gpu_time:?} max HDR error={max_error}");
        }
        assert!(stage
            .render(&gpu.device, &gpu.queue, &ep, &sources, 0, 0)
            .is_none());
        assert!(stage
            .render(&gpu.device, &gpu.queue, &ep, &sources, 8192, 8192)
            .is_none());
        let bad = [physical::BrightSource {
            angle_x: f64::NAN,
            angle_y: 0.0,
            rgb: [1.0; 3],
        }];
        assert!(stage
            .render(&gpu.device, &gpu.queue, &ep, &bad, 31, 17)
            .is_none());
        let empty = stage
            .render(&gpu.device, &gpu.queue, &ep, &[], 31, 17)
            .unwrap();
        assert!(empty.iter().flatten().all(|v| *v == 0.0));

        // Selection/truncation, equal energies, dense overlapping sources,
        // minimum and maximum aperture settings, and zero diffraction.
        let many: Vec<_> = (0..10)
            .map(|i| physical::BrightSource {
                angle_x: (i as f64 - 4.0) * 0.045,
                angle_y: (i as f64 - 5.0) * 0.025,
                rgb: if i % 2 == 0 {
                    [1.0, 0.7, 0.3]
                } else {
                    [0.2, 0.8, 1.0]
                },
            })
            .collect();
        for diffraction in [0.0, 1.0] {
            ep.unified_diffraction = diffraction;
            let cpu = crate::source_optics::render(&ep, &many, 193, 109);
            let actual = stage
                .render(&gpu.device, &gpu.queue, &ep, &many, 193, 109)
                .unwrap();
            for (a, b) in cpu.iter().flatten().zip(actual.iter().flatten()) {
                assert!((a - b).abs() <= 2e-4 + a.abs() * 1e-4);
            }
        }

        // Complete composite checks, including both image halves and all four
        // lens prescriptions. CPU reference is produced before toggling stage.
        for lens in 0..4 {
            ep = EffectParams {
                light_x: 195.0,
                light_y: 51.0,
                source_mode: 1,
                physical_enabled: true,
                lens_preset: lens,
                ray_grid: 12,
                physical_gain: 3000.0,
                ghost_intensity: 2.9,
                global_brightness: 1.0,
                global_scale: 1.0,
                hotspot_intensity: 1.6,
                starburst_blades: 6,
                unified_diffraction: 0.18,
                bloom_radius: 0.018,
                bloom_strength: 0.6,
                bloom_octaves: 4,
                bloom_passes: 3,
                flare_opacity: 1.0,
                ..Default::default()
            };
            let (w, h) = (321, 181);
            let mut input = vec![0u8; w * h * 4];
            for p in input.chunks_exact_mut(4) {
                p[0] = 255;
            }
            gpu.source_gpu = None;
            gpu.optical_cache.lock().take();
            let reference = gpu.process(&ep, &input, w, h);
            gpu.source_gpu = Some(crate::source_gpu::SourceGpu::new(&gpu.device).unwrap());
            gpu.optical_cache.lock().take();
            let actual = gpu.process(&ep, &input, w, h);
            let max = reference
                .iter()
                .zip(&actual)
                .map(|(a, b)| a.abs_diff(*b))
                .max()
                .unwrap();
            assert!(max <= 1, "lens {lens} final 8-bit error {max}");
            assert!(actual
                .chunks_exact(4)
                .any(|p| p[1..].iter().any(|v| *v > 0)));
            assert_eq!(actual, gpu.process(&ep, &input, w, h));
            eprintln!("lens {lens} final max 8-bit error={max}");
        }
    }

    #[test]
    #[ignore = "colored ARGB byte-layout references; capture explicitly on baseline"]
    fn colored_byte_layout_references() {
        let (w, h) = (37, 19);
        let gpu = GpuProcessor::new();
        let mut input = vec![0; w * h * 4];
        for (i, p) in input.chunks_mut(4).enumerate() {
            p.copy_from_slice(&[
                [0, 1, 128, 255][i % 4],
                (i * 17 % 256) as u8,
                (i * 31 % 256) as u8,
                (i * 47 % 256) as u8,
            ]);
        }
        let dir = std::path::Path::new("target/perf-colored-reference");
        let capture = !dir.exists();
        assert!(!capture || std::env::var_os("ONMK_CAPTURE_PERF_REFERENCE").is_some());
        std::fs::create_dir_all(dir).unwrap();
        let mut ep = EffectParams {
            light_x: 15.0,
            light_y: 6.0,
            global_scale: 1.0,
            global_brightness: 1.0,
            source_opacity: 0.7,
            flare_opacity: 0.5,
            source_downsample: 8,
            source_threshold: 0.5,
            ghost_intensity: 1.0,
            hotspot_intensity: 1.6,
            starburst_blades: 6,
            ..Default::default()
        };
        for source in [1, 2] {
            for transfer in 0..=5 {
                ep.source_mode = source;
                ep.transfer_mode = transfer;
                let pixels = gpu.process(&ep, &input, w, h);
                let path = dir.join(format!("source-{source}-transfer-{transfer}.png"));
                if capture {
                    image::RgbaImage::from_raw(w as u32, h as u32, pixels)
                        .unwrap()
                        .save(path)
                        .unwrap();
                } else {
                    assert_eq!(pixels, image::open(path).unwrap().into_rgba8().into_raw());
                }
            }
        }
    }

    #[test]
    #[ignore = "1080p optical combination A/B"]
    fn benchmark_combine() {
        let a = vec![[0.1, 0.2, 0.3]; 1920 * 1080];
        let b = vec![[0.4, 0.5, 0.6]; a.len()];
        let e = vec![[0.7, 0.8, 0.9]; a.len()];
        let s = vec![[1.0, 1.1, 1.2]; a.len()];
        for run in 0..3 {
            let start = std::time::Instant::now();
            let old: Vec<[f32; 4]> = a
                .iter()
                .zip(&b)
                .zip(&e)
                .zip(&s)
                .map(|(((g, b), e), s)| {
                    let safe = |v: f32| if v.is_finite() { v } else { 0.0 };
                    [
                        safe(g[0] + b[0]) + safe(e[0] + s[0] * 1.0) / 2.9_f32.max(0.001),
                        safe(g[1] + b[1]) + safe(e[1] + s[1] * 1.0) / 2.9_f32.max(0.001),
                        safe(g[2] + b[2]) + safe(e[2] + s[2] * 1.0) / 2.9_f32.max(0.001),
                        0.0,
                    ]
                })
                .collect();
            let old_time = start.elapsed();
            let start = std::time::Instant::now();
            let new = combine_optical_layers(&a, &b, &e, &s, 2.9, 1.0, 4);
            let new_time = start.elapsed();
            assert_eq!(old, new);
            eprintln!("combine {run}: old={old_time:?}, parallel={new_time:?}");
        }
    }

    #[test]
    fn combined_layers_match_original() {
        let values = [
            0.0,
            -0.0,
            1.0,
            -2.0,
            1e30,
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
        ];
        let layer: Vec<_> = (0..262147)
            .map(|i| [values[i % 8], values[(i + 1) % 8], values[(i + 2) % 8]])
            .collect();
        for (gain, modern) in [(0.0, 1.0), (2.9, 1.0), (-1.0, 0.0)] {
            let actual = combine_optical_layers(&layer, &layer, &layer, &layer, gain, modern, 4);
            for (i, p) in actual.iter().enumerate() {
                let safe = |v: f32| if v.is_finite() { v } else { 0.0 };
                for c in 0..3 {
                    let expected = safe(layer[i][c] + layer[i][c])
                        + safe(layer[i][c] + layer[i][c] * modern) / gain.max(0.001);
                    assert_eq!(p[c].to_bits(), expected.to_bits());
                }
                assert_eq!(p[3], 0.0);
            }
        }
    }

    #[test]
    #[ignore = "five-round 1080p benchmark with persistent exact-output references"]
    fn benchmark_five_rounds_1080p() {
        let gpu = GpuProcessor::new();
        let (w, h) = (1920, 1080);
        let mut src = vec![0u8; w * h * 4];
        for p in src.chunks_mut(4) {
            p[0] = 255;
        }
        let mut ep = EffectParams {
            light_x: 690.0,
            light_y: 465.0,
            global_scale: 1.0,
            global_brightness: 1.0,
            flare_opacity: 1.0,
            source_opacity: 1.0,
            source_mode: 1,
            transfer_mode: 4,
            physical_enabled: true,
            ray_grid: 26,
            physical_gain: 3000.0,
            max_area_boost: 8.0,
            ghost_intensity: 2.9,
            ghost_normalize: true,
            physical_ghost_blur: 0.005,
            physical_blur_passes: 3,
            bloom_strength: 0.6,
            bloom_radius: 0.018,
            bloom_passes: 3,
            bloom_octaves: 4,
            bloom_chromatic: true,
            starburst_blades: 6,
            unified_diffraction: 0.18,
            hotspot_intensity: 1.6,
            ..Default::default()
        };
        let directory = std::path::Path::new("target/perf-five-rounds-reference");
        let capture_reference = !directory.exists();
        assert!(!capture_reference || std::env::var_os("ONMK_CAPTURE_PERF_REFERENCE").is_some(),
            "Missing references; explicitly set ONMK_CAPTURE_PERF_REFERENCE=1 on the baseline build to capture them first");
        std::fs::create_dir_all(directory).unwrap();
        for lens in 0..2 {
            ep.lens_preset = lens;
            ep.light_x = 690.0;
            let _ = gpu.process(&ep, &src, w, h);
            let mut times = Vec::new();
            for frame in 0..5 {
                ep.light_x = 715.5 + frame as f64 * 25.5;
                let start = std::time::Instant::now();
                let pixels = gpu.process(&ep, &src, w, h);
                let elapsed = start.elapsed();
                times.push(elapsed);
                let path = directory.join(format!("lens-{lens}-frame-{frame}.png"));
                if !capture_reference {
                    assert_eq!(
                        image::open(&path).unwrap().into_rgba8().into_raw(),
                        pixels,
                        "{}",
                        path.display()
                    );
                } else {
                    image::RgbaImage::from_raw(w as u32, h as u32, pixels)
                        .unwrap()
                        .save(&path)
                        .unwrap();
                }
                eprintln!("1080p lens={lens} frame={frame} elapsed={elapsed:?}");
            }
            times.sort();
            eprintln!("1080p lens={lens} median={:?}", times[2]);
        }
    }

    #[test]
    #[ignore = "1080p full-frame CPU/GPU source optics A/B"]
    fn benchmark_gpu_source_full() {
        let mut gpu = GpuProcessor::new();
        gpu.source_gpu = None;
        gpu.bloom_gpu = None;
        let mut stage = Some(crate::source_gpu::SourceGpu::new(&gpu.device).unwrap());
        let mut bloom_stage = Some(crate::bloom_gpu::BloomGpu::new(&gpu.device).unwrap());
        let (w, h) = (1920, 1080);
        let mut src = vec![0u8; w * h * 4];
        for p in src.chunks_exact_mut(4) {
            p[0] = 255;
        }
        let mut ep = EffectParams {
            light_x: 715.5,
            light_y: 465.0,
            global_scale: 1.0,
            global_brightness: 1.0,
            flare_opacity: 1.0,
            source_opacity: 1.0,
            source_mode: 1,
            transfer_mode: 4,
            physical_enabled: true,
            ray_grid: 26,
            physical_gain: 3000.0,
            max_area_boost: 8.0,
            ghost_intensity: 2.9,
            ghost_normalize: true,
            physical_ghost_blur: 0.005,
            physical_blur_passes: 3,
            bloom_strength: 0.6,
            bloom_radius: 0.018,
            bloom_passes: 3,
            bloom_octaves: 4,
            bloom_chromatic: true,
            starburst_blades: 6,
            unified_diffraction: 0.18,
            hotspot_intensity: 1.6,
            ..Default::default()
        };
        for lens in 0..2 {
            ep.lens_preset = lens;
            let mut cpu_times = Vec::new();
            let mut gpu_times = Vec::new();
            for frame in 0..5 {
                ep.light_x = 715.5 + frame as f64 * 25.5;
                gpu.optical_cache.lock().take();
                let start = std::time::Instant::now();
                let reference = gpu.process(&ep, &src, w, h);
                cpu_times.push(start.elapsed());
                gpu.source_gpu = stage.take();
                gpu.bloom_gpu = bloom_stage.take();
                gpu.optical_cache.lock().take();
                let completed = gpu
                    .source_gpu
                    .as_ref()
                    .unwrap()
                    .completed
                    .load(std::sync::atomic::Ordering::Relaxed);
                let start = std::time::Instant::now();
                let actual = gpu.process(&ep, &src, w, h);
                gpu_times.push(start.elapsed());
                assert_eq!(
                    completed + 1,
                    gpu.source_gpu
                        .as_ref()
                        .unwrap()
                        .completed
                        .load(std::sync::atomic::Ordering::Relaxed),
                    "GPU stage must run, not silently fall back"
                );
                stage = gpu.source_gpu.take();
                assert!(
                    gpu.bloom_gpu
                        .as_ref()
                        .unwrap()
                        .completed
                        .load(std::sync::atomic::Ordering::Relaxed)
                        > 0
                );
                bloom_stage = gpu.bloom_gpu.take();
                gpu.optical_cache.lock().take();
                let max = reference
                    .iter()
                    .zip(&actual)
                    .map(|(a, b)| a.abs_diff(*b))
                    .max()
                    .unwrap();
                let changed = reference
                    .iter()
                    .zip(&actual)
                    .filter(|(a, b)| a != b)
                    .count();
                assert!(
                    max <= 1 && changed < reference.len() / 1000,
                    "lens={lens} frame={frame} max={max} changed={changed}"
                );
                eprintln!(
                    "full A/B lens={lens} frame={frame} max={max} changed channels={changed}"
                );
                if lens == 1 && frame == 0 {
                    std::fs::create_dir_all("target").unwrap();
                    let save = |bytes: &[u8], path: &str| {
                        let rgba: Vec<u8> = bytes
                            .chunks_exact(4)
                            .flat_map(|p| [p[1], p[2], p[3], p[0]])
                            .collect();
                        image::RgbaImage::from_raw(w as u32, h as u32, rgba)
                            .unwrap()
                            .save(path)
                            .unwrap();
                    };
                    save(&reference, "target/gpu-source-cpu-reference.png");
                    save(&actual, "target/gpu-source-candidate.png");
                }
            }
            cpu_times.sort();
            gpu_times.sort();
            eprintln!(
                "full 1080p lens={lens} CPU-source median={:?} GPU-source median={:?}",
                cpu_times[2], gpu_times[2]
            );
        }
    }

    #[test]
    #[ignore = "GPU Bloom reference comparisons and resource fallback"]
    fn gpu_bloom_qualification() {
        let gpu = GpuProcessor::new();
        let stage =
            crate::bloom_gpu::BloomGpu::new(&gpu.device).expect("Bloom pipeline validation");
        for (w, h) in [(1, 1), (1, 31), (31, 1), (97, 61), (1920, 1080)] {
            let mut input = vec![[0.0; 3]; w * h];
            input[0] = [0.2, 0.7, 1.0];
            input[w * h / 2] = [1.0, 0.3, 0.6];
            input[w * h - 1] = [0.1, 0.4, 0.2];
            for octaves in [1, 4, 6] {
                let cfg = physical::Config {
                    bloom_strength: 0.6,
                    bloom_radius: 0.018,
                    bloom_passes: 3,
                    bloom_octaves: octaves,
                    bloom_chromatic: true,
                    ..Default::default()
                };
                let start = std::time::Instant::now();
                let reference = physical::bloom(&input, w, h, cfg);
                let cpu_time = start.elapsed();
                let start = std::time::Instant::now();
                let actual = stage
                    .render(&gpu.device, &gpu.queue, &input, w, h, cfg)
                    .expect("GPU Bloom");
                let gpu_time = start.elapsed();
                let mut max_error = 0.0_f32;
                for (i, (a, b)) in reference
                    .iter()
                    .flatten()
                    .zip(actual.iter().flatten())
                    .enumerate()
                {
                    let error = (a - b).abs();
                    max_error = max_error.max(error);
                    assert!(
                        error <= 1e-7 + a.abs() * 5e-5,
                        "{w}x{h},octaves={octaves},channel={i},CPU={a},GPU={b}"
                    );
                }
                eprintln!("Bloom {w}x{h} octaves={octaves} CPU={cpu_time:?} GPU={gpu_time:?} max={max_error}");
            }
        }
        for v in [-0.0, -1.0, f32::NAN, f32::INFINITY, f32::MAX] {
            assert!(stage
                .render(
                    &gpu.device,
                    &gpu.queue,
                    &[[v; 3]],
                    1,
                    1,
                    physical::Config::default()
                )
                .is_none());
        }
        // Dense input also exercises prefix cancellation, edge windows, and
        // the radius>=frame branch, not just sparse impulse fixtures.
        let (w, h) = (79, 43);
        let dense: Vec<_> = (0..w * h)
            .map(|i| {
                [
                    (i % 17) as f32 / 17.0,
                    (i % 7) as f32 / 7.0,
                    (i % 23) as f32 / 23.0,
                ]
            })
            .collect();
        for passes in [0, 1, 4] {
            for radius in [0.0, 0.02, 10.0] {
                let cfg = physical::Config {
                    bloom_strength: 2.5,
                    bloom_radius: radius,
                    bloom_passes: passes,
                    bloom_octaves: 6,
                    bloom_chromatic: false,
                    ..Default::default()
                };
                let cpu = physical::bloom(&dense, w, h, cfg);
                let actual = stage
                    .render(&gpu.device, &gpu.queue, &dense, w, h, cfg)
                    .unwrap();
                for (a, b) in cpu.iter().flatten().zip(actual.iter().flatten()) {
                    assert!(
                        (a - b).abs() <= 1e-7 + a.abs() * 5e-5,
                        "dense CPU={a} GPU={b}"
                    );
                }
            }
        }
    }

    #[test]
    #[ignore = "compare existing AEXCompat CPU/GPU PNG results"]
    fn aex_gpu_output_comparison() {
        let directory = std::env::var("ONMK_AEX_GPU_TEST_DIR").expect("AEXCompat result directory");
        for size in ["1920x1080", "1280x720"] {
            let cpu = image::open(format!("{directory}/gpu-optics-cpu-{size}.png"))
                .unwrap()
                .into_rgba8();
            let gpu = image::open(format!("{directory}/gpu-optics-gpu-{size}.png"))
                .unwrap()
                .into_rgba8();
            assert_eq!(cpu.dimensions(), gpu.dimensions());
            let expected = if size == "1920x1080" {
                (1920, 1080)
            } else {
                (1280, 720)
            };
            assert_eq!(gpu.dimensions(), expected);
            let mut count = 0;
            let mut max = 0;
            for (a, b) in cpu.as_raw().iter().zip(gpu.as_raw()) {
                max = max.max(a.abs_diff(*b));
                count += usize::from(a != b);
            }
            assert!(
                max <= 1 && count < cpu.as_raw().len() / 1000,
                "{size}: max={max} changed={count}"
            );
            eprintln!("AEXCompat {size}: max 8-bit difference={max}, changed channels={count}");
        }
    }

    #[test]
    #[ignore = "release GPU cache benchmark and invalidation verification"]
    fn optical_cache_matches_uncached_render() {
        check_cache(1280, 720);
    }

    #[test]
    #[ignore = "1080p GPU benchmark and invalidation verification"]
    fn optical_cache_1080p() {
        check_cache(1920, 1080);
    }

    fn check_cache(w: usize, h: usize) {
        let gpu = GpuProcessor::new();
        let mut src = vec![0u8; w * h * 4];
        for p in src.chunks_mut(4) {
            p[0] = 255;
        }
        let mut ep = EffectParams {
            light_x: 460.0 * w as f64 / 1280.0,
            light_y: 310.0 * h as f64 / 720.0,
            global_scale: 1.0,
            global_brightness: 1.0,
            flare_opacity: 1.0,
            source_opacity: 1.0,
            source_mode: 1,
            transfer_mode: 4,
            physical_enabled: true,
            ray_grid: 26,
            physical_gain: 3000.0,
            max_area_boost: 8.0,
            ghost_intensity: 2.9,
            ghost_normalize: true,
            physical_ghost_blur: 0.005,
            physical_blur_passes: 3,
            bloom_strength: 0.6,
            bloom_radius: 0.018,
            bloom_passes: 3,
            bloom_octaves: 4,
            bloom_chromatic: true,
            starburst_blades: 6,
            unified_diffraction: 0.18,
            hotspot_intensity: 1.6,
            ..Default::default()
        };
        let start = std::time::Instant::now();
        let expected = gpu.process(&ep, &src, w, h);
        let uploads = gpu
            .optical_uploads
            .load(std::sync::atomic::Ordering::Relaxed);
        let cold = start.elapsed();
        for run in 0..3 {
            let start = std::time::Instant::now();
            assert_eq!(expected, gpu.process(&ep, &src, w, h));
            assert_eq!(
                uploads,
                gpu.optical_uploads
                    .load(std::sync::atomic::Ordering::Relaxed)
            );
            eprintln!("cache run {run}: cold={cold:?}, warm={:?}", start.elapsed());
        }
        // A replacement GPU buffer must be uploaded even while CPU optics hit.
        gpu.state.write().clear();
        assert_eq!(expected, gpu.process(&ep, &src, w, h));
        assert_eq!(
            uploads + 1,
            gpu.optical_uploads
                .load(std::sync::atomic::Ordering::Relaxed)
        );
        for run in 0..5 {
            ep.light_x += 17.0;
            let start = std::time::Instant::now();
            let moving = gpu.process(&ep, &src, w, h);
            eprintln!("moving light {run}: {:?}", start.elapsed());
            assert_eq!(moving, gpu.process(&ep, &src, w, h));
        }
        // Composite-only changes may reuse optics, but must match fresh optics.
        let uploads = gpu
            .optical_uploads
            .load(std::sync::atomic::Ordering::Relaxed);
        ep.global_brightness = 0.4;
        src[1] = 180;
        let cached = gpu.process(&ep, &src, w, h);
        assert_eq!(
            uploads,
            gpu.optical_uploads
                .load(std::sync::atomic::Ordering::Relaxed)
        );
        gpu.optical_cache.lock().take();
        assert_eq!(cached, gpu.process(&ep, &src, w, h));
        assert_eq!(
            uploads + 1,
            gpu.optical_uploads
                .load(std::sync::atomic::Ordering::Relaxed)
        );
        // Optical controls and source changes must invalidate the previous image.
        for change in 0..3 {
            let uploads = gpu
                .optical_uploads
                .load(std::sync::atomic::Ordering::Relaxed);
            match change {
                0 => ep.light_x += 200.0,
                1 => ep.lens_preset = 1,
                _ => ep.bloom_strength = 1.4,
            }
            let actual = gpu.process(&ep, &src, w, h);
            assert_eq!(
                uploads + 1,
                gpu.optical_uploads
                    .load(std::sync::atomic::Ordering::Relaxed)
            );
            gpu.optical_cache.lock().take();
            assert_eq!(actual, gpu.process(&ep, &src, w, h));
        }
        ep.source_mode = 3;
        ep.source_downsample = 8;
        ep.source_threshold = 0.5;
        for y in 120..128 {
            for x in 180..188 {
                let off = (y * w + x) * 4;
                src[off + 1..off + 4].fill(255);
            }
        }
        let highlights = gpu.process(&ep, &src, w, h);
        gpu.optical_cache.lock().take();
        assert_eq!(highlights, gpu.process(&ep, &src, w, h));
        let smaller = gpu.process(&ep, &src[..640 * 360 * 4], 640, 360);
        gpu.optical_cache.lock().take();
        assert_eq!(smaller, gpu.process(&ep, &src[..640 * 360 * 4], 640, 360));
    }
}

impl GpuProcessor {
    pub fn new() -> Self {
        let mut instance_desc = InstanceDescriptor::default();
        if instance_desc.backends.contains(Backends::DX12)
            && instance_desc.flags.contains(InstanceFlags::VALIDATION)
        {
            instance_desc.backends.remove(Backends::DX12);
        }

        let instance = Instance::new(&instance_desc);
        let adapter = pollster::block_on(instance.request_adapter(&RequestAdapterOptions {
            power_preference: PowerPreference::HighPerformance,
            ..Default::default()
        }))
        .expect("No GPU adapter found");

        let (device, queue) = pollster::block_on(adapter.request_device(&DeviceDescriptor {
            label: None,
            required_features: adapter.features(),
            required_limits: adapter.limits(),
            memory_hints: MemoryHints::Performance,
            trace: Trace::Off,
        }))
        .expect("Failed to create GPU device");

        let healthy = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let lost = healthy.clone();
        device.set_device_lost_callback(move |_, message| {
            lost.store(false, std::sync::atomic::Ordering::Release);
            log::error!("onmkFlare GPU device lost: {message}");
        });
        let failed = healthy.clone();
        device.on_uncaptured_error(Box::new(move |error| {
            failed.store(false, std::sync::atomic::Ordering::Release);
            log::error!("onmkFlare uncaptured GPU error: {error}");
        }));

        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("onmk_flare"),
            source: ShaderSource::Wgsl(std::borrow::Cow::Borrowed(include_str!("../shader.wgsl"))),
        });

        let bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: None,
            entries: &[
                bgl_uniform(0),
                bgl_storage_ro(1),
                bgl_storage_rw(2),
                bgl_storage_ro(3),
            ],
        });

        let pipeline_layout = device.create_pipeline_layout(&PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let flare_pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
            module: &shader,
            entry_point: Some("flare_pass"),
            label: None,
            layout: Some(&pipeline_layout),
            compilation_options: Default::default(),
            cache: Default::default(),
        });

        // Explicit opt-in until cross-device/AE qualification is complete.
        let source_gpu = if std::env::var("ONMK_GPU_SOURCE_OPTICS").as_deref() == Ok("1") {
            crate::source_gpu::SourceGpu::new(&device)
        } else {
            None
        };
        let bloom_gpu = if std::env::var("ONMK_GPU_BLOOM").as_deref() == Ok("1") {
            crate::bloom_gpu::BloomGpu::new(&device)
        } else {
            None
        };
        let resident_gpu = if std::env::var("ONMK_GPU_RESIDENT").as_deref() == Ok("1") {
            crate::resident_gpu::ResidentGpu::new(&device)
        } else {
            None
        };
        Self {
            healthy,
            device,
            queue,
            flare_pipeline,
            bind_group_layout,
            state: RwLock::new(HashMap::new()),
            optical_cache: parking_lot::Mutex::new(None),
            source_gpu,
            bloom_gpu,
            optical_gpu_guard: parking_lot::Mutex::new(()),
            resident_gpu,
            #[cfg(test)]
            optical_uploads: AtomicUsize::new(0),
        }
    }

    fn create_buffers(&self, w: u32, h: u32) -> BufferState {
        let n = (w * h) as u64;
        let mk = |sz: u64, usage: BufferUsages| {
            self.device.create_buffer(&BufferDescriptor {
                size: sz,
                usage,
                label: None,
                mapped_at_creation: false,
            })
        };

        let source_buf = mk(n * 4, BufferUsages::STORAGE | BufferUsages::COPY_DST);
        let output_buf = mk(n * 4, BufferUsages::STORAGE | BufferUsages::COPY_SRC);
        let staging_buf = mk(n * 4, BufferUsages::MAP_READ | BufferUsages::COPY_DST);
        let params_buf = mk(
            std::mem::size_of::<GpuParams>() as u64,
            BufferUsages::UNIFORM | BufferUsages::COPY_DST,
        );
        let physical_buf = mk(n * 16, BufferUsages::STORAGE | BufferUsages::COPY_DST);

        let bind_group = self.device.create_bind_group(&BindGroupDescriptor {
            label: None,
            layout: &self.bind_group_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,
                    resource: params_buf.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 1,
                    resource: source_buf.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 2,
                    resource: output_buf.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 3,
                    resource: physical_buf.as_entire_binding(),
                },
            ],
        });

        BufferState {
            width: w,
            height: h,
            source_buf,
            output_buf,
            staging_buf,
            params_buf,
            physical_buf,
            bind_group,
            uploaded_optics: parking_lot::Mutex::new(std::sync::Weak::new()),
            resident_cache: parking_lot::Mutex::new(None),
        }
    }

    fn get_state(
        &self,
        w: u32,
        h: u32,
    ) -> parking_lot::RwLockUpgradableReadGuard<'_, HashMap<usize, BufferState>> {
        let mut lock = self.state.upgradable_read();
        let tid = 0;
        if lock
            .get(&tid)
            .map(|s| s.width != w || s.height != h)
            .unwrap_or(true)
        {
            lock.with_upgraded(|x| {
                // The upgradable guard already serializes frames. Keep one
                // shared frame, not a full-frame allocation per AE worker.
                x.clear();
                self.optical_cache.lock().take();
                if let Some(gpu) = &self.resident_gpu {
                    gpu.release();
                }
                x.insert(tid, self.create_buffers(w, h));
            });
        }
        lock
    }

    pub fn process(&self, ep: &EffectParams, src: &[u8], w: usize, h: usize) -> Vec<u8> {
        // Bound in-flight work across processor instances in this plugin/DLL.
        static FRAME: parking_lot::Mutex<()> = parking_lot::Mutex::new(());
        let _frame = FRAME.lock();
        let valid = w.checked_mul(h).and_then(|n| n.checked_mul(4));
        if w == 0 || h == 0 || valid != Some(src.len()) {
            return src.to_vec();
        }
        let limits = self.device.limits();
        let n = w * h;
        let fits = w <= u32::MAX as usize
            && h <= u32::MAX as usize
            && n as u64 <= (512 * 1024 * 1024) / 28
            && (n as u64) * 16 <= limits.max_storage_buffer_binding_size as u64
            && (n as u64) * 16 <= limits.max_buffer_size
            && w.div_ceil(16) <= limits.max_compute_workgroups_per_dimension as usize
            && h.div_ceil(16) <= limits.max_compute_workgroups_per_dimension as usize;
        if self.is_healthy() && fits {
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                self.process_inner(ep, src, w, h)
            })) {
                Ok(Some(result)) if self.is_healthy() => return result,
                _ => {
                    self.healthy
                        .store(false, std::sync::atomic::Ordering::Release);
                    log::error!("onmkFlare GPU frame failed; discarding caches and using CPU");
                }
            }
        }
        self.release_caches();
        crate::flare::process(ep, src, w, h)
    }

    pub(crate) fn is_healthy(&self) -> bool {
        self.healthy.load(std::sync::atomic::Ordering::Acquire)
    }

    #[cfg(test)]
    pub(crate) fn destroy_device_for_test(&self) {
        self.device.destroy();
        let _ = self.device.poll(PollType::Wait);
    }

    pub(crate) fn release_caches(&self) {
        self.state.write().clear();
        self.optical_cache.lock().take();
        if let Some(gpu) = &self.resident_gpu {
            gpu.release();
        }
    }

    fn process_inner(&self, ep: &EffectParams, src: &[u8], w: usize, h: usize) -> Option<Vec<u8>> {
        let (ww, hh, n) = (w as u32, h as u32, w * h);
        let lock = self.get_state(ww, hh);
        let state = lock.get(&0).unwrap();

        self.queue.write_buffer(&state.source_buf, 0, &src[..n * 4]);

        let cfg = physical::Config {
            ray_grid: ep.ray_grid.min(32),
            min_ghost: 1e-7,
            gain: ep.physical_gain,
            normalize: ep.ghost_normalize,
            max_area_boost: ep.max_area_boost,
            ghost_blur: ep.physical_ghost_blur,
            ghost_blur_passes: ep.physical_blur_passes,
            surface_raster: true,
            bloom_strength: ep.bloom_strength,
            bloom_radius: ep.bloom_radius,
            bloom_passes: ep.bloom_passes,
            bloom_octaves: ep.bloom_octaves,
            bloom_chromatic: ep.bloom_chromatic,
        };
        let fov_h = 50_f64.to_radians();
        let fov_v = 2.0 * ((h as f64 / w as f64) * (fov_h * 0.5).tan()).atan();
        let mut sources = if ep.source_mode >= 2 {
            physical::prune_sources(
                physical::extract_sources(src, w, h, ep.source_threshold, ep.source_downsample),
                4,
            )
        } else {
            Vec::new()
        };
        if ep.source_mode == 1 || ep.source_mode == 3 {
            sources.push(physical::BrightSource {
                angle_x: (ep.light_x / w as f64 - 0.5) * fov_h,
                angle_y: (ep.light_y / h as f64 - 0.5) * fov_v,
                rgb: [1.0; 3],
            });
        }
        let key = optical_key(ep);
        let resident = if let Some(gpu) = &self.resident_gpu {
            let mut cache = state.resident_cache.lock();
            if cache
                .as_ref()
                .is_some_and(|(k, s)| *k == key && *s == sources)
            {
                true
            } else {
                *cache = None;
                // A failed GPU pass may have written the destination. Force a
                // CPU upload even if its cached Arc still matches a prior frame.
                *state.uploaded_optics.lock() = std::sync::Weak::new();
                let _guard = self.optical_gpu_guard.lock();
                if gpu
                    .render(
                        &self.device,
                        &self.queue,
                        &state.physical_buf,
                        ep,
                        &sources,
                        w,
                        h,
                        cfg,
                    )
                    .is_some()
                {
                    *cache = Some((key.clone(), sources.clone()));
                    true
                } else {
                    false
                }
            }
        } else {
            state.resident_cache.lock().take();
            false
        };
        if !resident {
            // If a resident attempt failed, use the CPU reference rather than
            // retrying the older GPU readback stages on the same device.
            let allow_layer_gpu = self.resident_gpu.is_none();
            let cached = self
                .optical_cache
                .lock()
                .as_ref()
                .filter(|cache| {
                    cache.params == key && cache.sources == sources && cache.dimensions == (w, h)
                })
                .map(|cache| Arc::clone(&cache.pixels));
            let physical_layer = if let Some(pixels) = cached {
                pixels
            } else {
                // These layers are independent and previously blocked one another on
                // the render thread.  Compute them concurrently without reducing ray
                // count, blur passes, or any optical detail.
                let (ghosts, bloom, ocular, source_optics) = std::thread::scope(|scope| {
                    let physical_job = scope.spawn(|| {
                        #[cfg(test)]
                        let start = std::time::Instant::now();
                        if ep.physical_enabled {
                            let lens = physical::bundled_lens(ep.lens_preset);
                            let result = physical::render(&lens, &sources, w, h, cfg);
                            #[cfg(test)]
                            eprintln!("ghosts: {:?}", start.elapsed());
                            result
                        } else {
                            vec![[0.0; 3]; n]
                        }
                    });
                    let bloom_job = scope.spawn(|| {
                        #[cfg(test)]
                        let start = std::time::Instant::now();
                        if ep.physical_enabled {
                            let bloom_source = physical::source_layer(&sources, w, h);
                            let result = self
                                .bloom_gpu
                                .as_ref()
                                .filter(|_| allow_layer_gpu)
                                .and_then(|gpu| {
                                    let _guard = self.optical_gpu_guard.lock();
                                    gpu.render(&self.device, &self.queue, &bloom_source, w, h, cfg)
                                })
                                .unwrap_or_else(|| physical::bloom(&bloom_source, w, h, cfg));
                            #[cfg(test)]
                            eprintln!("bloom: {:?}", start.elapsed());
                            result
                        } else {
                            vec![[0.0; 3]; n]
                        }
                    });
                    let ocular_job = scope.spawn(|| crate::ocular::render(ep, &sources, w, h));
                    let source_job = scope.spawn(|| {
                        #[cfg(test)]
                        let start = std::time::Instant::now();
                        let result = self
                            .source_gpu
                            .as_ref()
                            .filter(|_| allow_layer_gpu)
                            .and_then(|gpu| {
                                let _guard = self.optical_gpu_guard.lock();
                                gpu.render(&self.device, &self.queue, ep, &sources, w, h)
                            })
                            .unwrap_or_else(|| crate::source_optics::render(ep, &sources, w, h));
                        #[cfg(test)]
                        eprintln!("source optics: {:?}", start.elapsed());
                        result
                    });
                    (
                        physical_job.join().expect("physical flare worker"),
                        bloom_job.join().expect("bloom worker"),
                        ocular_job.join().expect("ocular flare worker"),
                        source_job.join().expect("source optics worker"),
                    )
                });
                #[cfg(test)]
                let start = std::time::Instant::now();
                let workers = std::thread::available_parallelism().map_or(1, |n| n.get().min(4));
                let physical_layer = combine_optical_layers(
                    &ghosts,
                    &bloom,
                    &ocular,
                    &source_optics,
                    ep.ghost_intensity as f32,
                    if ep.style_preset < 6 { 1.0 } else { 0.0 },
                    workers,
                );
                #[cfg(test)]
                eprintln!("combine: {:?}", start.elapsed());
                let pixels = Arc::new(physical_layer);
                // One shared entry, at most 128 MiB, rather than a full-frame cache per
                // AE render thread. In-flight readers retain their own Arc safely.
                let mut cache = self.optical_cache.lock();
                *cache = if pixels.len() <= (128 * 1024 * 1024) / 16 {
                    Some(OpticalCache {
                        params: key,
                        sources,
                        dimensions: (w, h),
                        pixels: Arc::clone(&pixels),
                    })
                } else {
                    None
                };
                pixels
            };
            {
                let mut uploaded = state.uploaded_optics.lock();
                let same = uploaded
                    .upgrade()
                    .is_some_and(|previous| Arc::ptr_eq(&previous, &physical_layer));
                if !same {
                    self.queue.write_buffer(
                        &state.physical_buf,
                        0,
                        as_bytes_slice(physical_layer.as_slice()),
                    );
                    *uploaded = Arc::downgrade(&physical_layer);
                    #[cfg(test)]
                    self.optical_uploads
                        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                }
            }
        }
        if resident {
            self.optical_cache.lock().take();
        }
        let flicker_mod = compute_flicker(ep);
        let (edge_bright, edge_scale) = compute_edge_trigger(ep, w, h);

        let cooke_a = [[0.0_f32; 4]; 12];
        let cooke_b = [[0.0_f32; 4]; 12];
        let cooke_c = [[0.0_f32; 4]; 12];

        let params = GpuParams {
            width: ww,
            height: hh,
            light_x: ep.light_x as f32,
            light_y: ep.light_y as f32,
            center_x: w as f32 * 0.5,
            center_y: h as f32 * 0.5,
            global_brightness: ep.global_brightness as f32,
            global_scale: ep.global_scale as f32,
            flare_angle: ep.flare_angle as f32,
            hotspot_intensity: if ep.style_preset < 6 {
                0.0
            } else {
                ep.hotspot_intensity as f32
            },
            hotspot_size: ep.hotspot_size as f32,
            glow_intensity: if ep.style_preset < 6 {
                0.0
            } else {
                ep.glow_intensity as f32
            },
            glow_radius: ep.glow_radius as f32,
            glow_falloff: ep.glow_falloff as f32,
            streak_intensity: ep.streak_intensity as f32,
            streak_length: ep.streak_length as f32,
            streak_width: ep.streak_width as f32,
            streak_count: ep.streak_count as u32,
            streak_rotation: ep.streak_rotation as f32,
            stripe_intensity: ep.stripe_intensity as f32,
            stripe_length: ep.stripe_length as f32,
            stripe_width: ep.stripe_width as f32,
            ring_intensity: ep.ring_intensity as f32,
            ring_radius: ep.ring_radius as f32,
            ring_width: ep.ring_width as f32,
            ring_chromatic: ep.ring_chromatic as f32,
            ring_spectrum: if ep.ring_spectrum { 1 } else { 0 },
            starburst_intensity: if ep.style_preset < 6 {
                0.0
            } else {
                ep.starburst_intensity as f32
            },
            starburst_radius: ep.starburst_radius as f32,
            starburst_blades: ep.starburst_blades as u32,
            ghost_intensity: ep.ghost_intensity as f32,
            ghost_count: ep.ghost_count as u32,
            ghost_spread: ep.ghost_spread as f32,
            ghost_size: ep.ghost_size as f32,
            ghost_chromatic: ep.ghost_chromatic as f32,
            edge_bright_mult: edge_bright as f32,
            edge_scale_mult: edge_scale as f32,
            atmosphere_amount: ep.atmosphere_amount as f32,
            atmosphere_scale: ep.atmosphere_scale as f32,
            chromatic_amount: ep.chromatic_amount as f32,
            flicker_mod: flicker_mod as f32,
            source_opacity: ep.source_opacity as f32,
            flare_opacity: ep.flare_opacity as f32,
            transfer_mode: ep.transfer_mode as u32,
            source_mode: ep.source_mode as u32,
            _padding: [0; 3],
            glow_color: [
                ep.glow_color[0] as f32,
                ep.glow_color[1] as f32,
                ep.glow_color[2] as f32,
                0.0,
            ],
            streak_color: [
                ep.streak_color[0] as f32,
                ep.streak_color[1] as f32,
                ep.streak_color[2] as f32,
                0.0,
            ],
            stripe_color: [
                ep.stripe_color[0] as f32,
                ep.stripe_color[1] as f32,
                ep.stripe_color[2] as f32,
                0.0,
            ],
            ring_color: [
                ep.ring_color[0] as f32,
                ep.ring_color[1] as f32,
                ep.ring_color[2] as f32,
                0.0,
            ],
            starburst_color: [
                ep.starburst_color[0] as f32,
                ep.starburst_color[1] as f32,
                ep.starburst_color[2] as f32,
                0.0,
            ],
            ghost_color: [
                ep.ghost_color[0] as f32,
                ep.ghost_color[1] as f32,
                ep.ghost_color[2] as f32,
                0.0,
            ],
            cooke_ghost_a: cooke_a,
            cooke_ghost_b: cooke_b,
            cooke_ghost_c: cooke_c,
        };

        self.queue
            .write_buffer(&state.params_buf, 0, as_bytes(&params));

        let wg_x = (ww + 15) / 16;
        let wg_y = (hh + 15) / 16;

        let mut enc = self
            .device
            .create_command_encoder(&CommandEncoderDescriptor { label: None });
        {
            let mut cp = enc.begin_compute_pass(&ComputePassDescriptor::default());
            cp.set_pipeline(&self.flare_pipeline);
            cp.set_bind_group(0, &state.bind_group, &[]);
            cp.dispatch_workgroups(wg_x, wg_y, 1);
        }
        enc.copy_buffer_to_buffer(&state.output_buf, 0, &state.staging_buf, 0, (n * 4) as u64);
        self.queue.submit(Some(enc.finish()));

        let slice = state.staging_buf.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(MapMode::Read, move |v| {
            let _ = tx.send(v);
        });
        if self.device.poll(PollType::Wait).is_err() {
            return None;
        }

        let mut result = vec![0u8; n * 4];
        if let Ok(Ok(())) = rx.recv_timeout(std::time::Duration::from_secs(5)) {
            let data = slice.get_mapped_range();
            result.copy_from_slice(&data[..n * 4]);
            drop(data);
            state.staging_buf.unmap();
        } else {
            state.staging_buf.unmap();
            return None;
        }
        Some(result)
    }
}

fn compute_flicker(ep: &EffectParams) -> f64 {
    if ep.flicker_amount <= 0.0 {
        return 1.0;
    }
    let phase = ep.flicker_phase * std::f64::consts::PI / 180.0;
    let noise = (phase * 2.137).sin() * 0.5
        + (phase * 5.891).sin() * 0.25
        + (phase * 11.23).sin() * 0.125
        + (phase * 23.71).sin() * 0.0625;
    let norm = noise / 0.9375;
    1.0 - ep.flicker_amount * 0.5 * (1.0 - norm)
}

fn compute_edge_trigger(ep: &EffectParams, w: usize, h: usize) -> (f64, f64) {
    if ep.edge_width <= 0.0 {
        return (1.0, 1.0);
    }
    let wf = w as f64;
    let hf = h as f64;
    let dist_left = ep.light_x;
    let dist_right = wf - ep.light_x;
    let dist_top = ep.light_y;
    let dist_bottom = hf - ep.light_y;
    let min_dist = dist_left.min(dist_right).min(dist_top).min(dist_bottom);
    let zone = ep.edge_width * wf.min(hf) * 0.5;
    if zone <= 0.0 {
        return (1.0, 1.0);
    }
    let t = (1.0 - (min_dist / zone).min(1.0)).max(0.0);
    let trigger = t * t * (3.0 - 2.0 * t);
    let bright = (1.0 + (ep.edge_brightness - 1.0) * trigger).max(1.0);
    let sc = (1.0 + (ep.edge_scale - 1.0) * trigger).max(1.0);
    (bright, sc)
}

fn bgl_uniform(binding: u32) -> BindGroupLayoutEntry {
    BindGroupLayoutEntry {
        binding,
        visibility: ShaderStages::COMPUTE,
        count: None,
        ty: BindingType::Buffer {
            ty: BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: BufferSize::new(std::mem::size_of::<GpuParams>() as _),
        },
    }
}

#[cfg(test)]
mod shader_validation_test {
    use super::*;

    #[test]
    fn creates_gpu_pipeline_with_physical_flare_buffer() {
        let _processor = GpuProcessor::new();
    }
}

fn bgl_storage_ro(binding: u32) -> BindGroupLayoutEntry {
    BindGroupLayoutEntry {
        binding,
        visibility: ShaderStages::COMPUTE,
        count: None,
        ty: BindingType::Buffer {
            ty: BufferBindingType::Storage { read_only: true },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
    }
}

fn bgl_storage_rw(binding: u32) -> BindGroupLayoutEntry {
    BindGroupLayoutEntry {
        binding,
        visibility: ShaderStages::COMPUTE,
        count: None,
        ty: BindingType::Buffer {
            ty: BufferBindingType::Storage { read_only: false },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
    }
}

fn as_bytes<T: Sized>(val: &T) -> &[u8] {
    unsafe { std::slice::from_raw_parts(val as *const T as *const u8, std::mem::size_of::<T>()) }
}

fn as_bytes_slice<T: Sized>(slice: &[T]) -> &[u8] {
    unsafe { std::slice::from_raw_parts(slice.as_ptr() as *const u8, std::mem::size_of_val(slice)) }
}
