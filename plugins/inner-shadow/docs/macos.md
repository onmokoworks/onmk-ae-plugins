# macOS build (0.2.0)

wgpu 25 enables its Metal backend by default. This plugin uses the same WGSL compute shaders on Windows and macOS, inside ordinary AE Render/SmartRender. It does not require CUDA. CPU mode does not initialise a GPU; Auto falls back to CPU on GPU failure.

## Build on a Mac

Install Rust through rustup and Xcode Command Line Tools (`xcode-select --install`), then open a terminal in `plugins/inner-shadow`:

```bash
bash build_macos_release.sh
```

The default target is the native Rust host architecture. The script builds with Cargo.lock, packages the PiPL resources, dylib and Info.plist, then ad-hoc signs and verifies the bundle. It does not install the plugin.

Apple Silicon explicitly:

```bash
rustup target add aarch64-apple-darwin
bash build_macos_release.sh aarch64-apple-darwin
```

Intel explicitly (requires an Xcode SDK/linker supporting that target):

```bash
rustup target add x86_64-apple-darwin
bash build_macos_release.sh x86_64-apple-darwin
```

Outputs:

- `dist/aarch64-apple-darwin/InnerShadow.plugin`
- `dist/x86_64-apple-darwin/InnerShadow.plugin`

These are separate architecture bundles, not universal binaries. Match the architecture of the running AE process (native Apple Silicon AE vs Intel/Rosetta). The script defaults to a macOS 12.0 deployment target; the AE version may require a newer macOS version.

To verify the hardware GPU implementation on the Mac:

```bash
INNER_SHADOW_REQUIRE_GPU=1 cargo test --locked -- --nocapture
cargo run --locked --release --example render_fixture -- verification/macos
```

The log should report a hardware adapter with `Metal` backend. Compare CPU/GPU raw results and inspect the generated PPM files. Copy the completed `.plugin` bundle into `/Library/Application Support/Adobe/Common/Plug-ins/7.0/MediaCore/` to install it, then restart AE after saving work.

## Validation status

The macOS packaging script was syntax-checked on Windows. PiPL output names and plist keys were checked against the pinned dependency source; wgpu's Metal feature is enabled. A macOS compiler, linker, Metal adapter and AE are not available in this task environment: **the Mac build and AE/Metal runtime are not yet verified**. Ad-hoc signing is for local use, not Developer ID distribution signing or notarization.
