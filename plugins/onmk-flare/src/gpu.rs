use parking_lot::RwLock;
use std::collections::HashMap;
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
    last_access: AtomicUsize,
    uploaded_optics: parking_lot::Mutex<std::sync::Weak<Vec<[f32; 4]>>>,
}

pub struct GpuProcessor {
    device: Device,
    queue: Queue,
    flare_pipeline: ComputePipeline,
    bind_group_layout: BindGroupLayout,
    state: RwLock<HashMap<std::thread::ThreadId, BufferState>>,
    optical_cache: parking_lot::Mutex<Option<OpticalCache>>,
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

        Self {
            device,
            queue,
            flare_pipeline,
            bind_group_layout,
            state: RwLock::new(HashMap::new()),
            optical_cache: parking_lot::Mutex::new(None),
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
            last_access: AtomicUsize::new(ts()),
            uploaded_optics: parking_lot::Mutex::new(std::sync::Weak::new()),
        }
    }

    fn get_state(
        &self,
        w: u32,
        h: u32,
    ) -> parking_lot::RwLockUpgradableReadGuard<'_, HashMap<std::thread::ThreadId, BufferState>>
    {
        let mut lock = self.state.upgradable_read();
        let tid = std::thread::current().id();
        if lock
            .get(&tid)
            .map(|s| s.width != w || s.height != h)
            .unwrap_or(true)
        {
            lock.with_upgraded(|x| {
                let max = num_cpus::get().max(2) - 1;
                if x.len() > max {
                    let mut keys: Vec<_> = x
                        .iter()
                        .map(|(k, v)| {
                            (*k, v.last_access.load(std::sync::atomic::Ordering::Relaxed))
                        })
                        .collect();
                    keys.sort_by_key(|a| a.1);
                    for (k, _) in keys.iter().take(x.len() - max) {
                        x.remove(k);
                    }
                }
                x.insert(tid, self.create_buffers(w, h));
            });
        }
        lock.get(&tid)
            .unwrap()
            .last_access
            .store(ts(), std::sync::atomic::Ordering::Relaxed);
        lock
    }

    pub fn process(&self, ep: &EffectParams, src: &[u8], w: usize, h: usize) -> Vec<u8> {
        let (ww, hh, n) = (w as u32, h as u32, w * h);
        let lock = self.get_state(ww, hh);
        let state = lock.get(&std::thread::current().id()).unwrap();

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
                        let result = physical::bloom(&bloom_source, w, h, cfg);
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
                    let result = crate::source_optics::render(ep, &sources, w, h);
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
        let (tx, rx) = futures_intrusive::channel::shared::oneshot_channel();
        slice.map_async(MapMode::Read, move |v| tx.send(v).unwrap());
        let _ = self.device.poll(PollType::Wait);

        let mut result = vec![0u8; n * 4];
        if let Some(Ok(())) = pollster::block_on(rx.receive()) {
            let data = slice.get_mapped_range();
            result.copy_from_slice(&data[..n * 4]);
            drop(data);
            state.staging_buf.unmap();
        }
        result
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

fn ts() -> usize {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as usize
}
