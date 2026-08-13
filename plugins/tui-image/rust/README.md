# Rust Effect

This folder contains the Rust-native After Effects implementation for `TuiImage`.

The current build target is a Windows `.aex` effect:

- Visible name: `TuiImage`
- Match name: `TuiImage`
- Category: `Stylize`
- Output file: `TuiImage.aex`
- Crate output DLL: `tui_image_renderer.dll`

## Build

From the repository root, prefer:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\build_release.ps1
```

Or from this folder:

```powershell
cargo build --release
Copy-Item -LiteralPath .\target\release\tui_image_renderer.dll -Destination .\target\release\TuiImage.aex -Force
```

## Install

Run from the repository root in an elevated PowerShell:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\install_tuiimage_admin.ps1
```

## Render Order

1. Get source frame from After Effects.
2. Flatten to contiguous RGBA.
3. Run GPU renderer only when explicitly enabled.
4. Fall back to CPU renderer by default.
5. Copy output back to After Effects.

## GPU Flag

GPU rendering is experimental. Enable it only for testing:

```powershell
$env:TUIIMAGE_ENABLE_GPU = "1"
```
