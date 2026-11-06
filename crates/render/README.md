# photrez-render

CPU compositing and the rasterizers behind it. The package is workspace-internal
(`publish = false`) and depends on `photrez-document` and `photrez-raster` only.

## Owns

- `compositor`: the document-to-pixels entry point. It covers layer stacking, blend modes,
  opacity, transforms, and the clipped-region path the editor redraws through.
- `shape` and `text`: the shape and text rasterizers a committed layer bakes from.
- `input`: `RenderInput` and `RenderLayerPixels`, the read-only pixel source the
  compositor draws from.

## Does not own

- Where the pixels live. `photrez-core` owns the registry and implements
  `RenderInput` for it; nothing in this package names a registry, a document
  store, or a history type.
- The layer model. `photrez-document` owns `LayerMeta` and the persisted content.
- File I/O and GPUI.

Compositing is the CPU authority: the output is correct without a GPU, and any
accelerated path must match it rather than replace it.

## Test

```text
cargo test -p photrez-render
```

The golden proofs compare against committed PNGs under `tests/goldens/`. Text
goldens pin one font by exact bytes, described on `TEST_FONT` in `src/text.rs`.
The `write_goldens` and `write_multilayer_golden` tests are `#[ignore]`d
fixture writers: run them deliberately, with `--ignored`, only when a
rasterizer change is intended, and commit the PNGs in the same change.
