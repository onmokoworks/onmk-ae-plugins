use depth_onnx_core::{
    default_scan_roots, load_manifest, scan_model_roots, BundledModel, ModelManifest,
};

#[derive(Clone, Copy, Debug)]
pub enum Resolution {
    R266,
    R392,
    R518,
}

impl Resolution {
    pub fn from_popup(value: i32) -> Self {
        match value {
            2 => Self::R392,
            3 => Self::R518,
            _ => Self::R266,
        }
    }

    pub fn size(self) -> i32 {
        match self {
            Self::R266 => 266,
            Self::R392 => 392,
            Self::R518 => 518,
        }
    }
}

pub struct ModelCatalog {
    pub bundled_models: Vec<BundledModel>,
}

impl ModelCatalog {
    pub fn scan() -> Self {
        Self {
            bundled_models: scan_model_roots(&default_scan_roots()),
        }
    }
}

pub fn resolve_model_selection(
    model_popup: i32,
    custom_path: &str,
    bundled_models: &[BundledModel],
) -> (
    Option<std::path::PathBuf>,
    Option<ModelManifest>,
    Option<String>,
) {
    let custom = custom_path.trim();
    if !custom.is_empty() {
        let path = std::path::PathBuf::from(custom);
        return match load_manifest(&path) {
            Ok(manifest) => (Some(path), Some(manifest), None),
            Err(err) => (None, None, Some(err.to_string())),
        };
    }

    if bundled_models.is_empty() {
        return (
            None,
            None,
            Some(
                "no models found; add packs under MediaCore/DepthONNX/models or use Browse Model Folder"
                    .into(),
            ),
        );
    }

    let index = (model_popup - 1) as usize;
    if index >= bundled_models.len() {
        return (None, None, Some("invalid model selection".into()));
    }

    let selected = &bundled_models[index];
    (
        Some(selected.directory.clone()),
        Some(selected.manifest.clone()),
        None,
    )
}

pub struct SmoothingCache {
    inner: parking_lot::Mutex<std::collections::HashMap<u64, CacheSlot>>,
}

struct CacheSlot {
    width: i32,
    depth: Vec<f32>,
}

impl SmoothingCache {
    pub fn new() -> Self {
        Self {
            inner: parking_lot::Mutex::new(std::collections::HashMap::new()),
        }
    }

    pub fn blend(&self, key: u64, size: i32, depth: &mut [f32], alpha: f32) {
        let mut guard = self.inner.lock();
        let alpha = alpha.clamp(0.0, 0.99);
        if let Some(slot) = guard.get(&key) {
            if alpha > 0.0 && slot.width == size && slot.depth.len() == depth.len() {
                for (out, prev) in depth.iter_mut().zip(slot.depth.iter()) {
                    *out = alpha * prev + (1.0 - alpha) * *out;
                }
            }
        }
        guard.insert(
            key,
            CacheSlot {
                width: size,
                depth: depth.to_vec(),
            },
        );
    }
}
