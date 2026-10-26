# macOS build

## Build

Install Rust through rustup and the Xcode Command Line Tools (`xcode-select --install`). Then run, from `plugins/particle-kit`:

```bash
rustup target add aarch64-apple-darwin x86_64-apple-darwin
bash build_macos_release.sh
```

The script builds both architectures with Cargo.lock and combines them into Universal bundles: `dist/ParticleKit.plugin` and `dist/LatticeLab.plugin`. It also ad-hoc signs the bundle and verifies it. It does not install the plugin.

To build a single architecture, pass its target, for example `bash build_macos_release.sh aarch64-apple-darwin`.

To install, quit AE first. Then copy the `.plugin` bundles to `/Library/Application Support/Adobe/Common/Plug-ins/7.0/MediaCore/`.

## Differences from Windows

- Particle Kit stores presets, node graphs, the Node UI shell and `debug.log` in `~/Documents/Particle Kit/` (on Windows: `%USERPROFILE%\Documents\Particle Kit\`). Lattice Lab uses `~/Documents/Lattice Lab/` the same way.
- File dialogs use `osascript` (`choose file` / `choose file name`). A saved file gets `.json` appended when the extension is missing, as on Windows.
- "Open Preset Folder" and the Node UI shell open with Finder / the default browser (`open`).
- The `\\.\pipe\AEExternalDebug` debug pipe exists only on Windows. The file log is written on both platforms.

## Validation status

Measured on macOS (Apple Silicon, Rust 1.95):
- Both targets build for both effects.
- `cargo test --workspace` passes.
- `cargo check --target x86_64-pc-windows-msvc` passes.
- The dialog AppleScript compiles with `osacompile`.

Not yet verified in After Effects on macOS: rendering, the dialogs inside AE, and the Node UI shell. Ad-hoc signing is for local use, not Developer ID signing or notarization.
