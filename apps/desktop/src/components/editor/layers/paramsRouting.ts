// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Committing a text-params edit (font/size/color/box in the properties panel, and
// the text overlay's session-close flush) while the native arm owns the layer.
// One SetLayerParams command carries the whole payload and the native engine
// records the history entry, so the routed path writes no TS history entry: a TS
// entry here would strand an undo point whose engine.restore() rejects with
// E_FACADE_OWNED.
//
// The local re-raster runs only from the value the projection wrote. A command
// reports success even when the arm restates no layer (an id the engine does not
// hold - the arm no-ops), so the patched fields are compared against the settled
// value; a mismatch is surfaced instead of reporting a no-op as applied, and the
// intended value is never written to the model.
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
}

// Key-order-insensitive equality for the patched fields. Used instead of a JSON
// compare because the native round trip re-serializes objects in its own field
// order, and instead of a full payload compare because a field the wire does not
// carry would read as a mismatch on a layer the engine DOES hold.
function sameValue(a: unknown, b: unknown): boolean {
  if (Object.is(a, b)) return true;
  if (typeof a !== "object" || typeof b !== "object" || a === null || b === null) return false;
  const ka = Object.keys(a as object);
  const kb = Object.keys(b as object);
  if (ka.length !== kb.length) return false;
  return ka.every((k) => sameValue((a as Record<string, unknown>)[k], (b as Record<string, unknown>)[k]));
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
      const held = changedKeys.every((k) => sameValue(settledData[k], next[k]));
      // The settled value is authoritative either way; writing the intended one
      // would move the model while the engine kept the old value.
      engine.updateTextData(layerId, settledData);
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
  const next = { ...layer.textData, ...patch };
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
