# Renderer Helper Placeholder

This directory is reserved for the image conversion helper.

The first CLI should accept:

```text
tui-render --input frame.png --preset presets/block.json --output out.png
```

Later:

```text
tui-render --input frames/ --preset presets/tui-gradient.json --output out.apng
```

The CLI boundary should stay stable even if the implementation moves from
Node/Python to Rust.
