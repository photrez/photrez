# Core module contract

`photrez-core` is the in-process authority for the persisted project, the live
document, pixel ownership, render projection, and paint/brush evaluation. The
crate is workspace-internal (`publish = false`); `protocol` is its stable
external facade.

## Modules

| Module | Owns |
|---|---|
| `wire::command` | `DocumentVersion`, `ResourceId` live here; they remain leaf aliases rather than a separate `ids` module. |
| `ptz` | The persisted `.ptz` model: `PtzDocumentContent`, `PtzLayer`, file envelope, projection/replica/validation, fixtures. |
| `document` | The live `DocumentCore`, command application, history, structural operations, canvas operations, duplicate-name planning. |
| `wire` | Crate-private command/projection/history DTOs exchanged between the runtime and the protocol facade. |
| `render` | `RenderLayer`, `Rect`, `RenderLayerChange`; render-owned projection leaves. |
| `pixel` | Pixel layers, tile patches, the pixel store, state nodes, tile store, byte accounting, kernel adjustments, export. |
| `paint`, `brush` | Paint parity and brush evaluation; leaves above `pixel`/`ptz`. |
| `protocol` | The public typed protocol facade consumed by `src-tauri`; the callerless process-global engine registry was removed during the native port. |
| `geometry`, `parallel` | Shared leaf helpers. |

## Actual tree (2026-10-21)

```text
lib.rs
geometry.rs  parallel.rs
document.rs
document/  engine.rs  engine/state.rs  apply.rs  apply/{create,metadata,selection}.rs
           canvas.rs  structural.rs  history.rs  history/payload.rs
           snapshot.rs  layer_set.rs  duplicate_name.rs  tests.rs  tests/
pixel.rs
pixel/     store.rs  store/{access,history,pixels}.rs  document_store.rs
           engine.rs  layer.rs  tile.rs  tile_store.rs  state_node.rs
           byte_accounting.rs  kernel.rs  kernel/adjustments.rs
           history.rs  export.rs  parity_oracle.rs  tests.rs  tests/
ptz.rs
ptz/       content.rs  content/layers.rs  file.rs  file/{read,write}.rs
           layer_params.rs  projection.rs  replica.rs  validate.rs
           fixtures.rs  tests.rs  tests/
paint.rs   paint/{parity.rs,tests.rs,tests/}
brush.rs   brush/{engine.rs,tip.rs,tests.rs,tests/}
render.rs  render/layer.rs
wire.rs    wire/{command,history,projection}.rs  wire/tests.rs  wire/tests/
protocol.rs  protocol/registry.rs
```

Deviations from the target sketch: no `ids.rs` (leaf aliases stay in
`wire/command.rs`); the apply dispatcher is split into three families, not six;
`document/history/payload.rs` stays beside the history module; `render/layer.rs`
and `document/layer_set.rs` own the old `model` split.

## Dependency direction

- `DocumentVersion`/`ResourceId` are leaf aliases in `wire/command.rs`.
  `render/layer` is a leaf for render values.
- `wire` may use `ptz::content`, `render/layer`, and `pixel::tile`; it
  must not use `document` or `pixel::store`.
- `document::engine` may use `pixel::tile`, `pixel::state_node`, `wire`, `ptz`,
  and `render/layer`; it must not use `pixel::store` or `pixel::document_store`.
- `pixel::document_store` is the only pixel module allowed to use
  `document::engine` and `document::snapshot`. `pixel::store` may use
  `pixel::document_store`, never the reverse.
- `document::history` is reachable from two pixel modules: `pixel::history`
  re-exports `PayloadKind` for the public callers, and `pixel::byte_accounting`
  reads `EntryPayload`. Neither reaches `document::engine` or
  `document::snapshot`.
- `pixel::store` uses the `paint::parity` rasterizer for the shadow path.
  `paint::parity` itself uses nothing from `pixel`; only `paint::tests` does,
  so the two modules do not form a production cycle.
- `document::snapshot` must not use `pixel::store`; its cross-cutting tests live
  under `document/tests/`.
- `protocol` may use `document`; `document` must not use `protocol`.
- `paint` and `brush` must not use `document`.
- `pixel::store` may call crate-private accessors on `DocumentPixelStore`
  (for example `cow_tiles`); those never become public API.
- `pixel::kernel` owns the fill kernels (flood fill, gradient fill) and
  re-exports the adjustment surface; `pixel::kernel::adjustments` owns the
  brightness/contrast/saturation bake, the transparent-bounds trim, the brush
  stamp and the composite. The split is a pure file move - no code was rewritten.
- `pixel::store::tests` is the store's test parent; the history-unification
  proof and the native-authority proof live in `pixel::store::tests/` so each
  file stays at or under the 1,000-line hard cap.

If a move creates a cycle, stop and move the shared type into a leaf module
before continuing.

## Naming

- Persisted model: `PtzDocumentContent`, `PtzLayer`, `PtzDocument`, `PtzReplica`.
- Render projection: `RenderLayer`, `to_render_layer`, `project_known_layer`,
  `project_new_layer`, `merge_into_layer`.
- Brush: `BrushTip`, `build_brush_tip`.
- `canonical` and `shadow` are not synonyms for the persisted model or a second
  authority; use the names above.
- Module names describe the domain, not an internal effort label.

## Tests and size

- Tests live next to their owner: `module/tests.rs` or `module/tests/`.
  `document/tests/` is for document-runtime integration tests only; it is not
  the destination for every core test.
- No `#[path]` test indirection. No root-level `*_tests.rs`; the two Cargo integration tests under `crates/core/tests/` are deliberate benchmark-style targets permitted at the crate root.
- Local policy: non-baseline production and test files stay at or under the
  1,000-line hard cap; warnings begin at 900 lines. Existing entries in
  `scripts/rust-file-size-baseline.txt` are frozen `apps/desktop` reference files
  and receive ratchet-only enforcement: growth fails, but the new cap and warning
  do not apply. No new baseline entries are allowed.

## Invariants

- The tile edge is 256 (`pixel::tile_store::TILE`); do not change it here.
- The existing move-commit interval and synchronous opening are unchanged.
- `STATE_NODE_CANONICAL` and `TILE_MAJOR` remain `false`; no default behavior
  changes in a structure-only change.
- `.ptz` serialized field names, command envelopes, and error codes do not
  change.

## Known debt

The frozen Tauri adapter retains `pixel::store::registry()` under the
non-default `legacy-tauri` feature while it remains a build/test reference.
The standalone GPUI app uses a private typed `PixelStoreRegistry` instance; it
cannot access the process-global registry, raw document map, or direct mutable
layer accessor. The callerless `protocol::registry::ENGINES` and its exports
were removed. Do not re-export new functions from that registry.

`geometry` and `parallel` are public leaf helpers with no in-repo caller; do
not add behavior there without a consumer. `pixel::store` -> `paint::parity`
is a real composition edge; do not extend it. `document/tests/commands.rs` is
near the 900-line warning threshold; split it before adding more cases.
