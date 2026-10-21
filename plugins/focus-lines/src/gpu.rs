//! Private wgpu compute device for coverage. AE sees a normal CPU effect: the
//! host world stays in CPU memory, primitives/bins are uploaded, coverage is
//! computed here and downloaded. No AE GPU (SmartRenderGpu) flags are declared.

use crate::raster::{Bins, PixelRect, Scene, MAX_VERTS};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Mutex, OnceLock};
use std::time::Instant;

/// One output strip is limited to this many coverage bytes.
const STRIP_BYTES: u64 = 64 << 20;
/// Scratch buffers are released after a render if they exceed this total.
const SCRATCH_KEEP_BYTES: u64 = 256 << 20;
const UNIFORM_BYTES: u64 = 80;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GpuInfo {
    pub adapter: String,
    pub backend: String,
    pub device_type: String,
    pub driver: String,
}

impl std::fmt::Display for GpuInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} ({}, {}, driver {})",
            self.adapter, self.backend, self.device_type, self.driver
        )
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GpuTiming {
    /// Host -> device upload of primitives, bins and uniforms (queue writes).
    pub upload_ms: f64,
    /// Dispatch + copy + map + host copy, all strips.
    pub dispatch_download_ms: f64,
    pub total_ms: f64,
    pub bytes_up: u64,
    pub bytes_down: u64,
    pub strips: u32,
}

struct Slot {
    buffer: wgpu::Buffer,
    capacity: u64,
}

#[derive(Default)]
struct Scratch {
    verts: Option<Slot>,
    counts: Option<Slot>,
    offsets: Option<Slot>,
    items: Option<Slot>,
    out: Option<Slot>,
    staging: Option<Slot>,
    uniform: Option<Slot>,
}

impl Scratch {
    fn total(&self) -> u64 {
        [
            &self.verts,
            &self.counts,
            &self.offsets,
            &self.items,
            &self.out,
            &self.staging,
            &self.uniform,
        ]
        .iter()
        .map(|s| s.as_ref().map_or(0, |s| s.capacity))
        .sum()
    }
}

struct Ctx {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::ComputePipeline,
    layout: wgpu::BindGroupLayout,
    info: GpuInfo,
    max_binding: u64,
    scratch: Mutex<Scratch>,
}

static CTX: OnceLock<Result<Ctx, String>> = OnceLock::new();
/// Set by the uncaptured-error handler or a failed render; GPU stays disabled
/// for the rest of the process (no re-initialization of a lost device).
static FAILED: AtomicBool = AtomicBool::new(false);
static FAILURE: Mutex<Option<String>> = Mutex::new(None);

fn mark_failed(reason: String) {
    FAILED.store(true, Ordering::SeqCst);
    if let Ok(mut f) = FAILURE.lock() {
        if f.is_none() {
            *f = Some(reason);
        }
    }
}

fn storage_entry(binding: u32, read_only: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn init() -> Result<Ctx, String> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::PRIMARY,
        flags: wgpu::InstanceFlags::empty(),
        ..Default::default()
    });
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        power_preference: wgpu::PowerPreference::HighPerformance,
        force_fallback_adapter: false,
        compatible_surface: None,
    }))
    .map_err(|e| format!("no GPU adapter: {e}"))?;
    let ai = adapter.get_info();
    let info = GpuInfo {
        adapter: ai.name.clone(),
        backend: format!("{:?}", ai.backend),
        device_type: format!("{:?}", ai.device_type),
        driver: format!("{} {}", ai.driver, ai.driver_info)
            .trim()
            .to_string(),
    };
    if ai.device_type == wgpu::DeviceType::Cpu {
        return Err(format!("adapter is a software rasterizer: {info}"));
    }
    let al = adapter.limits();
    let limits = wgpu::Limits {
        max_storage_buffer_binding_size: al.max_storage_buffer_binding_size.min(256 << 20),
        max_buffer_size: al.max_buffer_size.min(512 << 20),
        ..Default::default()
    };
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        label: Some("focus-lines"),
        required_features: wgpu::Features::empty(),
        required_limits: limits.clone(),
        memory_hints: wgpu::MemoryHints::Performance,
        trace: wgpu::Trace::Off,
    }))
    .map_err(|e| format!("GPU device request failed on {info}: {e}"))?;
    // The default handler panics; inside AE that must never happen.
    device.on_uncaptured_error(Box::new(|e: wgpu::Error| {
        mark_failed(format!("uncaptured wgpu error: {e}"))
    }));
    device.push_error_scope(wgpu::ErrorFilter::Validation);
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("focus-lines"),
        source: wgpu::ShaderSource::Wgsl(std::borrow::Cow::Borrowed(include_str!("shader.wgsl"))),
    });
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("focus-lines"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::COMPUTE,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
            storage_entry(1, true),
            storage_entry(2, true),
            storage_entry(3, true),
            storage_entry(4, true),
            storage_entry(5, false),
        ],
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("focus-lines"),
        bind_group_layouts: &[&layout],
        push_constant_ranges: &[],
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: Some("focus-lines"),
        layout: Some(&pipeline_layout),
        module: &shader,
        entry_point: Some("cs_main"),
        compilation_options: Default::default(),
        cache: None,
    });
    if let Some(e) = pollster::block_on(device.pop_error_scope()) {
        return Err(format!("shader/pipeline creation failed: {e}"));
    }
    Ok(Ctx {
        device,
        queue,
        pipeline,
        layout,
        info,
        max_binding: u64::from(limits.max_storage_buffer_binding_size).min(limits.max_buffer_size),
        scratch: Mutex::new(Scratch::default()),
    })
}

fn ctx() -> Result<&'static Ctx, String> {
    if FAILED.load(Ordering::SeqCst) {
        let reason = FAILURE.lock().ok().and_then(|f| f.clone());
        return Err(format!(
            "GPU disabled after an earlier device error: {}",
            reason.unwrap_or_default()
        ));
    }
    CTX.get_or_init(|| {
        catch_unwind(init).unwrap_or_else(|_| Err("panic during GPU initialization".into()))
    })
    .as_ref()
    .map_err(|e| e.clone())
}

/// Adapter description, initializing the device on first use.
pub fn adapter_info() -> Result<GpuInfo, String> {
    ctx().map(|c| c.info.clone())
}

/// Adapter description only if the device was already initialized.
pub fn initialized_info() -> Option<Result<GpuInfo, String>> {
    CTX.get()
        .map(|r| r.as_ref().map(|c| c.info.clone()).map_err(|e| e.clone()))
}

fn ensure<'a>(
    device: &wgpu::Device,
    slot: &'a mut Option<Slot>,
    need: u64,
    limit: u64,
    usage: wgpu::BufferUsages,
    label: &str,
) -> &'a wgpu::Buffer {
    let need = need.max(256).next_multiple_of(256);
    if slot.as_ref().is_none_or(|s| s.capacity < need) {
        // Grow geometrically, but never past the device binding limit.
        let capacity = need.next_power_of_two().min(limit.max(need));
        *slot = Some(Slot {
            buffer: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: capacity,
                usage,
                mapped_at_creation: false,
            }),
            capacity,
        });
    }
    &slot.as_ref().unwrap().buffer
}

fn bytes_u32(values: &[u32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(values.len() * 4);
    for v in values {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out
}

/// GPU coverage for `rect` (row-major `w * h`). Errors are returned, never hidden.
pub fn coverage(
    scene: &Scene,
    rect: PixelRect,
    samples: u32,
    bins: &Bins,
) -> Result<(Vec<f32>, GpuTiming), String> {
    let c = ctx()?;
    let result = catch_unwind(AssertUnwindSafe(|| run(c, scene, rect, samples, bins)))
        .unwrap_or_else(|_| Err("panic during GPU render".into()));
    if let Err(e) = &result {
        if e.starts_with("device:") {
            mark_failed(e.clone());
        }
    }
    result
}

fn run(
    c: &Ctx,
    scene: &Scene,
    rect: PixelRect,
    samples: u32,
    bins: &Bins,
) -> Result<(Vec<f32>, GpuTiming), String> {
    let start = Instant::now();
    let mut timing = GpuTiming::default();
    let w = rect.w as usize;
    let h = rect.h as usize;
    if w == 0 || h == 0 {
        return Ok((Vec::new(), timing));
    }
    if bins.tiles_x == 0 || bins.tile_offsets.len() != (bins.tiles_x * bins.tiles_y) as usize + 1 {
        return Err("invalid bins".into());
    }
    if rect.x0.checked_add(rect.w as i32).is_none() || rect.y0.checked_add(rect.h as i32).is_none()
    {
        return Err("rect overflows i32".into());
    }
    let samples = samples.clamp(1, 64);

    // Pack primitives as f32 absolute buffer coordinates (same for every ROI).
    let mut verts = Vec::with_capacity(scene.prims.len() * MAX_VERTS * 8);
    let mut counts = Vec::with_capacity(scene.prims.len());
    for p in &scene.prims {
        counts.push(u32::from(p.n));
        for i in 0..MAX_VERTS {
            let v = p.v[i];
            verts.extend_from_slice(&(v[0] as f32).to_le_bytes());
            verts.extend_from_slice(&(v[1] as f32).to_le_bytes());
        }
    }
    let counts = bytes_u32(&counts);
    let offsets = bytes_u32(&bins.tile_offsets);
    let items = bytes_u32(&bins.tile_items);
    let row_bytes = w as u64 * 4;
    let strip_rows = ((STRIP_BYTES.min(c.max_binding) / row_bytes).max(1) as usize).min(h);
    for (name, size) in [
        ("primitives", verts.len() as u64),
        ("tile items", items.len() as u64),
        ("tile offsets", offsets.len() as u64),
        ("coverage strip", strip_rows as u64 * row_bytes),
    ] {
        if size > c.max_binding {
            return Err(format!(
                "{name} buffer ({size} bytes) exceeds device binding limit"
            ));
        }
    }

    let mut scratch = c
        .scratch
        .lock()
        .map_err(|_| "GPU scratch mutex poisoned".to_string())?;
    let s = &mut *scratch;
    let dev = &c.device;
    use wgpu::BufferUsages as U;
    dev.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
    dev.push_error_scope(wgpu::ErrorFilter::Validation);
    let lim = c.max_binding;
    let b_verts = ensure(
        dev,
        &mut s.verts,
        verts.len() as u64,
        lim,
        U::STORAGE | U::COPY_DST,
        "verts",
    );
    let b_counts = ensure(
        dev,
        &mut s.counts,
        counts.len() as u64,
        lim,
        U::STORAGE | U::COPY_DST,
        "counts",
    );
    let b_offsets = ensure(
        dev,
        &mut s.offsets,
        offsets.len() as u64,
        lim,
        U::STORAGE | U::COPY_DST,
        "offsets",
    );
    let b_items = ensure(
        dev,
        &mut s.items,
        items.len() as u64,
        lim,
        U::STORAGE | U::COPY_DST,
        "items",
    );
    let strip_bytes = strip_rows as u64 * row_bytes;
    let b_out = ensure(
        dev,
        &mut s.out,
        strip_bytes,
        lim,
        U::STORAGE | U::COPY_SRC,
        "coverage",
    );
    let b_staging = ensure(
        dev,
        &mut s.staging,
        strip_bytes,
        lim,
        U::MAP_READ | U::COPY_DST,
        "staging",
    );
    let b_uniform = ensure(
        dev,
        &mut s.uniform,
        UNIFORM_BYTES,
        lim,
        U::UNIFORM | U::COPY_DST,
        "uniform",
    );

    let up = Instant::now();
    for (buffer, data) in [
        (b_verts, &verts),
        (b_counts, &counts),
        (b_offsets, &offsets),
        (b_items, &items),
    ] {
        if !data.is_empty() {
            c.queue.write_buffer(buffer, 0, data);
        }
    }
    timing.bytes_up = (verts.len() + counts.len() + offsets.len() + items.len()) as u64;
    timing.upload_ms = up.elapsed().as_secs_f64() * 1e3;

    let bind = dev.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("focus-lines"),
        layout: &c.layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: b_uniform.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: b_verts.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: b_counts.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: b_offsets.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: b_items.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: b_out.as_entire_binding(),
            },
        ],
    });
    let hole = scene.hole;
    let mut out = vec![0.0f32; w * h];
    let dl = Instant::now();
    let mut y = 0usize;
    while y < h {
        let rows = strip_rows.min(h - y);
        let mut u = Vec::with_capacity(UNIFORM_BYTES as usize);
        for v in [rect.x0, rect.y0] {
            u.extend_from_slice(&v.to_le_bytes());
        }
        for v in [
            rect.w,
            rect.h,
            y as u32,
            rows as u32,
            bins.tiles_x,
            bins.tiles_y,
        ] {
            u.extend_from_slice(&v.to_le_bytes());
        }
        for v in [bins.tx0, bins.ty0] {
            u.extend_from_slice(&v.to_le_bytes());
        }
        for v in [samples, u32::from(hole.is_some())] {
            u.extend_from_slice(&v.to_le_bytes());
        }
        let (hc, hm) = hole.map_or(([0.0; 2], [[0.0; 2]; 2]), |h| (h.c, h.m));
        for v in [hc[0], hc[1], hm[0][0], hm[0][1], hm[1][0], hm[1][1]] {
            u.extend_from_slice(&(v as f32).to_le_bytes());
        }
        u.extend_from_slice(&[0u8; 8]);
        debug_assert_eq!(u.len() as u64, UNIFORM_BYTES);
        c.queue.write_buffer(b_uniform, 0, &u);
        let mut enc = dev.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("focus-lines"),
        });
        {
            let mut pass = enc.begin_compute_pass(&wgpu::ComputePassDescriptor::default());
            pass.set_pipeline(&c.pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.dispatch_workgroups((w as u32).div_ceil(16), (rows as u32).div_ceil(16), 1);
        }
        let bytes = rows as u64 * row_bytes;
        enc.copy_buffer_to_buffer(b_out, 0, b_staging, 0, bytes);
        c.queue.submit(Some(enc.finish()));
        let slice = b_staging.slice(0..bytes);
        let (tx, rx) = mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        if let Err(e) = dev.poll(wgpu::PollType::Wait) {
            return Err(format!("device: poll failed: {e}"));
        }
        match rx.recv() {
            Ok(Ok(())) => {}
            Ok(Err(e)) => return Err(format!("device: readback map failed: {e}")),
            Err(_) => return Err("device: readback callback dropped".into()),
        }
        {
            let data = slice.get_mapped_range();
            for (dst, src) in out[y * w..(y + rows) * w]
                .iter_mut()
                .zip(data.chunks_exact(4))
            {
                *dst = f32::from_le_bytes([src[0], src[1], src[2], src[3]]);
            }
        }
        b_staging.unmap();
        timing.bytes_down += bytes;
        timing.strips += 1;
        y += rows;
    }
    timing.dispatch_download_ms = dl.elapsed().as_secs_f64() * 1e3;
    let validation = pollster::block_on(dev.pop_error_scope());
    let oom = pollster::block_on(dev.pop_error_scope());
    if let Some(e) = oom {
        *s = Scratch::default();
        return Err(format!("device: out of memory: {e}"));
    }
    if let Some(e) = validation {
        return Err(format!("device: validation error: {e}"));
    }
    if s.total() > SCRATCH_KEEP_BYTES {
        *s = Scratch::default();
    }
    if out.iter().any(|v| !v.is_finite()) {
        return Err("GPU produced non-finite coverage".into());
    }
    timing.total_ms = start.elapsed().as_secs_f64() * 1e3;
    Ok((out, timing))
}
