# photrez-core

`photrez-core` is the workspace-internal authority for the persistent project,
the live document, pixel ownership, history, and CPU rendering. The crate is not
published (`publish = false`). The `protocol` module is the stable facade the
application uses.

## Owns

- The canonical per-layer pixel buffers, tile patches, and copy-on-write state.
- One history stream per document, including the metadata and pixel halves of a
  step.
- Command dispatch, structural and canvas operations, and the pixel registry.
- One CPU compositing call path: this crate calls `photrez-render` and implements
  its `RenderInput`. The paint rasterizer and brush geometry the tool path uses
  are `photrez-raster` modules.

The persisted content model and the `LayerMeta` value live in
`photrez-document`; the pixel buffers, kernels, paint rasterizer and brush
geometry in `photrez-raster`; the `.ptz` envelope, its writers and readers, and
the image codecs in `photrez-format`; and the compositor in `photrez-render`.
Each is re-exported here so this crate stays the single entry point the
application imports.

## Does not own

- The document content model types themselves (see `photrez-document`).
- GPUI state, views, input policy, or window lifecycle.
- File I/O, save streams, autosave, printing, or platform adapters.
- A second document store or a second history cursor.
- A layer type of its own: `render` reads `LayerMeta` and must not define one.

## Module boundaries

- `engine` owns the live document: `DocumentCore`, its history stream, command
  dispatch, the structural and canvas families, the structural-sharing `LayerSet`,
  and the snapshot DTO. `document` is only the re-export of `photrez-document`.
- `raster` holds the pixel buffer, tiles, the copy-on-write state-node seam, the
  compute kernels, and the paint and brush geometry. It uses no `engine`,
  `document`, `pixel`, `wire`, or `protocol` type, and an inherent method of a
  raster type lives in `raster`.
- `pixel` holds the registry, the document store, and the history-aware byte
  accounting. It may use `raster`, never the reverse, and it re-exports
  `photrez-format`'s `export` so the host's save path keeps one entry point.
- `wire` may use `document` and `raster::tile`. It must not use `pixel::store`,
  `engine`, or `ptz`.
- `engine` may use `raster::tile`, `raster::state_node`, `wire`, and `ptz`. It
  must not use `pixel::store` or `pixel::document_store`.
- `pixel::document_store` is the only pixel module allowed to use `engine`.
- `pixel::store` may use `pixel::document_store`, never the reverse.
- `pixel::history` re-exports `PayloadKind`, and `pixel::byte_accounting` reads
  `EntryPayload`. Neither reaches `engine::state` or `engine::snapshot`.
- `render` is its own package (`photrez-render`) and composites through
  `render::input::RenderInput`, which `pixel::store::PixelStoreRegistry` implements
  here. Compositing must not gain a registry, store, or history import: that would
  put the canonical mutable buffer inside the render package.
- `protocol` may use `engine` and `document`; neither may use `protocol`.
- `engine` may use `document`, `raster`, `pixel`, `render`, and `wire`. `ptz` is a
  re-export surface only: it must not gain a `use crate::` line, because
  `photrez-format` may not look back into core.
- `raster::paint` and `raster::brush` must not use `engine` or `document`.
- If a change creates a dependency cycle, move the shared type to a leaf module
  before continuing.

## Invariants

- The tile edge is 256 (`raster::tile_store::TILE`).
- The row-major `PixelLayer` is the canonical byte source. Do not move pixel
  reads onto the packed tile store or the state-node seam.
- `.ptz` field names, command envelopes, and error codes do not change.
- The move-commit interval and synchronous document opening stay unchanged.
- Tile copy-on-write identity and one-step metadata plus pixel undo do not
  change.

## Naming

- `DocumentVersion` is a leaf alias in `wire::command.rs`; `ResourceId` is one
  in `photrez-document/src/identity.rs`.
- The layer metadata value is `LayerMeta` in `photrez-document/src/layer.rs`.
  There is no second layer type in `render`.
- Persisted model: `PtzDocumentContent`, `PtzLayer`, `PtzDocument`,
  `PtzReplica`.
- Model projection: `to_render_layer`, `project_known_layer`,
  `project_new_layer`, `merge_into_layer`.
- `canonical` and `shadow` are not synonyms for the persisted model or a second
  authority. Use the names above.

## Known limits

- `raster` is its own package (`photrez-raster`). Core's reach into it is a named
  contract, not an open door: `state_node`, `tile::TilePatch`, `layer::PixelLayer`
  with `write_tile` and `bump_epoch`, and `TileRef::identity_key`. Do not add a
  path through it without a caller that needs it, and keep `protocol` free of the
  write path. `PixelLayer` itself is still nameable by the host through this crate
  and carries public `pixels`, so the gate limits the package edge, not every
  method on a re-exported type.
- The raw process-global registry remains behind the non-default `legacy-host`
  feature. Do not re-export new functions from it.
- `geometry` and `parallel` are public leaf helpers with no in-repo caller. Do
  not add behavior there without a consumer.
- `pixel::store -> raster::paint::raster` is a real composition edge. Do not
  extend it.

## Test

```text
cargo test -p photrez-core
```

The crate has no GPUI dependency. Use this command for logic changes. The
application suite owns the production wiring tests. File and test layout follow
`docs/CONVENTIONS.md`. The Cargo integration tests under `tests/` are
benchmark-style targets; keep them there.
