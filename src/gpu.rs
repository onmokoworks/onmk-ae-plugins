// CUDA GPU backend for RefractionDispersion.
//
// Owns one CudaContext per process, loads the PTX module compiled by
// build.rs, and runs kernels for the refract pipeline.
//
// Stage 3: passthrough kernel only. Proves the end-to-end pipeline
// (host -> device memcpy -> kernel launch -> device -> host memcpy)
// before we commit to the real refraction shader.

use std::sync::{Arc, OnceLock};

use cudarc::driver::{CudaContext, CudaFunction, CudaModule, LaunchConfig, PushKernelArg};
use cudarc::nvrtc::Ptx;

const PTX_SRC: &str = include_str!(concat!(env!("OUT_DIR"), "/refract.ptx"));

struct GpuState {
    ctx: Arc<CudaContext>,
    _module: Arc<CudaModule>,
    passthrough: CudaFunction,
}

static GPU_STATE: OnceLock<Option<GpuState>> = OnceLock::new();

fn init() -> Option<GpuState> {
    let ctx = CudaContext::new(0).ok()?;
    let module = ctx.load_module(Ptx::from_src(PTX_SRC)).ok()?;
    let passthrough = module.load_function("passthrough_kernel").ok()?;
    Some(GpuState {
        ctx,
        _module: module,
        passthrough,
    })
}

fn state() -> Option<&'static GpuState> {
    GPU_STATE.get_or_init(init).as_ref()
}

/// True if a usable CUDA device exists, the PTX loaded, and the kernel
/// symbol was found. Cheap after the first call (cached).
pub fn available() -> bool {
    state().is_some()
}

/// Run the current GPU pipeline on `src` (ARGB u8) and return a new buffer.
/// Stage 3 implementation: passthrough. Ignores params/mask/bg and just
/// copies src through the kernel so we can verify the plumbing.
/// Returns None if GPU is not available or any CUDA call fails, in which
/// case the caller should fall back to CPU.
pub fn render(
    _params: &crate::RefractParams,
    src: &[u8],
    _mask: Option<&[u8]>,
    _bg: Option<&[u8]>,
    w: usize,
    h: usize,
) -> Option<Vec<u8>> {
    let gpu = state()?;
    let stream = gpu.ctx.default_stream();

    let pixel_count = w * h;
    if pixel_count == 0 || src.len() < pixel_count * 4 {
        return None;
    }

    let d_src = stream.clone_htod::<u8, [u8]>(src).ok()?;
    let mut d_dst = stream.alloc_zeros::<u8>(pixel_count * 4).ok()?;

    let block_x = 16u32;
    let block_y = 16u32;
    let grid_x = ((w as u32) + block_x - 1) / block_x;
    let grid_y = ((h as u32) + block_y - 1) / block_y;
    let cfg = LaunchConfig {
        grid_dim: (grid_x, grid_y, 1),
        block_dim: (block_x, block_y, 1),
        shared_mem_bytes: 0,
    };

    // Bind scalar args to locals so their addresses outlive the launch builder.
    let w_arg: i32 = w as i32;
    let h_arg: i32 = h as i32;

    let mut launch = stream.launch_builder(&gpu.passthrough);
    launch.arg(&d_src);
    launch.arg(&mut d_dst);
    launch.arg(&w_arg);
    launch.arg(&h_arg);
    unsafe { launch.launch(cfg).ok()? };

    let mut host = vec![0u8; pixel_count * 4];
    stream.memcpy_dtoh(&d_dst, &mut host).ok()?;
    stream.synchronize().ok()?;

    Some(host)
}
