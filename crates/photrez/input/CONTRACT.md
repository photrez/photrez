# Input / pointer per-tool contract

Status: **CENSUS COMPLETE** (2026-10-25). Every row is sourced from the frozen
reference or a named native API. Native implementation and the A3 event-path
proof are still pending.
Source commit: `360e18b1f1d55f7d19261f3881cb94199b054234`.

The frozen reference is `apps/desktop/src`; the native input module owns this
contract. It must drive a real gpui pointer/key event through the production
path into a real `crates/core` result, not a direct dispatcher call.

## Census commands

```bash
# tool ids
sed -n '1p' apps/desktop/src/components/editor/tools/toolTypes.ts

# dispatcher branches
grep -o 'activeTool() === "[a-zA-Z]*"' \
  apps/desktop/src/components/editor/canvas/useCanvasPointerTools.ts | sort -u

# cursor resolver
sed -n '35,95p' apps/desktop/src/viewport/cursorResolver.ts

# tool shortcuts
grep -n 'setActiveTool' \
  apps/desktop/src/components/editor/canvas/keyboardShortcuts/toolNav.ts
```

## Tool table

| Tool | Frozen pointer entry | Frozen core/document call | Native core command candidate | Frozen cursor | Shortcut |
|---|---|---|---|---|---|
| `move` | `useCanvasPointerTools.ts` move branch (line 526); layer routing via `layers/transformRouting` | `engine.moveLayerSilent` per move (`viewport/input-handler.ts:319`); one `history.commit(preDragSnapshot)` on pointer-up (`layers/useCanvasLayerDrag.ts:669`) | `Command::TransformLayer` (one committed edit per layer) | handle/rotate cursors from `getCursorForHandle` / `cursorRotate` | `V` |
| `selection` | `useCanvasPointerTools.ts:658,935`; `prepareToolContext.ts:107` | `engine.createSelection(x,y,w,h,angle,shape)` | `Command::SetSelection` / `ClearSelection` | `crosshair` | `M` (Shift+M toggles shape) |
| `crop` | `startCropDrag` + `pointerTools/modernCrop.ts`; pointer commit via `prepareToolContext.ts:139` `editor.setCropRect` | pointer sets the crop rect only; apply is a separate command at `cropToolActions.ts:173` (`engine.applyCrop`) | `Command::ApplyCrop`; `CropCanvas` is the destructive variant | `crosshair`; handle cursors when over a handle | `C` |
| `eyedropper` | `viewport/input-handler.ts:159,286` (dispatcher comment at line 510 says it routes there) | `engine.samplePixel(x,y)` -> `setFgColor`; `performPixelSampling` (`engine/pixelSample.ts:18`) is a bottom-to-top Normal composite, transform-aware, text bitmap at 2x | add a core sampling helper that replicates `performPixelSampling` over `PixelLayer.pixels` (`pixel/layer.rs:15`); do not use the full compositor without a labelled divergence | `crosshair` | `I`; Alt+brush/eraser at dispatcher 516/779 |
| `brush` | `useBrushOverlay.ts`; dispatcher 624-705/726-835 | `commitBrushStroke` (`useBrushOverlay.ts:1159`), which has tile and legacy commit paths | `BrushStrokeEngine::update` (`crates/core/src/brush/engine.rs`) produces dabs; `PixelStore::commit_pixels(..., include_tiles)` (`pixel/store/pixels.rs:243`) is the canonical commit; `brush_stamp` (`pixel/kernel/adjustments.rs:135`) builds the mask | `none` | `B` |
| `eraser` | same as brush with `isEraser` | `commitBrushStroke(..., isEraser=true)` | same native path with `eraser=true`; the frozen tile/legacy split maps to `include_tiles` and must be proven in A3 | `none` | `E` |
| `paintBucket` | `pointerTools/paintBucket.ts`; dispatcher `applyPaintBucketFill` | `floodFill` (`paintBucket.ts:117,205`); `engine.setLayerImageBitmap` (`:221`) | `flood_fill` (`crates/core/src/pixel/kernel.rs:159`) + `set_layer_pixels` (`pixel/engine.rs:68`) or `commit_pixels` | frozen custom SVG, hotspot 3/18; no custom-cursor API -> `CursorStyle::Crosshair` `[REHOST]` | `G` (toggles with gradient) |
| `gradient` | `pointerTools/gradientTool.ts`; dispatcher `startGradientDrag`/`applyGradientFill` | `applyGradientFill` (`gradientTool.ts:115`), `engine.snapshot`/surface write | `gradient_fill` (`crates/core/src/pixel/kernel.rs:322`) + layer write | frozen custom SVG, hotspot 12/12; no custom-cursor API -> `CursorStyle::Crosshair` `[REHOST]` | `G` (toggles with paint bucket) |
| `shape` | `pointerTools/shapeTool.ts`; dispatcher `startShapeDrag` | `engine.addShapeLayer` (`shapeTool.ts:178`) | `Command::AddLayer` with `ShapeParams`; later `SetLayerParams` | `crosshair` | `U` |
| `text` | `pointerTools/textTool.ts`; dispatcher `startTextPointer` | `engine.addTextLayer` (`textTool.ts:325`) | `Command::AddLayer` with `TextData`; later `SetLayerParams` | `text` | `T` |

## Census items (must be resolved before the contract is complete)

- `brush`/`eraser`: the native API mapping is named (`BrushStrokeEngine` ->
  `commit_pixels`), but the frozen tile/legacy split to `include_tiles`, the
  history-before-mutation step, and the exact pointer-up commit point need A3
  proof. This is a wiring question, not an API unknown.
- `eyedropper`: A3 must add the core sampling helper (Normal composite,
  transform-aware, text 2x) and prove out-of-bounds transparency plus the
  Alt+brush/eraser override and foreground/background target semantics.
- `paintBucket`/`gradient`: exact native layer-write and history path; dirty
  region behavior; fill-mask/selection interaction.
- `shape`/`text`: exact `AddLayer` payload field mapping and the pointer-up
  commit point; later param edits are `SetLayerParams`, not pointer input.
- Custom SVG cursors: no custom image/hotspot API exists in the pinned 0.7.3
  surface (`CursorStyle` only), so outcome (2) applies: use
  `CursorStyle::Crosshair` for paint bucket/gradient and record `[REHOST]` in
  the behavior ledger. This is a labelled divergence, not a silent substitution.
- The shortcut column covers tool selection; the full keyboard map is a separate
  binding gate and must be diffed against README §81 and `toolNav.ts`.

This table is a census, not implementation evidence. No production handler or
test is claimed by this file.
