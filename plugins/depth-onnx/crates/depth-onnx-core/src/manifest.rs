use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preprocess {
    ImageNetRgb,
}

impl Preprocess {
    pub fn from_str(value: &str) -> Self {
        match value {
            "imagenet_rgb" => Self::ImageNetRgb,
            _ => Self::ImageNetRgb,
        }
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct ModelManifest {
    pub id: String,
    #[serde(default)]
    pub label: String,
    #[serde(default = "default_preprocess")]
    pub preprocess: String,
    #[serde(default = "default_input_name")]
    pub input_name: String,
    #[serde(default = "default_output_name")]
    pub output_name: String,
    pub variants: BTreeMap<String, String>,
}

fn default_preprocess() -> String {
    "imagenet_rgb".into()
}
fn default_input_name() -> String {
    "pixel_values".into()
}
fn default_output_name() -> String {
    "predicted_depth".into()
}

impl ModelManifest {
    pub fn preprocess_kind(&self) -> Preprocess {
        Preprocess::from_str(&self.preprocess)
    }

    pub fn variant_file(&self, size: i32) -> Option<&str> {
        self.variants.get(&size.to_string()).map(String::as_str)
    }
}

#[derive(Debug, Error)]
pub enum ManifestError {
    #[error("manifest not found: {0}")]
    NotFound(PathBuf),
    #[error("manifest is empty: {0}")]
    Empty(PathBuf),
    #[error("failed to parse manifest: {0}")]
    Parse(PathBuf, #[source] serde_json::Error),
    #[error("manifest missing \"id\": {0}")]
    MissingId(PathBuf),
    #[error("manifest missing or invalid \"variants\": {0}")]
    MissingVariants(PathBuf),
}

pub fn load_manifest(directory: impl AsRef<Path>) -> Result<ModelManifest, ManifestError> {
    let directory = directory.as_ref();
    let path = directory.join("manifest.json");
    let raw = std::fs::read_to_string(&path).map_err(|_| ManifestError::NotFound(path.clone()))?;
    if raw.trim().is_empty() {
        return Err(ManifestError::Empty(path));
    }

    let mut manifest: ModelManifest =
        serde_json::from_str(&raw).map_err(|e| ManifestError::Parse(path.clone(), e))?;

    if manifest.id.is_empty() {
        return Err(ManifestError::MissingId(path));
    }
    if manifest.label.is_empty() {
        manifest.label = manifest.id.clone();
    }
    if manifest.variants.is_empty() {
        return Err(ManifestError::MissingVariants(path));
    }

    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_reference_manifest() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../model/depth_anything_v2_small/manifest.json");
        if !root.exists() {
            return;
        }
        let dir = root.parent().unwrap();
        let manifest = load_manifest(dir).expect("manifest");
        assert_eq!(manifest.id, "depth_anything_v2_small");
        assert_eq!(manifest.label, "Depth Anything V2 Small");
        assert_eq!(manifest.variant_file(518).unwrap(), "depth_anything_v2_small.onnx");
    }
}
