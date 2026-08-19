# onmk Film pipeline

Host-independent processing order (fixed):

1. Input colorspace decode (Rec.709 / sRGB / Linear / Bypass / S-Log3 / LogC3 / V-Log / C-Log3)
2. Exposure (EV stops)
3. Negative RGB response (per-stock gamma, toe, cast, optional mono)
4. Emulsion bleed (irradiation, dye cloud, interlayer fringe)
5. Halation (threshold extract → blur → tinted add)
6. Print contrast / shoulder / saturation / signature / skin
7. Optional crossover (age split-tone)
8. MTF softness + acutance
9. Density-space lattice grain (+ optional print grain, mottle)
10. Bloom / diffusion
11. Optics (chromatic aberration, vignette, light leak, gate weave sample)
12. Display-referred encode; alpha preserved

## Backends

- `onmk-film-core` — CPU reference implementation and unit tests
- `onmk-film-gpu` — wgpu compute with Metal backend for separable blur and grain
- Host falls back to CPU when GPU init/run fails

## Identity

| Field | Value |
|-------|--------|
| Effect name | onmk Film |
| Category | onmk |
| Match name | ONMK_Film |
| Bundle | onmkFilm.plugin |

## Presets

JSON files under `presets/` are documentation / offline defaults. Panel Look popup writes values into controls via `FilmParams::apply_look`.
