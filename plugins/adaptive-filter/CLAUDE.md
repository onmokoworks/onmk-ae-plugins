# AdaptiveFilter Agent Notes

## Project

Rust Adobe After Effects effect plug-in. `AdaptiveFilter` is a working name for the non-median filter set split out from MedianPro. The current display name is `AdaptiveFilter`; the match name is `ONMK_AdaptiveFilter`.

The implementation is CPU based. `SmartRenderGpu` currently falls back to CPU, so do not advertise a native GPU render path until one exists.

## Layout

- `README.md` / `README.ja.md`: public repo documentation
- `scripts/`: release build and install wrappers
- `rust/src/lib.rs`: After Effects registration, params, render dispatch, Smart Render glue
- `rust/src/filters.rs`: CPU filter implementations
- `rust/src/kernel.rs`: OpenCL kernel source retained as reference/experimental material
- `rust/build.rs`: PiPL generation

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

## Publish Notes

- Do not commit `target/`, `rust/target/`, `.aex`, `.plugin`, `.dll`, `.pdb`, or local scratch files.
- Keep Median and Weighted Median in the MedianPro repository. This project should stay focused on Kuwahara / Generalized Kuwahara / Bilateral-style filters while the final name is being decided.
