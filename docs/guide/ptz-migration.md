# .ptz Format Migration Guide

## Format Stability Policy

- **MVP scope lock:** format `v1` (below) is the locked MVP format. No format change ships between `0.1.0-beta.1` and the `0.1.0` stable release; the first extension (`v2`, additive, text/shape metadata) ships with `0.2.0` per `roadmap.md`.
- **Backward compatibility is guaranteed starting `v0.1.0`** (first stable release): every later format change must be additive and ship a migrator (see "How to Bump Format Version").
- **`v1.0.0` is the final format lock.** Between `0.1.0` and `1.0.0` the `version` field may still grow (`1`, `2`, ...) as features land (e.g. text/shape layer metadata), but files never become unreadable.
- Pre-release (`-alpha` / `-beta`) files remain loadable after `0.1.0` via the absent/`0`/`1` compat path below.

## Current Format (v1, alpha.1+)

```
.ptz (ZIP container)
├── document.json    (DocumentModel JSON, `version: 1`, layers contain `imageBitmap: null`)
├── <layer-id-1>.png (raw PNG data)
├── <layer-id-2>.png
└── ...
```

- **`document.json`**: Full `DocumentModel` serialized via `JSON.stringify`, with per-layer `imageBitmap` stubbed to `null`. Layer PNGs stored as separate entries in the ZIP, keyed by `layer.id`.
- **Layer PNGs**: Uncompressed raw PNG bytes (not base64). The Rust side reads them as `Vec<u8>` during streaming save.
- **Version field**: Alpha files may have no `version` field at all. Version `0` / absent / `1` are all treated as compatible.

## Loading Rules (`editorOpenImage.ts` loadProjectFile)

1. `JSON.parse(documentJson)` → `DocumentModel`
2. Read `version` field — if absent/`0`/`1`: compatible. If `>1`: show warning toast `"This project was saved by a newer Photrez version. It may not load correctly."`
3. For each `model.layers[i].id`, look up the PNG in the ZIP, decode from base64 → Blob → `createImageBitmap`. If missing: `imageBitmap = null`.
4. `engine.restore(model)` — reconstructs the document in memory.

## Migration Rules

### Adding a new field to DocumentModel

**Allowed.** Extra fields in `document.json` are silently ignored by `JSON.parse` + spread into the model. The engine handles missing optional fields via default values.

Example: adding `"guides": [...]` in v2 — old files without the field get `undefined`, engine treats it as "no guides".

### Changing the ZIP structure

**Breaking change.** Adding new ZIP entries (e.g., `metadata.json`) is safe as long as old code ignores unknown entries. Removing/renaming entries is breaking — bump version and add a migration step.

### Removing a layer field

**Breaking change.** If a field is required by `DocumentModel`, removing it crashes `engine.restore`. Never remove fields from the document model without a schema migration.

## How to Bump Format Version

1. Increment `version` in `projectSerialize.ts` (line 66).
2. In `editorOpenImage.ts` `loadProjectFile`, add a version map:
   ```ts
   const VERSION_MIGRATORS: Record<number, (model: DocumentModel) => DocumentModel> = {
     1: (m) => ({ ...m, newField: defaultValue }),
     2: (m) => { /* migrate from v2 to v3 */ },
   };
   ```
3. For each version gap, apply migrators sequentially.
4. Keep old version files loadable — after `v0.1.0` backward compat may **never** be dropped (additive + migrator only). Post-`v1.0.0`, dropping compatibility requires a MAJOR app version bump.

## Testing

- Save a project → verify `document.json` contains `"version": N`.
- Load an alpha file (no version field) → verify no crash, no warning.
- Load a v1 file → verify no warning.
- Load a future-v2 file (manually crafted) → verify warning toast appears.
- Add a new field to model → verify old files load with default value for the new field.
