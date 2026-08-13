# TuiImage

[English](./README.md) | [日本語](./README.ja.md)

![TuiImage demo](./docs/tuiimage-demo.gif)

TuiImage is an After Effects effect for creating ANSI art / TUI-style cell renderings from image layers.

It samples the input image into a grid, maps brightness to procedural glyph density, and reconstructs the result with Mono, Source, or Gradient color modes. It does not emit ANSI escape sequences; it recreates that visual style inside After Effects.

> Specifications, UI, parameter names, and defaults may change in future versions.

## Name

- Display name: `TuiImage`
- After Effects match name: `TuiImage`
- Plugin file name:
  - Windows: `TuiImage.aex`
  - macOS: `TuiImage.plugin`

## Main Features

- Cell-based sampling of the input layer
- Procedural ANSI art / TUI-style, ASCII-style, Block-style, and Braille-style glyph rendering
- ASCII Classic / Block / Braille / TUI Gradient presets
- Adjustable cell size, glyph scale, column gap, and row gap
- Mono / Source / Gradient color modes
- Experimental GPU rendering path enabled only through an environment variable

## Validation Status

- Tested on Adobe After Effects 2025 / Windows
- Tested on Adobe After Effects 2026 / Apple Silicon Mac
- Release binaries are built per platform from the same commit / tag

## Build

On Windows, run this from the repository root:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\build_release.ps1
```

Output:

```text
rust\target\release\TuiImage.aex
```

On macOS, run this on an Apple Silicon Mac:

```bash
bash ./scripts/build_macos_release.sh
```

Output:

```text
rust/target/release/TuiImage.plugin
```

## Installation

### Using a release build

On Windows, download `TuiImage.aex` from GitHub Releases, close After Effects, then copy it to the plug-ins folder:

```text
C:\Program Files\Adobe\Common\Plug-ins\7.0\MediaCore\
```

On macOS, download `TuiImage-macos-arm64.plugin.zip` from GitHub Releases, extract it, close After Effects, then copy `TuiImage.plugin` to the MediaCore folder:

```text
/Library/Application Support/Adobe/Common/Plug-ins/7.0/MediaCore/
```

Command-line copy example:

```bash
unzip TuiImage-macos-arm64.plugin.zip
sudo cp -R TuiImage.plugin "/Library/Application Support/Adobe/Common/Plug-ins/7.0/MediaCore/"
```

If a downloaded `.plugin` is blocked by macOS Gatekeeper, remove the quarantine attribute if needed:

```bash
sudo xattr -dr com.apple.quarantine "/Library/Application Support/Adobe/Common/Plug-ins/7.0/MediaCore/TuiImage.plugin"
```

Restart After Effects and apply the effect from `Stylize > TuiImage`.

If running PowerShell scripts is confusing, manual copy is fine.

### Installing a local build

Close After Effects, then run PowerShell as administrator:

```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\install_tuiimage_admin.ps1
```

By default, it copies the plugin to:

```text
C:\Program Files\Adobe\Common\Plug-ins\7.0\MediaCore\
```

For a local macOS build, manually copy the generated `rust/target/release/TuiImage.plugin` bundle into the MediaCore folder.

Generated `.aex` / `.plugin` files are not committed to Git. Distribution builds are attached as GitHub Release artifacts.

The macOS release artifact is for Apple Silicon / arm64. It is currently ad-hoc signed; Developer ID signing and notarization are not provided yet.

## Parameters

- `Preset`: chooses the glyph style and initial values
- `Cell Width` / `Cell Height`: controls grid density
- `Uniform Cell Size`: links Cell Width and Cell Height
- `Scale`: controls glyph size inside each cell
- `Column Gap %` / `Row Gap %`: adjusts spacing corresponding to character and line spacing
- `Color Mode`: selects Mono / Source / Gradient
- `Source Mix`: mixes the generated result back toward the source layer

## Environment Variables

The GPU path is experimental and disabled by default. Set this only when testing it.

```powershell
$env:TUIIMAGE_ENABLE_GPU = "1"
```

macOS / bash:

```bash
export TUIIMAGE_ENABLE_GPU=1
```

## Limitations

- The current renderer does not draw real fonts
- ASCII / Block / Braille styles are procedural glyph-like renderings
- Arbitrary font selection is not implemented
- GPU rendering is experimental

## License

MIT License. See [LICENSE](LICENSE).
