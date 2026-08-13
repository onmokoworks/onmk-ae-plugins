use crate::manifest::{load_manifest, ModelManifest};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct BundledModel {
    pub directory: PathBuf,
    pub manifest: ModelManifest,
}

pub fn default_scan_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();

    if let Ok(value) = std::env::var("AE_DEPTH_ONNX_MODEL_DIR") {
        if !value.is_empty() {
            append_root(&mut roots, PathBuf::from(value));
        }
    } else if let Ok(value) = std::env::var("AE_DEPTHANYTHING_MODEL_DIR") {
        if !value.is_empty() {
            append_root(&mut roots, PathBuf::from(value));
        }
    }

    if let Some(root) = user_models_root() {
        append_root(&mut roots, root);
    }

    if let Some(root) = option_env!("AE_DEPTH_ONNX_DEV_MODEL_DIR").map(PathBuf::from) {
        append_root(&mut roots, root);
    }

    roots
}

fn append_root(roots: &mut Vec<PathBuf>, root: PathBuf) {
    if root.as_os_str().is_empty() {
        return;
    }
    let canonical = root.canonicalize().unwrap_or(root);
    if !roots.iter().any(|existing| existing == &canonical) {
        roots.push(canonical);
    }
}

pub fn scan_model_roots(roots: &[PathBuf]) -> Vec<BundledModel> {
    let mut models = Vec::new();
    let mut seen = BTreeSet::new();

    for root in roots {
        scan_one_root(root, &mut models, &mut seen);
    }

    models.sort_by(|a, b| a.manifest.label.cmp(&b.manifest.label));
    models
}

fn scan_one_root(root: &Path, out: &mut Vec<BundledModel>, seen: &mut BTreeSet<PathBuf>) {
    if !root.is_dir() {
        return;
    }

    let direct_manifest = root.join("manifest.json");
    if direct_manifest.is_file() {
        push_if_valid(root, out, seen);
        return;
    }

    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() && path.join("manifest.json").is_file() {
            push_if_valid(&path, out, seen);
        }
    }
}

fn push_if_valid(dir: &Path, out: &mut Vec<BundledModel>, seen: &mut BTreeSet<PathBuf>) {
    let canonical = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
    if !seen.insert(canonical.clone()) {
        return;
    }
    if let Ok(manifest) = load_manifest(&canonical) {
        out.push(BundledModel {
            directory: canonical,
            manifest,
        });
    }
}

pub fn build_model_popup_names(models: &[BundledModel]) -> String {
    if models.is_empty() {
        return "(no models found)".into();
    }
    models
        .iter()
        .map(|m| m.manifest.label.as_str())
        .collect::<Vec<_>>()
        .join("|")
}

/// User-provided model packs live next to the plug-in install (not inside the signed bundle).
///
/// macOS: `.../MediaCore/DepthONNX/models/`
/// Windows: `.../Plug-ins/Effects/DepthONNX/models/`
fn user_models_root() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let path = std::env::current_exe().ok()?;
        let path = path.to_string_lossy();
        let cut = path.rfind("/Contents/MacOS/")?;
        let bundle = Path::new(&path[..cut]);
        let media_core = bundle.parent()?;
        Some(media_core.join("DepthONNX").join("models"))
    }
    #[cfg(target_os = "windows")]
    {
        crate::ort_loader::plugin_install_dir().map(|dir| dir.join("models"))
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn scans_nested_model_packs() {
        let dir = tempdir().unwrap();
        let pack = dir.path().join("demo_pack");
        fs::create_dir_all(&pack).unwrap();
        fs::write(
            pack.join("manifest.json"),
            r#"{
              "id": "demo",
              "label": "Demo Pack",
              "variants": { "266": "demo_266.onnx" }
            }"#,
        )
        .unwrap();

        let models = scan_model_roots(&[dir.path().to_path_buf()]);
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].manifest.id, "demo");
    }
}
