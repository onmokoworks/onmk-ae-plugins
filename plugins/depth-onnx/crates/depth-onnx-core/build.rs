fn main() {
    let manifest_dir = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());

    let ort_dylib = if cfg!(target_os = "windows") {
        let mut candidates: Vec<_> = std::fs::read_dir(manifest_dir.join("../../third_party"))
            .ok()
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("onnxruntime-win-x64-gpu-")
            })
            .map(|e| e.path().join("lib/onnxruntime.dll"))
            .filter(|p| p.is_file())
            .collect();
        candidates.sort();
        candidates.into_iter().next()
    } else {
        let mac = manifest_dir
            .join("../../third_party/onnxruntime-osx-arm64/lib/libonnxruntime.1.24.4.dylib");
        mac.is_file().then_some(mac)
    };

    if let Some(path) = ort_dylib {
        println!("cargo:rustc-env=ORT_DYLIB_PATH={}", path.display());
    }

    let model_root = manifest_dir.join("../../model");
    if model_root.is_dir() {
        println!(
            "cargo:rustc-env=AE_DEPTH_ONNX_DEV_MODEL_DIR={}",
            model_root.display()
        );
    }
}
