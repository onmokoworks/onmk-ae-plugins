# Visual Direction

## Reference Aesthetic

The target is not simply "ASCII art." The target is the recent class of stylish
terminal visuals used in modern TUI dashboards, splash screens, and terminal
image renderers.

## Trend Summary

The current visual trend tends to use:

- Unicode block elements
- half-blocks
- Braille cells
- 24-bit color / truecolor-style gradients
- silhouette-preserving cell mapping
- moderate edge enhancement

Pure ASCII ramps still have value, but mostly as a flavor preset rather than
the strongest default look.

This aligns with modern terminal tooling:

- Google's GIF for CLI converted animated media into ASCII frames and used ANSI
  escape sequences for animation and color.
- Charm's Lip Gloss treats terminal visuals like reusable style/layout objects
  and explicitly supports ANSI 16, ANSI 256, and 24-bit truecolor profiles.
- Chafa's image renderer uses ANSI/Unicode character art, including block and
  Braille-style approaches, and also supports higher-fidelity terminal graphics
  protocols where available.

## Internal Design Principle

Think of the output as:

`terminal cell graphics`

not:

`letters replacing pixels`

That difference should guide both parameter names and implementation choices.

## Good First-Look Criteria

A strong output should:

- read clearly from a distance
- hold recognizable shapes at low cell counts
- look intentional even when paused on a single frame
- feel like terminal art, not just posterization

## Weak-Look Failure Modes

Avoid these:

- too much reliance on classic ASCII ramps
- muddy gradients
- over-dithered noise
- tiny text-like glyphs that collapse at preview size
- excessive palette complexity before the basics work

## Source Notes

- Google Open Source Blog, "Tenor GIF for CLI": <https://opensource.googleblog.com/2018/06/tenor-gif-for-cli.html>
- Charmbracelet Lip Gloss README: <https://github.com/charmbracelet/lipgloss>
- Chafa project README: <https://github.com/hpjansson/chafa>
