//! Ordered tile-list rasterization: one invocation owns each pixel.
use crate::{bloom_gpu::BloomGpu, physical, physical::GhostMesh};
use wgpu::{util::DeviceExt, *};
pub struct GhostGpu {
    pipeline: ComputePipeline,
}
impl GhostGpu {
    pub fn new(device: &Device) -> Option<Self> {
        device.push_error_scope(ErrorFilter::Validation);
        let shader = device.create_shader_module(ShaderModuleDescriptor {
            label: Some("ghost raster"),
            source: ShaderSource::Wgsl(include_str!("../ghost_raster.wgsl").into()),
        });
        let pipeline = device.create_compute_pipeline(&ComputePipelineDescriptor {
            label: Some("ghost raster"),
            layout: None,
            module: &shader,
            entry_point: Some("raster"),
            compilation_options: Default::default(),
            cache: None,
        });
        if pollster::block_on(device.pop_error_scope()).is_some() {
            return None;
        }
        Some(Self { pipeline })
    }
    // Caller owns memory limits, error scopes, submission and finite checks.
    pub fn encode(
        &self,
        device: &Device,
        encoder: &mut CommandEncoder,
        mesh: &GhostMesh,
        w: usize,
        h: usize,
        cfg: physical::Config,
        blur: &BloomGpu,
    ) -> Buffer {
        fn bytes<T>(v: &[T]) -> &[u8] {
            unsafe { std::slice::from_raw_parts(v.as_ptr().cast(), std::mem::size_of_val(v)) }
        }
        let upload = |v: &[u8], usage| {
            device.create_buffer_init(&util::BufferInitDescriptor {
                label: None,
                contents: v,
                usage,
            })
        };
        let params = upload(
            bytes(&[w as u32, h as u32, w.div_ceil(16) as u32, 0]),
            BufferUsages::UNIFORM,
        );
        let triangles = upload(
            if mesh.triangles.is_empty() {
                &[0u8; 64]
            } else {
                bytes(&mesh.triangles)
            },
            BufferUsages::STORAGE,
        );
        let ranges = upload(bytes(&mesh.ranges), BufferUsages::STORAGE);
        let indices = upload(
            if mesh.indices.is_empty() {
                &[0u8; 4]
            } else {
                bytes(&mesh.indices)
            },
            BufferUsages::STORAGE,
        );
        let output = device.create_buffer(&BufferDescriptor {
            label: None,
            size: (w * h * 16) as u64,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let buffers = [&params, &triangles, &ranges, &indices, &output];
        let entries: Vec<_> = buffers
            .iter()
            .enumerate()
            .map(|(i, b)| BindGroupEntry {
                binding: i as u32,
                resource: b.as_entire_binding(),
            })
            .collect();
        let bind = device.create_bind_group(&BindGroupDescriptor {
            label: None,
            layout: &self.pipeline.get_bind_group_layout(0),
            entries: &entries,
        });
        {
            let mut pass = encoder.begin_compute_pass(&ComputePassDescriptor::default());
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind, &[]);
            pass.dispatch_workgroups(w.div_ceil(8) as u32, h.div_ceil(8) as u32, 1);
        }
        let radius = (cfg.ghost_blur * ((w * w + h * h) as f32).sqrt()) as usize;
        if cfg.ghost_blur > 0.0 && radius > 0 {
            blur.encode_ghost_blur(device, encoder, output, w, h, cfg)
        } else {
            output
        }
    }
}
