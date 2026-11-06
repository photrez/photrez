# photrez-document

The canonical Photrez document model. The package is workspace-internal
(`publish = false`) and depends on no other package here.

## Owns

- `content`: the persisted document content model. It carries layers, transforms, blend
  modes, viewport, and selection state.
- `layer_params`: the nested per-layer payloads (shape, text, basic adjustment).
- `layer`: `LayerMeta`, the per-layer metadata value the engine stores in
  history, its dirty `Rect`, and the `LayerChange` delta one command emits.
- `identity`: `ResourceId`, the token handle for a layer's pixel resource.

## Does not own

- Active document state, the history stream, or command dispatch. Those stay in
  `photrez-core`.
- The `.ptz` envelope, the JSON writers, and file I/O. Those stay in
  `photrez-core::ptz`.
- Rendering. The compositor reads `LayerMeta`; it does not define a layer type.

Serialized field names here are the `.ptz` on-disk contract. A change must be
additive and ship a migrator; see [`docs/ptz-migration.md`](../../docs/ptz-migration.md).

## Test

```text
cargo test -p photrez-document
```
