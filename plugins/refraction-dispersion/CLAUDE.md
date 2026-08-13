# RefractionDispersion Agent Notes

## Project

Rust Adobe After Effects effect plug-in. Display name and match name are both `RefractionDispersion`; category is `Distort`.

The CPU/Rayon implementation in `rust/src/refract.rs` is the reference render path. The CUDA backend in `rust/src/gpu.rs` and `rust/kernels/refract.cu` is currently passthrough-only plumbing, so do not treat it as feature-complete.

## Layout

- `README.md` / `README.ja.md`: public repo documentation
- `scripts/`: release build and install wrappers
- `docs/mac-gpu-roadmap.md`: macOS and GPU roadmap
- `rust/src/lib.rs`: After Effects effect registration, params, render dispatch, Smart Render glue
- `rust/src/refract.rs`: CPU refraction/dispersion implementation
- `rust/src/gpu.rs`: experimental CUDA setup and passthrough launch
- `rust/kernels/refract.cu`: experimental CUDA passthrough kernel
- `rust/build.rs`: PiPL generation and CUDA PTX compilation

## Build

Run release wrappers from the repository root:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\build_release.ps1
```

Run Rust checks from `rust/`:

```powershell
cd rust
cargo fmt
cargo check
cargo test
cargo build --release
```

On Windows, `rust/build.rs` invokes `nvcc`; CUDA Toolkit must be installed or available on `PATH`. Non-Windows targets skip CUDA PTX and use the CPU renderer.

## Publish Notes

- Do not commit `target/`, `rust/target/`, `.aex`, `.plugin`, `.dll`, `.pdb`, or local scratch files.
- Keep `Use GPU (CUDA)` disabled by default until the CUDA backend renders the real effect.
- Add the intended `LICENSE` before publishing to GitHub.
