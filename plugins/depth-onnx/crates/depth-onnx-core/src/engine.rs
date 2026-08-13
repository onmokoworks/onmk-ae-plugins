use crate::manifest::ModelManifest;
use crate::ort_loader::ensure_ort;
use crate::preprocess::preprocess_to_nchw;
use crate::SUPPORTED_SIZES;
use ort::session::builder::GraphOptimizationLevel;
use ort::session::Session;
use ort::value::Tensor;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct InferenceRequest<'a> {
    pub input_size: i32,
    pub src_width: i32,
    pub src_height: i32,
    pub src_rgba: &'a [f32],
    pub src_is_argb: bool,
}

#[derive(Debug, Clone)]
pub struct InferenceResult {
    pub width: i32,
    pub height: i32,
    pub depth: Vec<f32>,
    pub d_min: f32,
    pub d_max: f32,
}

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("invalid request (empty src or zero size)")]
    InvalidRequest,
    #[error("unsupported input_size {0}")]
    UnsupportedSize(i32),
    #[error("model not configured")]
    NotConfigured,
    #[error("manifest has no variant for size {0}")]
    MissingVariant(i32),
    #[error("session init failed: {0}")]
    SessionInit(String),
    #[error("session returned no outputs")]
    NoOutputs,
    #[error("unexpected output element count {0}")]
    BadOutputCount(usize),
    #[error("session run failed: {0}")]
    SessionRun(String),
}

#[derive(Hash, PartialEq, Eq, Clone)]
struct SessionKey {
    model_dir: PathBuf,
    model_id: String,
    size: i32,
}

pub struct DepthEngine {
    model_dir: PathBuf,
    manifest: ModelManifest,
    sessions: Mutex<HashMap<SessionKey, Session>>,
}

impl DepthEngine {
    pub fn new() -> Result<Self, EngineError> {
        ensure_ort()?;

        Ok(Self {
            model_dir: PathBuf::new(),
            manifest: ModelManifest {
                id: String::new(),
                label: String::new(),
                preprocess: "imagenet_rgb".into(),
                input_name: "pixel_values".into(),
                output_name: "predicted_depth".into(),
                variants: Default::default(),
            },
            sessions: Mutex::new(HashMap::new()),
        })
    }

    pub fn configure(&mut self, model_dir: PathBuf, manifest: ModelManifest) {
        if self.model_dir == model_dir && self.manifest.id == manifest.id {
            return;
        }
        self.model_dir = model_dir;
        self.manifest = manifest;
        self.sessions.lock().clear();
    }

    pub fn run(&self, req: InferenceRequest<'_>) -> Result<InferenceResult, EngineError> {
        if req.src_rgba.is_empty() || req.src_width <= 0 || req.src_height <= 0 {
            return Err(EngineError::InvalidRequest);
        }
        if !SUPPORTED_SIZES.contains(&req.input_size) {
            return Err(EngineError::UnsupportedSize(req.input_size));
        }
        if self.model_dir.as_os_str().is_empty() {
            return Err(EngineError::NotConfigured);
        }

        let variant = self
            .manifest
            .variant_file(req.input_size)
            .ok_or(EngineError::MissingVariant(req.input_size))?;

        let model_path = self.model_dir.join(variant);
        let key = SessionKey {
            model_dir: self.model_dir.clone(),
            model_id: self.manifest.id.clone(),
            size: req.input_size,
        };

        let mut sessions = self.sessions.lock();
        if !sessions.contains_key(&key) {
            let session = build_session(&model_path)?;
            sessions.insert(key.clone(), session);
        }
        let session = sessions.get_mut(&key).expect("session");

        let size = req.input_size;
        let plane = (size * size) as usize;
        let input = preprocess_to_nchw(
            req.src_rgba,
            req.src_width,
            req.src_height,
            req.src_is_argb,
            size,
        );

        let shape = vec![1_i64, 3, size as i64, size as i64];
        let input_tensor = Tensor::from_array((shape, input.into_boxed_slice()))
            .map_err(|e| EngineError::SessionRun(e.to_string()))?;

        let outputs = session
            .run(ort::inputs![self.manifest.input_name.as_str() => input_tensor])
            .map_err(|e| EngineError::SessionRun(e.to_string()))?;

        let output = outputs
            .get(self.manifest.output_name.as_str())
            .ok_or(EngineError::NoOutputs)?;

        let (_output_shape, data) = output
            .try_extract_tensor::<f32>()
            .map_err(|e| EngineError::SessionRun(e.to_string()))?;

        let count = data.len();
        if count == 0 || count != plane {
            return Err(EngineError::BadOutputCount(count));
        }

        let depth = data.to_vec();
        let d_min = depth.iter().copied().fold(f32::INFINITY, f32::min);
        let d_max = depth.iter().copied().fold(f32::NEG_INFINITY, f32::max);

        Ok(InferenceResult {
            width: size,
            height: size,
            depth,
            d_min,
            d_max,
        })
    }
}

fn build_session(model_path: &Path) -> Result<Session, EngineError> {
    let mut builder = Session::builder()
        .map_err(|e| EngineError::SessionInit(e.to_string()))?
        .with_optimization_level(GraphOptimizationLevel::Level3)
        .map_err(|e| EngineError::SessionInit(e.to_string()))?;

    #[cfg(windows)]
    {
        use ort::ep::DirectML;
        builder = builder
            .with_execution_providers([DirectML::default().build()])
            .map_err(|e| EngineError::SessionInit(e.to_string()))?;
    }

    builder
        .commit_from_file(model_path)
        .map_err(|e| EngineError::SessionInit(e.to_string()))
}
