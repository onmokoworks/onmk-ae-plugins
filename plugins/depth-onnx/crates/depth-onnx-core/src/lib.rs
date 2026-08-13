//! Shared Depth ONNX logic: manifest parsing, model discovery, ONNX inference.

pub mod engine;
pub mod manifest;
pub mod ort_loader;
pub mod preprocess;
pub mod registry;

pub use engine::{DepthEngine, InferenceRequest, InferenceResult};
pub use manifest::{load_manifest, ModelManifest, Preprocess};
pub use registry::{build_model_popup_names, default_scan_roots, scan_model_roots, BundledModel};

pub const SUPPORTED_SIZES: [i32; 3] = [266, 392, 518];
pub const USER_ERROR_PREFIX: &str = "Depth ONNX: ";
