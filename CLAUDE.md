# MedianPro Agent Notes

## Project

Rust Adobe After Effects effect plug-in. The public project name and After Effects display name are `MedianPro`; the match name is `ONMK_MedianPro`.

The implementation is CPU based. `SmartRenderGpu` currently falls back to CPU, so do not advertise a native GPU render path until one exists.

## Layout

- `README.md` / `README.ja.md`: public repo documentation
- `scripts/`: release build and install wrappers
- `rust/src/lib.rs`: After Effects registration, params, render dispatch, Smart Render glue
- `rust/src/filters.rs`: CPU filter implementations
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
- MedianPro should stay focused on Median and Weighted Median. Keep Kuwahara / Bilateral style filters in the separate working-name plug-in.
