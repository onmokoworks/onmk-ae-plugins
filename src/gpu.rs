// CUDA GPU backend for RefractionDispersion.
//
// Goal of this module: own a single CUDA context for the plugin process,
// expose `available()` so the render path can decide GPU vs CPU, and
// eventually expose `render()` that runs the refract kernel.
//
// Stage 2: device probe only. No PTX, no kernels.

use std::sync::OnceLock;

use cudarc::driver::CudaContext;
use std::sync::Arc;

/// Lazily-initialized CUDA state shared by all render calls in this process.
/// `None` means we attempted init and it failed (no driver / no device / etc).
/// Once set, the state is cached for the rest of the process lifetime.
struct GpuState {
    _ctx: Arc<CudaContext>,
}

static GPU_STATE: OnceLock<Option<GpuState>> = OnceLock::new();

fn init() -> Option<GpuState> {
    let ctx = CudaContext::new(0).ok()?;
    Some(GpuState { _ctx: ctx })
}

fn state() -> Option<&'static GpuState> {
    GPU_STATE.get_or_init(init).as_ref()
}

/// True if a usable CUDA device exists and we successfully initialized a context.
/// Cheap after the first call (cached).
pub fn available() -> bool {
    state().is_some()
}

/// Stub render entry. Stage 2 returns None so lib.rs always falls back to CPU.
/// Stage 3 will actually invoke a passthrough kernel.
#[allow(dead_code)]
pub fn render(
    _params: &crate::RefractParams,
    _src: &[u8],
    _mask: Option<&[u8]>,
    _bg: Option<&[u8]>,
    _w: usize,
    _h: usize,
) -> Option<Vec<u8>> {
    None
}
