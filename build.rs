use pipl::*;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Find the newest CUDA toolkit installed on this machine.
/// Returns the path to `nvcc.exe` (Windows) or `nvcc` (other) of the
/// highest-version toolkit found. Falls back to bare "nvcc" so PATH is used.
fn find_nvcc() -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        let root = Path::new(r"C:\Program Files\NVIDIA GPU Computing Toolkit\CUDA");
        if let Ok(entries) = std::fs::read_dir(root) {
            let mut versions: Vec<(u32, u32, PathBuf)> = Vec::new();
            for e in entries.flatten() {
                let name = e.file_name();
                let name = name.to_string_lossy();
                // Expect names like "v13.2", "v12.6", "v11.8"
                if let Some(rest) = name.strip_prefix('v') {
                    let mut parts = rest.splitn(2, '.');
                    if let (Some(maj), Some(min)) = (parts.next(), parts.next()) {
                        if let (Ok(maj), Ok(min)) = (maj.parse::<u32>(), min.parse::<u32>()) {
                            let nvcc = e.path().join("bin").join("nvcc.exe");
                            if nvcc.is_file() {
                                versions.push((maj, min, nvcc));
                            }
                        }
                    }
                }
            }
            versions.sort_by_key(|(maj, min, _)| (*maj, *min));
            if let Some((_, _, path)) = versions.pop() {
                println!("cargo:warning=using nvcc at {}", path.display());
                return path;
            }
        }
    }
    PathBuf::from("nvcc")
}

fn compile_cuda_ptx() {
    let cu_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("kernels/refract.cu");
    println!("cargo:rerun-if-changed={}", cu_path.display());

    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR not set");
    let ptx_path = PathBuf::from(&out_dir).join("refract.ptx");

    let target = std::env::var("TARGET").unwrap_or_default();
    let nvcc = find_nvcc();

    // Target compute_75 (Turing) as the virtual ISA baseline. PTX is
    // forward-compatible: the CUDA driver JIT-compiles it to whatever real
    // SM the user's card actually is (Ampere, Ada, Blackwell, ...), so one
    // PTX covers everything from RTX 20xx / GTX 16xx onwards.
    // (CUDA 13.x dropped compute_60/_70 support, so Pascal is no longer
    // reachable from a 13.x toolchain anyway.)
    let mut cmd = Command::new(&nvcc);
    cmd.args([
        "--ptx",
        "-O3",
        "-arch=compute_75",
        // Safety net for older CUDA + newer MSVC combos. Harmless on newer
        // toolkits that already accept the host compiler.
        "-allow-unsupported-compiler",
        "-o",
    ])
    .arg(&ptx_path)
    .arg(&cu_path);

    // On Windows, nvcc shells out to the MSVC host compiler (cl.exe) even
    // when producing PTX, because it runs the C++ front-end for preprocessing.
    // Locate MSVC via the cc crate's registry helper and expose its bin
    // directory via PATH so nvcc can find cl.exe.
    if target.contains("windows") {
        if let Some(cl) = cc::windows_registry::find_tool(&target, "cl.exe") {
            let cl_path = cl.path();
            if let Some(bin_dir) = cl_path.parent() {
                let old_path = std::env::var("PATH").unwrap_or_default();
                let new_path = format!("{};{}", bin_dir.display(), old_path);
                cmd.env("PATH", new_path);
            }
        }
    }

    let status = cmd
        .status()
        .expect("failed to invoke nvcc — is the CUDA toolkit installed and on PATH?");

    if !status.success() {
        panic!("nvcc failed to compile {}", cu_path.display());
    }
}

fn main() {
    compile_cuda_ptx();

    pipl::plugin_build(vec![
        Property::Kind(PIPLType::AEEffect),
        Property::Name("RefractionDispersion"),
        Property::Category("Distort"),
        #[cfg(target_os = "windows")]
        Property::CodeWin64X86("EffectMain"),
        #[cfg(target_os = "macos")]
        Property::CodeMacIntel64("EffectMain"),
        #[cfg(target_os = "macos")]
        Property::CodeMacARM64("EffectMain"),
        Property::AE_PiPL_Version { major: 2, minor: 0 },
        Property::AE_Effect_Spec_Version {
            major: 13,
            minor: 28,
        },
        Property::AE_Effect_Version {
            version: 1,
            subversion: 0,
            bugversion: 0,
            stage: Stage::Develop,
            build: 1,
        },
        Property::AE_Effect_Info_Flags(0),
        Property::AE_Effect_Global_OutFlags(
            OutFlags::DeepColorAware,
        ),
        Property::AE_Effect_Global_OutFlags_2(
            OutFlags2::FloatColorAware
                | OutFlags2::SupportsSmartRender
                | OutFlags2::SupportsThreadedRendering,
        ),
        Property::AE_Effect_Match_Name("RefractionDispersion"),
        Property::AE_Reserved_Info(13),
        Property::AE_Effect_Support_URL("https://github.com"),
    ]);
}
