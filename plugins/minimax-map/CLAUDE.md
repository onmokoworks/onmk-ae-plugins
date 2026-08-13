# MinimaxMap Agent Notes

## Project

Rust Adobe After Effects effect plug-in. Display name and match name are both `MinimaxMap`; category is `Channel`.

The main control is signed `Amount`: negative values run Minimum, positive values run Maximum, and `0.00` is a pass-through. Fractional radii are blended between adjacent integer-radius morphology results for smooth small values such as `0.01`.

The implementation is CPU based. `SmartRenderGpu` currently falls back to CPU, so do not advertise a native GPU render path until one exists.

## Layout

- `README.md` / `README.ja.md`: public repo documentation
- `scripts/`: release build and install wrappers
- `rust/src/lib.rs`: After Effects registration, params, render dispatch, Smart Render glue
- `rust/src/minimax.rs`: CPU signed linear minimax morphology implementation
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
- Keep the current match name unless intentionally making a breaking compatibility change.
