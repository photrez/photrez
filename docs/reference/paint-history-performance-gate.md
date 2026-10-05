# Paint History Performance Gate

Status: **documentation-only.** There is no executable budget test behind this
gate. The budget constant and its guards ARE executable and tested elsewhere; the
scenario table below is arithmetic, not a passing run. This document says so
rather than implying otherwise.

## Why This Exists

Brush and eraser commits currently use the normal snapshot history path. `createSnapshot()` is a shallow model snapshot, so it does not clone every pixel on every commit. Paint commits still create new bitmap generations, however, and undo/redo history can keep those generations reachable. Large paint-heavy documents therefore need a measurable budget and a migration plan before release claims scale readiness.

This gate mitigates FRR-BRUSH-002 without rewriting undo/redo yet.

## What Is Actually Executable

Verified against the source, 2026-10-06:

| Claim | Status | Evidence |
| --- | --- | --- |
| A pixel-memory budget constant exists | executable | `MAX_PIXEL_BUDGET = 1024 * 1024 * 1024` (1 GiB), `apps/desktop/src/engine/types.ts:193` |
| `MAX_HISTORY_DEPTH` is 50 | executable | `apps/desktop/src/engine/types.ts:192`; consumed by `CommandHistory` (`engine/history.ts:223`) and by the Rust stream's `max_depth` (`crates/core/src/document_core.rs:85`) |
| Adding a layer is budget-guarded | executable | `canAddLayer` (`engine/layerOps.ts:499-502`) |
| Setting a layer bitmap is budget-guarded | executable | `setLayerImageBitmap` (`engine/document.ts:1419-1425`) |
| Resizing the canvas is budget-guarded | executable | `resizeCanvasExceedsBudget` (`engine/document.ts:97-109`) |
| The Rust pixel store reports its own byte footprint | executable | `rust_pixels_store_bytes` (`apps/desktop/src-tauri/src/pixel_store_bytes.rs`), TS wrapper `lib/protocol/pixelStoreBytes.ts` |
| A test named `paintHistoryBudget.test.ts` exists | **NO - does not exist** | `apps/desktop/src/engine/__tests__/` has no such file |
| `bun run perf:paint-history` runs a test | **NO - the script has no target** | root `package.json:17` delegates to `photrez-desktop`, whose `package.json` defines no `perf:paint-history`; the command exits 1 with `Script "perf:paint-history" not found` |

So: the guards are enforced in production code and covered by existing tests, but
**this gate has no executable test of its own**. There is no
`bun run perf:paint-history` that can go green, and the release rule below must
not be read as claiming one does. Any change to `MAX_HISTORY_DEPTH`,
`MAX_PIXEL_BUDGET`, or the canvas dimension limit updates the table here, and
this paragraph, in the same change.

## Current Budget Scenario

Default constants:

- `MAX_HISTORY_DEPTH`: 50
- `MAX_PIXEL_BUDGET`: 1,073,741,824 bytes (1 GiB)
- bytes per RGBA pixel: 4

Reference scenario:

| Scenario | Estimate | Budget result |
| --- | ---: | --- |
| 4096 x 4096 full-layer bitmap | 67,108,864 bytes | one layer generation is within budget |
| 4096 x 4096 full-layer bitmap x 50 history entries | 3,355,443,200 bytes | exceeds the 1 GiB budget by 3.1x |
| 256 x 256 dirty region x 50 entries x undo/redo patches | 26,214,400 bytes | within the 1 GiB budget |
| Dirty-region proposal ratio for that scenario | 0.78125% of the full snapshot estimate | acceptable target |

Each estimate is the pixel count times 4: `4096*4096*4 = 67,108,864`, times 50
entries for the second row; `256*256*4 = 262,144` per patch, two patches
(undo + redo) per entry, times 50 entries for the third; and
`26,214,400 / 3,355,443,200 = 0.78125%` for the ratio.

**Correction, 2026-10-06.** The one error in this table was the budget constant: it read `MAX_PIXEL_BUDGET: 256 MiB` where the code has always been 1 GiB (`engine/types.ts:193`). Every ESTIMATE above was already correct and is unchanged by this correction - including the 26,214,400-byte dirty-region row and the 0.78125% ratio, which were internally consistent with each other and with the full-snapshot row.

What changes is only the "budget result" verdicts that were judged against the wrong constant. The 50-entry row's verdict was previously the bare text `exceeds 256 MiB budget`, with no multiple stated anywhere in it; it now reads `exceeds the 1 GiB budget by 3.1x`, where `3,355,443,200 / 1,073,741,824 = 3.125`. The dirty-region row's verdict likewise changed only in which budget it names.

## Rust-Side Byte Accounting

The Rust pixel owner can report its OWN footprint, through the read-only
`rust_pixels_store_bytes` command (`apps/desktop/src-tauri/src/pixel_store_bytes.rs`,
backed by `crates/core/src/pixel_store/byte_accounting.rs`). It reports two
populations that share nothing with each other:

- `row_major_bytes`: the active canonical mirror, one `Vec<u8>` per layer, read
  from each buffer's own length.
- `tile_graph`: the tiles behind the copy-on-write history states, counted ONCE
  per distinct tile - keyed by `TileRef::identity_key()`, the `(data_ptr,
  offset)` pair the production touch-set already uses - and split into
  `shared_bytes` (tiles two or more committed states reference) and
  `private_bytes` (tiles one state owns).

The unit is the TILE, not the `Arc<[u8]>` block. `tiled_state_node` packs a whole
layer into ONE block at distinct offsets, so a block-level key would hand
`shared_bytes` the entire block - including the byte regions of tiles that were
edited and are no longer referenced from it. On a 4096 x 4096 layer (16 x 16 =
256 tiles of 262,144 bytes) whose anchor block is 67,108,864 bytes, a commit that
re-tiles 8 of them reports 65,011,712 shared (the 248 untouched tiles) rather than
the whole 67,108,864-byte block.

This is a MEASUREMENT, not an estimate, and it supersedes any closed-form figure
for the Rust side. Honest limits, in both directions:

1. The row-major mirror duplicates every layer's pixels independently of the tile
   graph, so the store holds the pixels twice by construction while
   `STATE_NODE_CANONICAL` and `TILE_MAJOR` are both off. Byte accounting makes
   that visible; it does not remove it.
2. `shared_bytes` reads LOW, never high, and can reach 0: a commit that touches
   every tile of a layer re-tiles all of them, so nothing keeps the anchor's
   identity and nothing is shared. A single-tile layer (anything under 256 x 256)
   is always in this case. Both are pinned by tests so the figure cannot drift
   into a flattering constant.
3. `shared_bytes` reads LOW in the other direction too, relative to the allocation
   it describes: a re-tiled tile's byte range inside the anchor block is no longer
   referenced by the new state, but the whole block is still resident as long as
   ANY of its tiles is. So the sum of `shared_bytes + private_bytes` correctly
   partitions `total_bytes`, yet the true allocator footprint can exceed
   `total_bytes` by the orphaned tail of a partially-referenced block. That
   residue is not tracked anywhere in the store.
4. A separate undercount mechanism, unrelated to the split and NOT covered by
   limit 3: the walk used to key visited states on `StateNode::id`, which is
   minted per `LayerState` from that `LayerState`'s own arena starting at 0. A
   re-seed or resize drops the `LayerState` while `invalidate_layer`
   early-returns under a `pending_external` barrier, so a stale `Pixel` entry can
   survive with its states still alive while the next commit's fresh arena mints
   the same ids - and the colliding generation's blocks would never be visited.
   This is FIXED: the walk now keys on the `Arc<StateNode>` allocation address,
   which cannot collide while both are alive. The regression test is
   `bytes_survive_an_arena_reset_while_the_invalidate_barrier_is_set`. It is
   listed because the underlying store condition - a stale entry surviving an
   invalidation - is still live and is not an accounting bug to be re-fixed.
5. Neither term counts `TileRef` / `StateNode` / `Arc` headers, `Vec`
   over-allocation, the host-side `ImageBitmap` generations, or the TypeScript
   undo stack. Those belong to other owners.

## Jank: Which Half Is Measurable

The interaction-latency requirement splits into two halves with different
answers, and conflating them is how a green-but-false measurement gets shipped.

- **The commit half is already headless-measurable.** `native_commit_latency_bench`
  (`apps/desktop/src-tauri/src/protocol_native_cmds.rs:1129-1200`) is a live
  `cargo test` case, not `#[ignore]`d, so `cargo test -p photrez-desktop` exercises
  the CPU commit path today. Nothing new is needed to measure it.
- **The frame-PACING half needs a real WebView2 window.** The only frame counter
  is `RenderScheduler.getFrameMetrics()` (`apps/desktop/src/renderer/scheduler.ts:93`),
  polled every 2 s by the status bar dev-only (`BottomStatusBar.tsx:53`), and it
  times the render CALLBACK (`:28-30`) - not the interval between frames. A stall
  in compositing, layout, GC or IPC sits outside its window. jsdom has no
  compositor, no GPU and no real raster, and its `requestAnimationFrame` is
  timer-based, so a jsdom "frame time" is a fiction.

Measuring pacing therefore needs a real window: launch with
`WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=<port>`, attach
Playwright over `connectOverCDP`, drive real commits, and sample frame-to-frame
deltas from `requestAnimationFrame` timestamps. `.tmp-harness/cdp-probe.sh`
stages that path. Do not add a jsdom frame-time test to stand in for it.

## Dirty-Region History Proposal

The next paint-history implementation should store paint commands as bounded dirty-region patches instead of relying on full-layer bitmap generations for every stroke.

Required command payload:

- document id and layer id
- dirty rect in layer-local coordinates
- before patch RGBA bytes for undo
- after patch RGBA bytes for redo
- paint settings hash for diagnostics

Required invariants:

- Dirty rect must be clipped to layer bounds before allocation.
- Patch bytes must be rejected if `dirtyRegionUndoRedoBytes > MAX_PIXEL_BUDGET`.
- Undo restores only the dirty rect and marks that layer dirty for texture upload.
- Redo reapplies only the dirty rect and marks that layer dirty for texture upload.
- A stroke that touches the full layer is still allowed only if its estimated patch bytes pass the same budget gate.

## Release Rule

FRR-BRUSH-002 can remain mitigated while snapshot history is still the runtime path only if:

1. The budget guards listed above still exist and their existing tests still pass.
2. Any increase to `MAX_HISTORY_DEPTH`, the pixel budget, or the canvas dimension limit updates this document in the same change.
3. Large-canvas release notes explicitly state that dirty-region history is planned but not yet the active runtime implementation.

There is deliberately no item here that names `bun run perf:paint-history`,
because that command does not run. If a budget test is added later, add the
command and the script at the same time, and update this section.
