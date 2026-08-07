# Render / Export Parity Matrix

Status date: 2026-08-07

This matrix is the release gate for blend modes that affect both WebGL preview and Canvas2D export. A mode may be exposed in the product UI only when it is listed in `BLEND_MODE_OPTIONS` and has matching preview/export behavior documented here.

## Blend Modes

| Mode | UI exposed | Engine `BlendMode` | WebGL preview | Canvas2D export | Status |
| --- | --- | --- | --- | --- | --- |
| Normal | Yes | `normal` | `u_blendMode = 0` | `source-over` | Verified MVP |
| Multiply | Yes | `multiply` | `u_blendMode = 1` | `multiply` | Verified MVP |
| Screen | Yes | `screen` | `u_blendMode = 2` | `screen` | Verified MVP |
| Overlay | Yes | `overlay` | `u_blendMode = 3` | `overlay` | Verified MVP |
| Darken | Yes | `darken` | `u_blendMode = 4` | `darken` | Verified 0.2.0 |
| Lighten | Yes | `lighten` | `u_blendMode = 5` | `lighten` | Verified 0.2.0 |
| Color Dodge | Yes | `color-dodge` | `u_blendMode = 6` | `color-dodge` | Verified 0.2.0 |
| Color Burn | Yes | `color-burn` | `u_blendMode = 7` | `color-burn` | Verified 0.2.0 |
| Hard Light | Yes | `hard-light` | `u_blendMode = 8` | `hard-light` | Verified 0.2.0 |
| Soft Light | Yes | `soft-light` | `u_blendMode = 9` | `soft-light` | Verified 0.2.0 |
| Difference | Yes | `difference` | `u_blendMode = 10` | `difference` | Verified 0.2.0 |
| Exclusion | Yes | `exclusion` | `u_blendMode = 11` | `exclusion` | Verified 0.2.0 |

**Verification basis (2026-08-07):** all 8 new modes were already implemented in the WebGL fragment shader (`blendColors()` cases 4-11, `apps/desktop/src/renderer/shaders.ts`) and are natively supported by Canvas2D `globalCompositeOperation` under the same names (W3C compositing spec). Parity proof = `BLEND_MODE_SHADER_IDS` compile-time `Record<BlendMode, number>` (union member without shader id fails the build) + `blendModes.test.ts` mapping every registry option to its explicit Canvas2D operation and shader id. Rust flatten path (`crates/core` `flatten_document`/`sample_pixel`) is Porter-Duff "over" only and NOT frontend-reachable — not part of this gate (see `docs/decisions/decision-log.md` 2026-08-07).

## Gate

- The product UI renders blend modes from `apps/desktop/src/engine/blendModes.ts`.
- `LayersPanel` must not cast arbitrary select values to `BlendMode`.
- Export compositing must use the registry's Canvas2D operation mapping.
- Adding a new blend mode requires updating this matrix, the `BlendMode` type, the registry, WebGL shader mapping, export mapping, and preview/export parity tests.
