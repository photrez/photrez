import type { DocumentEngine } from "@/engine/document";
import type { CommandHistory, HistoryTilePatches } from "@/engine/history";
import type { WebGL2Backend } from "@/renderer/webgl2";
import { compositeAllLayers } from "@/engine/layerComposite";
import { applyBasicAdjustmentToColor } from "@/engine/layerAdjustments";
import { SelectionOperations } from "@/features/selection/SelectionOperations";
import type { SelectionState } from "@/features/selection/SelectionTypes";
import type { LayerNode, DocumentModel } from "@/engine/types";
import { applyRustTilesToSurface, projectRustPixelsToVisibleSurface, rehydratePaintSurfaceFromRust } from "@/lib/rustShadow";
import { syncFacadeVersionFromPixel } from "@/lib/protocol/facadeRegistry";
import { computeChangedRegion, reconstructLayerBuffer } from "@/lib/paint/regionProducer";
import { selectionUploadRect } from "@/components/editor/canvas/keyboardShortcuts/selectionTool";
import { computeDirtyRegion } from "@/lib/paint/regionProducer";
import { resolveRustPixelOperationArm } from "@/lib/paint/rustPixelOperationArm";
import { showToast } from "../Toast";
import { ipcErrorMessage } from "@/tauri/native";

export function mergeActiveLayerDown(
  engine: DocumentEngine,
  history: CommandHistory,
  renderer: WebGL2Backend,
  activeId: string,
) {
  const beforeLayers = engine.getLayers();
  const activeIndex = beforeLayers.findIndex((layer) => layer.id === activeId);
  const bottomLayer = activeIndex >= 0 ? beforeLayers[activeIndex + 1] : null;

  if (!bottomLayer) {
    return false;
  }

  history.commit(engine.snapshot(), "Merge Down");
  engine.mergeDown(activeId);
  renderer.destroyTexture(activeId);
  renderer.destroyTexture(bottomLayer.id);

  const mergedLayer = engine.getLayer(engine.getActiveLayerId() || "");
  if (!mergedLayer?.imageBitmap) {
    return false;
  }
  renderer.uploadImage(mergedLayer.id, mergedLayer.imageBitmap);

  return true;
}

export function mergeSelectedLayers(
  engine: DocumentEngine,
  history: CommandHistory,
  renderer: WebGL2Backend,
  layerIds: string[],
): boolean {
  if (!layerIds || layerIds.length < 2) {
    return false;
  }

  history.commit(engine.snapshot(), "Merge Selected Layers");
  engine.mergeSelectedLayers(layerIds);

  for (const id of layerIds) {
    renderer.destroyTexture(id);
  }

  const activeId = engine.getActiveLayerId();
  const mergedLayer = activeId ? engine.getLayer(activeId) : null;
  if (!mergedLayer?.imageBitmap) {
    return false;
  }
  renderer.uploadImage(mergedLayer.id, mergedLayer.imageBitmap);

  return true;
}

export function deleteMultipleLayers(
  engine: DocumentEngine,
  history: CommandHistory,
  renderer: WebGL2Backend,
  layerIds: string[],
): boolean {
  if (!layerIds || layerIds.length === 0) return false;
  const currentLayers = engine.getLayers();

  // Guard against deleting background or leaving zero layers
  const validIdsToDelete = layerIds.filter((id) => {
    const layer = currentLayers.find((l) => l.id === id);
    return layer && !layer.isBackground;
  });

  if (validIdsToDelete.length === 0) return false;

  // Cannot delete all layers — must preserve at least one
  if (currentLayers.length - validIdsToDelete.length < 1) {
    const canDeleteCount = currentLayers.length - 1;
    if (canDeleteCount <= 0) return false;
    validIdsToDelete.splice(canDeleteCount);
  }

  history.commit(
    engine.snapshot(),
    validIdsToDelete.length > 1 ? "Delete Layers" : "Delete Layer",
  );

  for (const id of validIdsToDelete) {
    engine.deleteLayer(id);
    renderer.destroyTexture(id);
  }

  return true;
}

export function duplicateMultipleLayers(
  engine: DocumentEngine,
  history: CommandHistory,
  renderer: WebGL2Backend,
  layerIds: string[],
): string[] {
  if (!layerIds || layerIds.length === 0) return [];
  const currentLayers = engine.getLayers();
  const validIds = layerIds.filter((id) => currentLayers.some((l) => l.id === id));
  if (validIds.length === 0) return [];

  history.commit(
    engine.snapshot(),
    validIds.length > 1 ? "Duplicate Layers" : "Duplicate Layer",
  );

  const newIds: string[] = [];
  for (const id of validIds) {
    try {
      const dup = engine.duplicateLayer(id);
      if (dup.imageBitmap) {
        renderer.uploadImage(dup.id, dup.imageBitmap);
      }
      newIds.push(dup.id);
    } catch {
      // Continue if resource limit reached on some
    }
  }

  return newIds;
}

export function flattenAllLayers(
  engine: DocumentEngine,
  history: CommandHistory,
  renderer: WebGL2Backend,
) {
  const oldLayerIds = engine.getLayers().map((layer) => layer.id);
  if (oldLayerIds.length <= 1) {
    return false;
  }

  history.commit(engine.snapshot(), "Flatten Image");
  engine.flattenLayers();

  for (const id of oldLayerIds) {
    renderer.destroyTexture(id);
  }

  const flattenedLayer = engine.getLayer(engine.getActiveLayerId() || "");
  if (!flattenedLayer?.imageBitmap) {
    return false;
  }
  renderer.uploadImage(flattenedLayer.id, flattenedLayer.imageBitmap);

  return true;
}

export function stampVisibleLayers(
  engine: DocumentEngine,
  history: CommandHistory,
  renderer: WebGL2Backend,
) {
  const layers = engine.getLayers();
  const visibleLayers = layers.filter((l) => l.visible);
  if (visibleLayers.length === 0) return false;

  history.commit(engine.snapshot(), "Stamp Visible");

  const w = engine.getWidth();
  const h = engine.getHeight();
  const composite = compositeAllLayers(visibleLayers, w, h);
  if (!composite) {
    return false;
  }

  const newLayer = engine.addLayer("Stamp Visible", w, h);
  engine.setLayerImageBitmap(newLayer.id, composite);
  renderer.uploadImage(newLayer.id, composite);

  return true;
}

/**
 * Fill the active layer with a solid color (Alt+Del / Ctrl+Del).
 * Replaces the entire layer content with an opaque `color` bitmap. Skips
 * locked layers and layers with no active id. Commits history BEFORE mutation
 * so the fill is undoable/redoable, then uploads the new bitmap to the renderer.
 *
 * Rust records the fill: it writes through `rust_pixels_write_region` (one
 * canonical `Pixel` history entry) and drives the derived TS PaintTileSurface +
 * history memento from the Rust result. A single user Fill yields exactly ONE
 * user-visible undo step: the TS entry is a cursor token marked `rustOwned`, and
 * the undo/redo dispatch takes the pixels from Rust while draining that token in
 * lockstep. A layer with no paint surface (no raster yet) is a visible error.
 */
export function fillActiveLayerWithColor(
  engine: DocumentEngine,
  history: CommandHistory,
  renderer: WebGL2Backend,
  color: string,
): boolean {
  const activeId = engine.getActiveLayerId();
  if (!activeId) return false;

  const layer = engine.getLayer(activeId);
  if (!layer || layer.locked) return false;

  // A solid fill is a uniform color — apply the layer adjustment to the color
  // directly (O(1)) instead of baking every pixel on the CPU. This matches the
  // shader's applyAdjustment on the fill and then drops the adjustment param.
  const fillColor = layer.basicAdjustment
    ? applyBasicAdjustmentToColor(color, layer.basicAdjustment)
    : color;

  const sel = engine.getSelection();

  // UNCONDITIONAL: a Fill Layer is always recorded by Rust, so the default
  // state of photrez.rustPixels (key absent or "0") takes the same path as "1".
  // Gating on it sent the default state down the TS-only commit, which recorded
  // no Rust Pixel entry and therefore had no twin for the undo drain to move in
  // lockstep. The shared operation arm still decides rust/blocked: a missing
  // paint surface is a visible error, never a silent second pixel owner.
  let surface = engine.getPaintSurface(activeId);
  if (!surface && layer.width > 0 && layer.height > 0) {
    // A layer with no raster yet (a freshly added blank layer) has nothing to
    // seed the canonical store from, and `getPaintSurface` is what the Rust
    // entry is seeded from. Materialise the empty raster so the surface and the
    // Rust entry both exist and the fill runs through the same single-owner
    // path as any other layer - the pixels it fills are opaque, so nothing of
    // the blank starting state survives. Without this, Alt+Del on a new layer
    // silently did nothing. setLayerImageBitmap drops the cached surface, so
    // this must run BEFORE the getPaintSurface above is reused.
    //
    // The context MUST be obtained before transferring. transferToImageBitmap()
    // throws InvalidStateError on a canvas whose 2d context was never requested,
    // which killed the canvas on the first use of this block; the same rule is
    // honoured in useBrushOverlay for its scratch canvas.
    const blank = new OffscreenCanvas(layer.width, layer.height);
    blank.getContext("2d");
    engine.setLayerImageBitmap(activeId, blank.transferToImageBitmap());
    surface = engine.getPaintSurface(activeId);
  }
  const arm = resolveRustPixelOperationArm("fill", true, surface !== null);
  if (arm !== "legacy") {
    if (!surface) { showToast("Rust pixel surface not ready", "warn"); return true; }
    const docId = engine.getId();
    // Fire-and-forget keeps the Alt+Del handler synchronous so it can
    // requestRender immediately; the canonical write + cache sync complete async.
    void (async () => {
      // Hoisted out of the try: the adjustment clear below mutates the document
      // BEFORE the write, so the catch must be able to record a history entry
      // even when the write (or anything after it) rejects.
      let preSnapshot: DocumentModel | undefined;
      let mutationLanded = false;
      let writeLanded = false;
      let imperative: HistoryTilePatches | undefined;
      try {
        const { invoke } = await import("@tauri-apps/api/core");
        const { pixelInvoke } = await import("@/lib/protocol/pixelInvokeCensus");
        // C5.4 ensure-if-absent: a Fill Layer can be the FIRST raster op on a
        // layer, so seed the canonical store from the current derived pixels when
        // Rust has no entry yet (mirrors the brush/bucket; never overwrites).
        let layerReady = true;
        try {
          await invoke("rust_pixels_get_epoch", { docId, layerId: activeId });
        } catch {
          layerReady = false;
        }
        if (!layerReady) {
          const seedData = surface.context.getImageData(0, 0, layer.width, layer.height).data;
          await invoke("rust_pixels_init", {
            docId,
            layerId: activeId,
            width: layer.width,
            height: layer.height,
            bytes: new Uint8Array(seedData.buffer, seedData.byteOffset, seedData.byteLength),
          });
        }
        // Ensure the derived surface reflects the CURRENT canonical state before
        // we overlay the fill (mirrors the brush/bucket pre-commit rehydration).
        await rehydratePaintSurfaceFromRust(docId, activeId, surface);
        // Source current pixels from Rust so OVERLAPPING fills read the post-prior-fill
        // canonical buffer (not a stale TS bitmap).
        const tiles = (await invoke("rust_pixels_snapshot_layer", { docId, layerId: activeId })) as
          { x: number; y: number; w: number; h: number; data: number[] }[];
        const before = reconstructLayerBuffer(tiles, layer.width, layer.height);
        // Build the filled result, preserving outside-selection pixels from `before`.
        const existing = await createImageBitmap(new ImageData(before as Uint8ClampedArray<ArrayBuffer>, layer.width, layer.height));
        const offscreen = buildFilledCanvas(layer, fillColor, sel, existing);
        if (!offscreen) return;
        const after = (offscreen.getContext("2d") as CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D)
          .getImageData(0, 0, layer.width, layer.height).data as Uint8ClampedArray;
        const changed = computeChangedRegion(before, after, layer.width, layer.height);
        if (!changed) return;
        // The byte-diff oracle stays the authority for WHAT changed; the producer
        // owns the region arithmetic. applyFillToContext only paints inside the
        // same rounded selection rect selectionUploadRect returns, so for a
        // non-inverted selection that rect contains the changed box and the
        // intersection below is that same box - the shipped rgba therefore
        // always matches the region (rust_pixels_write_region rejects any
        // length mismatch). Inverted/absent selections pass null.
        const selRect = selectionUploadRect(engine);
        const region = computeDirtyRegion(changed, selRect
          ? { x: selRect.x, y: selRect.y, w: selRect.width, h: selRect.height }
          : null);
        if (!region) return;
        // Capture pre-fill state BEFORE clearing the adjustment so undo restores it.
        preSnapshot = engine.snapshot();
        // Host pixel composite: this metadata clear rides the SAME single history
        // entry as the Rust pixel write below, so it stays on the host path. A
        // routed SetAdjustment clear would add a separate native entry and split
        // the fill gesture's undo (pixel composites for fill/bake stay host-side
        // until pixel authority).
        if (layer.basicAdjustment) {
          engine.clearBasicAdjustments(activeId);
          mutationLanded = true;
        }
        const res = (await pixelInvoke("rust_pixels_write_region", {
          docId,
          layerId: activeId,
          x: region.x,
          y: region.y,
          w: region.w,
          h: region.h,
          rgba: new Uint8Array(changed.rgba.buffer, changed.rgba.byteOffset, changed.rgba.byteLength),
        })) as {
          before: { x: number; y: number; w: number; h: number; data: number[] }[];
          after: { x: number; y: number; w: number; h: number; data: number[] }[];
          epoch: number;
          version: number;
        };
        writeLanded = true;
        // Imperative is entry-owned (tile-memento model): history stores it and
        // replays its before/after tiles on undo/redo; the Rust entry is synced
        // via `rust_pixels_undo` (single step, no second TS-visible entry).
        // Built straight off the write result so a throw further down still
        // leaves the catch with a complete memento to record.
        // `rustOwned` marks it a cursor token for a step Rust already holds, so
        // the undo/redo dispatch takes the pixels from Rust and drains this twin
        // in lockstep instead of replaying tiles no store holds.
        imperative = {
          layerId: activeId,
          surfaceWidth: layer.width,
          surfaceHeight: layer.height,
          before: res.before.map((t) => ({ x: t.x, y: t.y, width: t.w, height: t.h, data: new Uint8ClampedArray(t.data) })),
          after: res.after.map((t) => ({ x: t.x, y: t.y, width: t.w, height: t.h, data: new Uint8ClampedArray(t.data) })),
          rustOwned: true,
        };
        // TS derived cache updated from Rust's authoritative returned `after` tiles + epoch.
        applyRustTilesToSurface(surface.context, res.after);
        surface.pixelEpoch = res.epoch;
        surface.pixelVersion = res.version;
        syncFacadeVersionFromPixel(docId, res.version);
        renderer?.uploadSurfaceTiles?.(activeId, layer.width, layer.height, res.after.map(t => ({ x: t.x, y: t.y, width: t.w, height: t.h, data: new Uint8ClampedArray(t.data) })));
        // The drawn layer comes from layer.imageBitmap, not from Rust, so rebuild
        // that raster from the surface the canonical tiles were just applied to.
        // Otherwise the fill is recorded correctly but stays invisible until some
        // later consumer runs ensureBitmapCurrent. Installing bitmapEpoch is part
        // of this step and only valid once the bitmap really changed - setting it
        // against an unchanged bitmap permanently pinned the pre-fill raster.
        await projectRustPixelsToVisibleSurface(engine, renderer, activeId, surface, res.epoch);
        history.commit(preSnapshot!, "Fill Layer", imperative, true);
      } catch (err) {
        showToast(`Fill Layer failed: ${ipcErrorMessage(err)}`, "error");
        // The adjustment clear above already mutated the document. When the write
        // never landed there is no Rust entry for that mutation, so record a
        // metadata entry (no imperative) instead of leaving it unundoable. A
        // landed write already owns its Rust entry, so it is flagged as such.
        if (mutationLanded && preSnapshot) {
          history.commit(preSnapshot, "Fill Layer", writeLanded ? imperative : undefined, writeLanded);
        }
      }
    })();
    return true;
  }

  // ── Legacy arm (TS-authoritative bitmap) ──
  // UNREACHABLE now that the Rust arm above is unconditional: `arm` is "rust"
  // whenever a paint surface exists and "blocked" when it does not, so
  // resolveRustPixelOperationArm never returns "legacy" for this call site.
  // Kept verbatim pending the retirement phase, which deletes it together with
  // the "legacy" member of RustPixelOperationArm.
  let bitmap: ImageBitmap | null = null;
  try {
    const offscreen = buildFilledCanvas(layer, fillColor, sel, engine.getLayerImageBitmap(activeId));
    if (!offscreen) return false;
    bitmap = (offscreen as OffscreenCanvas).transferToImageBitmap();
  } catch (err: unknown) {
    if (import.meta.env.DEV) console.error("Failed to fill layer with color:", err);
    return false;
  }
  if (!bitmap) return false;

  // Capture the pre-bake state so the fill's undo checkpoint restores to the
  // adjustment-still-applied state (not pre-adjustment) — keeping the
  // adjustment independently undoable from the fill.
  const preFillSnapshot = engine.snapshot();

  // Same rationale as the Rust fill path above: the clear rides this fill's
  // single history entry and its host-side bitmap replacement, so it is not
  // routed to the native SetAdjustment arm.
  if (layer.basicAdjustment) engine.clearBasicAdjustments(activeId);

  // Commit pre-action snapshot BEFORE mutating so the fill is undoable/redoable.
  history.commit(preFillSnapshot, "Fill Layer");
  engine.setLayerImageBitmap(activeId, bitmap);
  renderer.uploadImage(activeId, bitmap);
  return true;
}

/** Build an OffscreenCanvas (or HTMLCanvas fallback) with the solid fill applied,
 *  preserving existing pixels outside the (optional) selection. Shared by the
 *  legacy and C5.4 Rust-canonical paths so fill semantics stay identical. */
function buildFilledCanvas(
  layer: LayerNode,
  fillColor: string,
  sel: SelectionState | null,
  existing: ImageBitmap | null,
): OffscreenCanvas | HTMLCanvasElement | null {
  const w = layer.width, h = layer.height;
  const offscreen = typeof OffscreenCanvas !== "undefined"
    ? new OffscreenCanvas(w, h)
    : (() => {
        const el = document.createElement("canvas");
        el.width = w;
        el.height = h;
        return el;
      })();
  const ctx = offscreen.getContext("2d") as CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D | null;
  if (!ctx) return null;
  if (existing) ctx.drawImage(existing, 0, 0);
  applyFillToContext(ctx, layer, fillColor, sel);
  return offscreen;
}

/** Apply the solid fill (whole-layer or selection-scoped) onto an already-drawn
 *  2D context. Extracted from `fillActiveLayerWithColor` so the legacy and
 *  Rust-canonical paths share identical fill semantics. */
function applyFillToContext(
  ctx: CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D,
  layer: LayerNode,
  fillColor: string,
  sel: SelectionState | null,
): void {
  const w = layer.width, h = layer.height;
  ctx.fillStyle = fillColor;
  if (sel) {
    // Selection is in document space; map it into layer-local pixel space so the
    // fill lands under the marquee even after the layer is resized/translated/rotated.
    const aabb = SelectionOperations.selectionToLayerAabb(sel, layer.transform, w, h);
    const sx = Math.round(aabb.x);
    const sy = Math.round(aabb.y);
    const sw = Math.max(0, Math.round(aabb.width));
    const sh = Math.max(0, Math.round(aabb.height));
    if (sel.inverted) {
      if (sel.shape === "ellipse") {
        // Inverted ellipse: fill everything EXCEPT the ellipse interior.
        const img = ctx.getImageData(0, 0, w, h);
        const hex = fillColor.replace("#", "");
        const r = parseInt(hex.slice(0, 2), 16);
        const g = parseInt(hex.slice(2, 4), 16);
        const b = parseInt(hex.slice(4, 6), 16);
        const localSel: SelectionState = { x: aabb.x, y: aabb.y, width: aabb.width, height: aabb.height, angle: 0, shape: "ellipse" };
        for (let py = 0; py < h; py++) {
          for (let px = 0; px < w; px++) {
            if (!SelectionOperations.isInsideEllipse(px, py, localSel)) {
              const idx = (py * w + px) * 4;
              img.data[idx] = r;
              img.data[idx + 1] = g;
              img.data[idx + 2] = b;
              img.data[idx + 3] = 255;
            }
          }
        }
        ctx.putImageData(img, 0, 0);
      } else {
        // Fill everything EXCEPT the (clamped) selected rect.
        const left = Math.max(0, Math.min(w, sx));
        const top = Math.max(0, Math.min(h, sy));
        const right = Math.max(0, Math.min(w, sx + sw));
        const bottom = Math.max(0, Math.min(h, sy + sh));
        ctx.fillRect(0, 0, w, top);
        ctx.fillRect(0, bottom, w, h - bottom);
        ctx.fillRect(0, top, left, bottom - top);
        ctx.fillRect(right, top, w - right, bottom - top);
      }
    } else {
      if (sel.shape === "ellipse") {
        // Non-inverted ellipse: fill only pixels INSIDE the ellipse.
        const img = ctx.getImageData(0, 0, w, h);
        const hex = fillColor.replace("#", "");
        const r = parseInt(hex.slice(0, 2), 16);
        const g = parseInt(hex.slice(2, 4), 16);
        const b = parseInt(hex.slice(4, 6), 16);
        const localSel: SelectionState = { x: aabb.x, y: aabb.y, width: aabb.width, height: aabb.height, angle: 0, shape: "ellipse" };
        for (let py = sy; py < sy + sh; py++) {
          for (let px = sx; px < sx + sw; px++) {
            if (SelectionOperations.isInsideEllipse(px, py, localSel)) {
              const idx = (py * w + px) * 4;
              img.data[idx] = r;
              img.data[idx + 1] = g;
              img.data[idx + 2] = b;
              img.data[idx + 3] = 255;
            }
          }
        }
        ctx.putImageData(img, 0, 0);
      } else {
        ctx.fillRect(sx, sy, sw, sh);
      }
    }
  } else {
    ctx.fillRect(0, 0, w, h);
  }
}
