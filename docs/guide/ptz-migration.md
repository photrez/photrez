# .ptz Format Migration Guide

## Format Stability Policy

- **MVP scope lock:** format `v1` was the locked MVP format. No format change shipped between `0.1.0-beta.1` and the `0.1.0` stable release. The additive extensions have since landed: `v2` (text/shape metadata) and `v3` (text layer metadata riding the layer spread). **The current version is `3`** — see "Current Format" below, which is what this build writes.
- **Backward compatibility is guaranteed starting `v0.1.0`** (first stable release): every later format change must be additive and ship a migrator (see "How to Bump Format Version").
- **`v1.0.0` is the final format lock.** Between `0.1.0` and `1.0.0` the `version` field may still grow (`1`, `2`, `3`, ...) as features land, but files never become unreadable. `editorOpenImage.ts` warns only above `3`.
- Pre-release (`-alpha` / `-beta`) files remain loadable after `0.1.0` via the absent/`0`/`1`/`2`/`3` compat path below.

## Current Format (v3)

```
.ptz (ZIP container)
├── document.json    (document JSON, `version: 3`, every layer has `imageBitmap: null`)
├── layers/<layer-id-1>.png (PNG data)
├── layers/<layer-id-2>.png
└── ...
```

- **`document.json`**: Written by the Rust core, not the TypeScript host. The save path (`save_project_streaming_begin` in `apps/desktop/src-tauri/src/save_stream.rs`) parses the document into `PtzDocument` (`crates/core/src/ptz_document.rs`) and that type serializes the bytes. Per-layer `imageBitmap` / `baseImageBitmap` are written as `null`; pixels live in the ZIP.
- **Layer PNGs**: Raw PNG bytes (not base64), written under `layers/<layer.id>.png` and DEFLATE-compressed by the ZIP container (`CompressionMethod::Deflated`) — the PNG data itself is not additionally compressed. The Rust side reads them as `Vec<u8>`.
- **Version field**: Files this build writes carry `version: 3`. Alpha files may have no `version` field at all; version `0` / absent / `1` / `2` / `3` are all treated as compatible.
- **Float precision**: floats are parsed correctly-rounded and echoed back unchanged. `crates/core/Cargo.toml` enables `serde_json`'s `float_roundtrip` feature for this — its default float parsing is approximate and can be 1 ULP off on some decimal literals. A file re-saved by this build may therefore differ from the previous writer's output in the last digit of some floats (and in nothing else). This is a precision *gain*: the value now matches what `str::parse::<f64>` and the loader's `JSON.parse` both produce.

## Loading Rules (`editorOpenImage.ts` loadProjectFile)

1. `JSON.parse(documentJson)` → the document model
2. Read `version` field — if absent/`0`/`1`/`2`/`3`: compatible. If `>3`: show warning toast `"This project was saved by a newer Photrez version. It may not load correctly."`
3. For each layer, look up `layers/<id>.png` in the ZIP, decode → Blob → `createImageBitmap`. If missing: `imageBitmap = null`.
4. `engine.restore(model)` — reconstructs the document in memory.

Rust owns the WRITE; TypeScript owns this READ. There is no Rust read path for
`document.json`, so nothing at runtime enforces that the two agree on the shape
— only the tests do (`crates/core/src/ptz_document.rs` round-trips the real
writer output, and `apps/desktop/src/components/editor/__tests__/ptzWriterGolden.test.ts`
diffs the TypeScript test model against those bytes).

## Migration Rules

### Adding a new field to DocumentModel

**Two halves, and only one of them is silent.**

*On load:* extra fields in `document.json` are ignored by `JSON.parse` + the spread into the model. Old files without the field get `undefined` and the engine's defaults apply. Example: adding `"guides": [...]` — old files get `undefined`, treated as "no guides".

*On save:* **not silent.** `PtzDocument` is a typed projection, so a document field with no corresponding field on that struct is DROPPED from every file written from then on. Adding a field to `DocumentModel` therefore requires adding it to `PtzDocument` (and to `CanonicalDocument` / `CanonicalLayer` in `crates/core/src/canonical_model.rs`) in the same change, or the data is lost on the next save. The round-trip tests in `ptz_document.rs` assert exact key sets precisely to catch this.

Fields that predate the Rust writer must carry a serde default on the Rust side: the loader never normalises nested layer params (`textData`, `shapeParams`, `basicAdjustment` are raw-spread by `restoreSnapshot`), so a file predating a nested field reaches the save path with it genuinely absent and would otherwise fail the save with `E_VALIDATION` and write nothing.

### Changing the ZIP structure

**Breaking change.** Adding new ZIP entries (e.g., `metadata.json`) is safe as long as old code ignores unknown entries. Removing/renaming entries is breaking — bump version and add a migration step.

### Removing a layer field

**Breaking change.** If a field is required by `DocumentModel`, removing it crashes `engine.restore`. Never remove fields from the document model without a schema migration.

## How to Bump Format Version

1. Increment `PTZ_VERSION` in `crates/core/src/ptz_document.rs`. That constant is the only place the written version lives — the host cannot set it, and `projectSerialize.ts` no longer builds the payload.
2. Teach the loader the new version in `editorOpenImage.ts` `loadProjectFile`. It warns on any `version` greater than the highest it knows, so raising `PTZ_VERSION` without this step toasts on every project open.
3. In `editorOpenImage.ts` `loadProjectFile`, add a version map:
   ```ts
   const VERSION_MIGRATORS: Record<number, (model: DocumentModel) => DocumentModel> = {
     1: (m) => ({ ...m, newField: defaultValue }),
     2: (m) => { /* migrate from v2 to v3 */ },
   };
   ```
4. For each version gap, apply migrators sequentially.
5. Keep old version files loadable — after `v0.1.0` backward compat may **never** be dropped (additive + migrator only). Post-`v1.0.0`, dropping compatibility requires a MAJOR app version bump.
6. **Update the version assertions.** `crates/core/src/ptz_document.rs` has `writes_the_v3_header_the_loader_expects`, which hard-asserts `version == 3`; it will go red the moment you bump the constant, and its failure message points here. `crates/core/src/ptz_writer_golden.rs` also pins the exact written bytes — regenerate with `cargo test -p photrez-core --lib -- --ignored ptz_writer_golden` and review the diff as the record of the format change.
7. Add a fixture for the new shape to `crates/core/src/ptz_fixtures.rs` and assert it round-trips. `crates/core/src/canonical_model.rs` also contains a `to_ptz_document_json` that writes `version: 4`; it is fenced to `#[cfg(test)]` and is NOT the writer — do not wire it into a save path.

## Testing

- Save a project → verify `document.json` contains `"version": N`.
- Load an alpha file (no version field) → verify no crash, no warning.
- Load a v1 file → verify no warning.
- Load a future-v2 file (manually crafted) → verify warning toast appears.
- Add a new field to model → verify old files load with default value for the new field.
