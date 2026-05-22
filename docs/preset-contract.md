# Preset Contract

Preset files live in `presets/*.json`. They are data, not code. The renderer
must treat unknown fields as ignorable so presets can evolve without breaking
old builds.

## Shape

```json
{
  "id": "block",
  "label": "Block",
  "version": 1,
  "glyphMode": "block",
  "glyphs": [" ", "░", "▒", "▓", "█"],
  "cell": { "width": 8, "height": 12 },
  "sampling": {
    "luma": "rec709",
    "contrast": 1.1,
    "gamma": 1.0,
    "edgeBoost": 0.15,
    "invert": false
  },
  "color": {
    "mode": "source",
    "foreground": "#ffffff",
    "background": "#000000"
  }
}
```

## Field Notes

- `glyphMode` chooses the renderer path. Braille needs a different mapper from
  simple ramp glyphs.
- `glyphs` is ordered from low density to high density.
- `cell.width` / `cell.height` are output-cell dimensions, not source pixels.
- `sampling.edgeBoost` is deliberately small by default; heavy edge passes make
  output noisy.
- `color.mode` should start with `mono`, `source`, and `gradient`.
- The current renderer is procedural and does not load arbitrary fonts from the
  preset file.

## Compatibility Rule

Presets should be portable across:

- preview CLI
- CEP panel
- future native/helper implementations

Do not put AE-specific state in preset files.
