// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Committing a text-params edit (font/size/color/box in the properties panel, and
// the text overlay's session-close flush) while the native arm owns the layer.
// One SetLayerParams command carries the whole payload and the native engine
// records the history entry, so the routed path writes no TS history entry: a TS
// entry here would strand an undo point whose engine.restore() rejects with
// E_FACADE_OWNED.
//
// A command reports success even when the arm restates no layer (an id the engine
// does not hold - the arm no-ops), so the patched fields are compared against the
// settled value. On a match the settled value is authoritative and drives the local
// re-raster; on a mismatch the arm restated nothing, so the user's edit is kept and
// the miss is surfaced out loud instead of reporting a no-op as applied (see
// sameParams for the comparison rules).
//
// A transient caller (the color picker's per-tick preview) passes deps.transient so
// the tick reaches the model + raster only: per-tick commands would all be built
// from the same expectedVersion and the arm would reject every one after the first.
// The transient branch is the same shape as the live-session branch below.
import type { DocumentEngine } from "@/engine/document";
import { isFacadeOwnedLayer } from "@/engine/document";
import type { TextData } from "@/engine/textTypes";
import { commitFacadeParams, isFacadeEnabled } from "@/lib/protocol/facadeRegistry";
import { showToast } from "../Toast";

export interface TextParamsRouterDeps {
  renderer?: { uploadImage: (layerId: string, bitmap: ImageBitmap) => void } | null;
  scheduler?: { requestRender: () => void } | null;
  notifyVisualChange?: () => void;
}

export interface TextDataEditDeps extends TextParamsRouterDeps {
  history?: { commit(snapshot: unknown, label: string): void } | null;
  sessionLayerId?: string | null;
  // Live interaction tick on a layer the native arm owns (the color picker emits one
  // per HSV change). A tick moves the model and its raster and stops there - no
  // command, no history entry. Only the interaction boundary commits, because a
  // command per tick would share one expectedVersion with the tick behind it.
  transient?: boolean;
}

// Field-wise equality between a patch and the value the engine settled. Used
// instead of a JSON compare because the native round trip re-serializes objects in
// its own field order, and instead of a full payload compare because a field the
// wire does not carry would read as a mismatch on a layer the engine DOES hold.
//
// Rules, in the order they are applied:
//  - null and undefined are the same absence (the wire omits a None Option or
//    serializes it as null depending on the field - TextStroke.align comes back as
//    align: null);
//  - keys are the UNION of both sides, so an extra restated field is not a mismatch
//    and a member the other side carries a value for IS one;
//  - nested objects (stroke, fill) recurse instead of comparing by reference;
//  - a non-finite number never compares equal, not even to itself. A settled
//    NaN/Infinity is not a value any renderer can use, so a caller has to surface
//    it; this is also the answer the overlay's box check already gave with `!==`.
export function sameParams(a: unknown, b: unknown): boolean {
  if (a == null || b == null) return a == null && b == null;
  if (typeof a === "number" || typeof b === "number") {
    return typeof a === "number" && typeof b === "number" && Number.isFinite(a) && Number.isFinite(b) && a === b;
  }
  if (a === b) return true;
  if (typeof a !== "object" || typeof b !== "object") return false;
  const keys = new Set([...Object.keys(a as object), ...Object.keys(b as object)]);
  for (const key of keys) {
    if (!sameParams((a as Record<string, unknown>)[key], (b as Record<string, unknown>)[key])) return false;
  }
  return true;
}

// Dispatch one text-params edit when the native arm owns the layer. Returns true
// when it took the edit (the caller must not fall through to the legacy path),
// false when the layer is not facade-owned.
export function commitRoutedTextParams(
  engine: DocumentEngine,
  layerId: string,
  patch: Partial<TextData>,
  deps: TextParamsRouterDeps,
): boolean {
  if (!isFacadeEnabled() || !isFacadeOwnedLayer(layerId)) return false;
  const layer = engine.getLayer(layerId);
  if (!layer || layer.type !== "text" || !layer.textData) return false;
  const next = { ...layer.textData, ...patch };
  const changedKeys = Object.keys(patch) as Array<keyof TextData>;
  void commitFacadeParams(engine, [layerId], { textData: next })
    .then((result) => {
      if (result.status !== "applied") {
        showToast(`Cannot update text (${result.status})`, "error");
        return;
      }
      const settled = engine.getLayer(layerId);
      if (!settled?.textData) return;
      const settledData = settled.textData;
      const held = changedKeys.every((k) => sameParams(settledData[k], next[k]));
      // A mismatch means the arm restated nothing (an id the engine does not hold),
      // so the settled read is the pre-edit value. Keep the user's edit and surface
      // the miss: writing the stale value back loses an edit the UI just reported.
      // On a match the settled value is authoritative - it is what the renderer and
      // the export will use.
      engine.updateTextData(layerId, held ? settledData : next);
      const bitmap = engine.getLayerImageBitmap(layerId);
      if (bitmap) deps.renderer?.uploadImage(layerId, bitmap);
      deps.scheduler?.requestRender();
      deps.notifyVisualChange?.();
      if (!held) {
        showToast("Cannot update text: the native engine does not hold this layer", "error");
      }
    })
    .catch((err) => {
      showToast(`Cannot update text: ${err instanceof Error ? err.message : String(err)}`, "error");
    });
  return true;
}

// Commit one properties-panel text edit: the routed command when the native arm
// owns the layer, the legacy commit-before-mutate path otherwise. A live edit
// session owns the value already and skips the history entry.
export function commitTextParamsEdit(
  engine: DocumentEngine,
  layerId: string,
  patch: Partial<TextData>,
  label: string,
  deps: TextDataEditDeps,
): void {
  const layer = engine.getLayer(layerId);
  if (!layer || layer.locked || layer.type !== "text" || !layer.textData) return;
  const current = layer.textData;
  const next = { ...current, ...patch };
  if (deps.transient) {
    const patched = Object.keys(patch) as Array<keyof TextData>;
    // The picker also emits once at mount and whenever a scrub returns to the
    // starting value: those ticks must not re-raster or dirty the document.
    if (!patched.some((key) => !sameParams(current[key], next[key]))) return;
    engine.updateTextData(layerId, next);
    const transientBitmap = engine.getLayerImageBitmap(layerId);
    if (transientBitmap) deps.renderer?.uploadImage(layerId, transientBitmap);
    deps.scheduler?.requestRender();
    deps.notifyVisualChange?.();
    return;
  }
  if (deps.sessionLayerId === layerId) {
    engine.updateTextData(layerId, next);
    deps.scheduler?.requestRender();
    deps.notifyVisualChange?.();
    return;
  }
  if (commitRoutedTextParams(engine, layerId, patch, deps)) return;
  deps.history?.commit(engine.snapshot(), label);
  engine.updateTextData(layerId, next);
  deps.scheduler?.requestRender();
  deps.notifyVisualChange?.();
}
