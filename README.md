# CelGlow

After Effects SmartFX plug-in for illustration and anime-style compositing.
The current implementation is Rust-only and generates flat stepped hard-edge
glow bands from a luma or color mask. It uses a GPU compute path for source-mask
generation, approximate distance-field generation via JFA, and per-pixel
band/copy compositing when a GPU adapter is available.

## Current Implementation

- Source: `rust/celglow`
- Crate: `cdylib`
- AE SDK wrapper: `virtualritz/after-effects`
- PiPL: generated from `rust/celglow/build.rs`
- Effect menu: `Effects > onmk > CelGlow`
- Match name: `ANTH CelGlow`
- Render path: SmartPreRender / SmartRender
- Output artifact on Windows: `rust/celglow/target/release/celglow.dll`

For After Effects on Windows, install the release DLL as `CelGlow.aex`.

## Build

```powershell
cd D:\Projects\01_Project\04_Tools\Ae_Plugins\CelGlow\rust\celglow
cargo build --release
```

## Install for Local AE Testing on Windows

Run PowerShell as Administrator if your After Effects plug-in directory requires
elevated permissions.

```powershell
cd D:\Projects\01_Project\04_Tools\Ae_Plugins\CelGlow
.\scripts\install_dev.ps1
```

By default, the script copies:

```text
rust\celglow\target\release\celglow.dll
```

to:

```text
C:\Program Files\Adobe\Common\Plug-ins\7.0\MediaCore\CelGlow.aex
```

You can override the install directory:

```powershell
.\scripts\install_dev.ps1 -InstallDir "D:\AePlugins"
```

## C++ Version

The old C++ plug-in tree has been removed. Keep only one CelGlow plug-in visible
to After Effects at a time, because duplicate plug-ins with the same match name
can cause AE to load only one of them.

## Notes

- The Rust version follows the same `PathArrayRust` pattern:
  `SmartPreRender`, `SmartRender`, `checkout_layer(0, 0)`, and pre-render data.
- The current feature set is generated 1-8 band glow, color source mode, basic
  copies, blend controls, GPU compute, and debug views.
- Band opacity is flat across each band for a storybook/cel-like look; only the
  overall glow start/end edges are softened, so adjacent bands do not create
  transparent gaps.
- `Luma Softness` affects the source-mask boundary used by the distance field.
- `Source Box Blur` softens the source mask before distance-field generation for
  a gentler storybook-style glow.
- `Band Count`, `Glow Start`, `Total Size`, and `Size Mode` generate adjacent
  band ranges automatically, so outer boundaries follow the previous band
  without exposing per-band radius controls.
- GPU compute falls back to CPU if initialization or execution fails.
- Planned work such as matte sources, presets, custom UI, and exact GPU EDT
  remains tracked in `docs/`.
