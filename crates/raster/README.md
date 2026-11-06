# photrez-raster

The raster primitives Photrez edits pixels with. The package is workspace-internal
(`publish = false`) and depends on no other package here.

## Owns

- `layer`: `PixelLayer`, the canonical row-major RGBA buffer for one layer, with its
  tile write path and the cache-validity `epoch`.
- `tile` and `tile_store`: the `TilePatch` value and the packed tile-major store.
- `state_node`: the per-layer copy-on-write `StateNode` seam, `TileRef`, and
  `LayerState`.
- `kernel`: the pixel compute kernels over an RGBA plane.
- `paint`: the paint rasterizer whose bytes the host's paint path is compared
  against, plus its tile-shaped output.
- `brush`: the stroke spacing state machine and the brush tip bitmap its dabs are
  stamped with.
- `engine`: an in-place compute buffer type that no caller in this workspace uses.
  Its own tests are the only construction sites, so it waits on an owner decision
  rather than serving as the package's compute entry point.
- `reference`: an independent copy of the pixel reads, used only to check the packed
  canon against something that does not share its code.

## Does not own

- Document state. No command, history entry, document identity, or registry type
  reaches this package; `photrez-core` owns those and binds them to these buffers.
- The `.ptz` format, file I/O, and rendering.

Core's uses of this package are the contract listed in
[`crates/core/README.md`](../core/README.md). What the boundary gate enforces is the
package edge: `photrez` may not depend on this package, so the way the host names
these types is through `photrez-core`. The gate cannot make an inherent method
unreachable, and it does not today: `PixelLayer` carries `pub width`, `pub height`
and `pub pixels`, and the application already builds and reads canonical buffers
(`services/document.rs` creates one and snapshots its region, and a measurement
binary under `src/bin/` does the same). Making the canonical buffer unnameable
from the host is a separate design step, not something this package's visibility
can claim.

## Test

```text
cargo test -p photrez-raster
```

The `test-reference` feature exposes the reference copy to another package's tests.
No production build of this workspace enables it. `paint` and `brush` need no
package here: they take and return bytes, which is what keeps this package a leaf.
The one external crate they use, `rayon`, is a dev-dependency for the paint
measurement experiments only.
