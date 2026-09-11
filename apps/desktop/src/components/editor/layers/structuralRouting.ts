// Routing seam for the GRAPH half of the five structural ops (duplicate, merge
// down, merge selected, flatten, rasterize). The facade command owns the layer
// graph; pixel compositing stays host-side because the native arm is pixel-neutral.
//
// Each routed op fans out the same way:
//   1. capture the source/victim layers from the live engine model,
//   2. gate on the facade flag plus native authority (under wasm authority the
//      engine only knows facade-created layers, so a merge/flatten would hit a
//      partial set and drop layers - the same danger that gates reorder to native),
//   3. compute the pixel result host-side with the SAME pure helpers the legacy
//      engine uses (compositeTwoLayers / compositeAllLayers / duplicateLayerNode)
//      BEFORE dispatching any command,
//   4. if pixels cannot be built (null) the graph command is never dispatched -
//      no half-mutation, no wasted command,
//   5. dispatch the facade command via the commitFacadeX funnel (ordered
//      restatement: victims removed, merged/clone node appears at the engine
//      position with a null bitmap),
//   6. attach the precomputed pixels via engine.setLayerImageBitmap +
//      renderer.uploadImage, then reproduce the legacy post-selection side effect
//      (active layer selection, flatten background flags) so UI parity holds.
//
// The legacy path is byte-identical: when the funnel returns "legacy" the caller
// runs the original layerOperations helper unchanged.

import { isFacadeOwnedLayer, type DocumentEngine } from "@/engine/document";
import type { WebGL2Backend } from "@/renderer/webgl2";
import { compositeTwoLayers, compositeAllLayers } from "@/engine/layerComposite";
import { duplicateLayerNode } from "@/engine/layerFactory";
import { MAX_LAYERS } from "@/engine/types";
import {
  commitFacadeDuplicate,
  commitFacadeMergeDown,
  commitFacadeMergeSelected,
  commitFacadeFlatten,
  commitFacadeRasterize,
  getFacade,
  seedFacadeFromEngine,
  isFacadeEnabled,
  isNativeAuthority,
  type FacadeRouteStatus,
} from "@/lib/protocol/facadeRegistry";
import { showToast } from "../Toast";

export type StructuralRouteStatus =
  | "applied"
  | "legacy"
  | "mixed-rejected"
  | "error";

function mintLayerId(): string {
  // Host-owned identity: match the TS engine's id space so both engines share it.
  return `layer-${Math.random().toString(36).slice(2, 10)}`;
}

// Seed the facade from the engine exactly once (idempotent: no-op once its
// snapshot carries the layer set). Mirrors the handleAddLayer seeding pattern so
// the routed command's expectedVersion envelope builds against a seeded facade.
async function ensureSeeded(engine: DocumentEngine): Promise<void> {
  const facade = getFacade(engine.getId());
  if (facade.snapshot.layers.length > 0) return;
  try {
    await seedFacadeFromEngine(engine as never, facade);
  } catch {
    // Seeding failure is non-fatal here; the command envelope will surface a
    // version/owned error which the caller toasts.
  }
}

// ── Duplicate ───────────────────────────────────────────────────────────────
// Returns the routed status plus the minted ids so the caller can update
// selection. Per the legacy ts-duplicateLargeLayer cap, the MAX_LAYERS and pixel
// budget guards live TS-side: the Rust Duplicate arm does NOT run can_accept_layer
// (engine.duplicateLayer keeps the same guard for that reason), so if we omit the
// TS guard a facade command could push the stack past MAX_LAYERS silently.
export type DuplicateRouteResult = { status: StructuralRouteStatus; newIds: string[] };

export async function routeDuplicate(
  engine: DocumentEngine,
  _history: unknown,
  renderer: WebGL2Backend,
  ids: string[],
): Promise<DuplicateRouteResult> {
  if (!isFacadeEnabled()) return { status: "legacy", newIds: [] };
  await ensureSeeded(engine);
  // Under wasm authority the native arms are unreachable: every funnel returns
  // "legacy" for each id (the engine only knows facade-created layers, so a
  // routed structural op would hit a partial set). Bail BEFORE the loop so the
  // caller's legacy path runs with zero per-id clone/dispatch work and no
  // half-seeded retention entries.
  if (!isNativeAuthority()) return { status: "legacy", newIds: [] };
  // Atomicity: a mixed selection must not partially duplicate. Callers re-run the
  // legacy path over the FULL selection when the route reports "legacy"; if any
  // selected id is not facade-owned the routed loop would clone the owned ids
  // first, then the legacy re-run would clone them again (double clone). Reject
  // the whole selection up front, mirroring the mixed-ownership rule used by the
  // merge-selected funnel.
  for (const id of ids) {
    if (!isFacadeOwnedLayer(id)) return { status: "legacy", newIds: [] };
  }
  const newIds: string[] = [];
  for (const id of ids) {
    // Routed multi-duplicate records one native history entry per clone; the
    // legacy path recorded a single combined entry, so undo granularity differs
    // by design pending a history-batch follow-up.
    const src = engine.getLayer(id);
    if (!src) continue;
    // TS-side cap guard (keep on the correct side — the Rust arm skips it).
    if (engine.getLayers().length >= MAX_LAYERS) {
      return { status: "error", newIds };
    }
    if (!engine.canAddLayer(src.width, src.height)) {
      return { status: "error", newIds };
    }
    const newId = mintLayerId();
    // Build the full-fidelity clone BEFORE dispatching the graph command.
    // duplicateLayerNode keeps the source type (text/shape params) and its
    // dimensions, and reproduces the source pixels. If no clone bitmap can be
    // produced the command is never dispatched (no half-mutation).
    const cloneNode = duplicateLayerNode(src);
    cloneNode.id = newId;
    const clone = cloneNode.imageBitmap;
    if (!clone) return { status: "error", newIds };
    // Hand the projection a full node to re-use for the id about to arrive:
    // applyFacadeSnapshot's new-id path would otherwise rebuild a metadata-only
    // raster node at the DOCUMENT dimensions, corrupting a text/shape/dims!=doc
    // clone. Name/visible/opacity/transform still come from the arm's
    // restatement; only type/params/dims/baseBitmap come from this seed.
    engine.seedRetainedNodeForProjection(cloneNode);
    let r: { status: FacadeRouteStatus; count?: number };
    try {
      r = await commitFacadeDuplicate(engine as never, id, newId);
    } catch {
      engine.unseedRetainedNodeForProjection(newId);
      return { status: "error", newIds };
    }
    if (r.status !== "applied") {
      // The arrival never happened: drop the pre-seed so a later unrelated
      // re-appearance of this id cannot pick up stale pixels.
      engine.unseedRetainedNodeForProjection(newId);
      // Defensive: with the pre-loop ownership guard in place a "legacy" result
      // is unreachable here by construction. Kept as a safety net so a future
      // funnel change cannot silently drop already-applied clones; the caller's
      // legacy block then runs over the whole selection.
      if (r.status === "legacy") return { status: "legacy", newIds };
      return { status: r.status === "mixed-rejected" ? "mixed-rejected" : "error", newIds };
    }
    // Attach the precomputed pixels only after a successful dispatch. When the
    // projection consumed the pre-seed the clone already carries its bitmap,
    // document dimensions and base bitmap: a text bitmap is rasterized at 2x
    // document space, so calling setLayerImageBitmap here would overwrite
    // width/height with the bitmap dims and blank the base bitmap, rendering the
    // clone at double size. Only ensure its GPU texture exists in that case;
    // otherwise the projection rebuilt the node and needs the pixels attached.
    const projected = engine.getLayer(newId);
    if (projected && projected.imageBitmap === clone) {
      renderer.uploadImage(newId, clone);
    } else {
      engine.setLayerImageBitmap(newId, clone);
      renderer.uploadImage(newId, clone);
    }
    newIds.push(newId);
  }
  return { status: newIds.length > 0 ? "applied" : ids.length === 0 ? "legacy" : "error", newIds };
}

// ── Merge Down ───────────────────────────────────────────────────────────────
export async function routeMergeDown(
  engine: DocumentEngine,
  _history: unknown,
  renderer: WebGL2Backend,
  activeId: string,
): Promise<StructuralRouteStatus> {
  if (!isFacadeEnabled()) return "legacy";
  const layers = engine.getLayers();
  const idx = layers.findIndex((l) => l.id === activeId);
  const bottom = idx >= 0 ? layers[idx + 1] : null;
  if (!bottom) return "legacy";
  const top = layers[idx];
  await ensureSeeded(engine);
  const mergedId = mintLayerId();
  // Composite BEFORE dispatch: if pixels cannot be built the graph command is
  // never dispatched, so a failed composite leaves no half-mutation.
  const composite = compositeTwoLayers(top, bottom, engine.getWidth(), engine.getHeight());
  if (!composite) return "error";
  let r: { status: FacadeRouteStatus; count?: number };
  try {
    r = await commitFacadeMergeDown(engine as never, activeId, mergedId);
  } catch {
    return "error";
  }
  if (r.status === "mixed-rejected") return "mixed-rejected";
  if (r.status === "legacy") return "legacy";
  engine.setLayerImageBitmap(mergedId, composite);
  renderer.uploadImage(mergedId, composite);
  // Mirror legacy mergeDown: the merged node becomes the active layer.
  engine.setActiveLayer(mergedId);
  return "applied";
}

// ── Merge Selected ───────────────────────────────────────────────────────────
export async function routeMergeSelected(
  engine: DocumentEngine,
  _history: unknown,
  renderer: WebGL2Backend,
  ids: string[],
): Promise<StructuralRouteStatus> {
  if (!isFacadeEnabled()) return "legacy";
  const selected = engine.getLayers().filter((l) => ids.includes(l.id));
  if (selected.length < 2) return "legacy";
  await ensureSeeded(engine);
  const mergedId = mintLayerId();
  // Composite BEFORE dispatch (see routeMergeDown).
  const composite = compositeAllLayers(selected, engine.getWidth(), engine.getHeight());
  if (!composite) return "error";
  let r: { status: FacadeRouteStatus; count?: number };
  try {
    r = await commitFacadeMergeSelected(engine as never, ids, mergedId);
  } catch {
    return "error";
  }
  if (r.status === "mixed-rejected") return "mixed-rejected";
  if (r.status === "legacy") return "legacy";
  engine.setLayerImageBitmap(mergedId, composite);
  renderer.uploadImage(mergedId, composite);
  engine.setActiveLayer(mergedId);
  return "applied";
}

// ── Flatten ──────────────────────────────────────────────────────────────────
export async function routeFlatten(
  engine: DocumentEngine,
  _history: unknown,
  renderer: WebGL2Backend,
): Promise<StructuralRouteStatus> {
  if (!isFacadeEnabled()) return "legacy";
  const layers = engine.getLayers();
  if (layers.length <= 1) return "legacy";
  await ensureSeeded(engine);
  const mergedId = mintLayerId();
  // Composite BEFORE dispatch (see routeMergeDown).
  const composite = compositeAllLayers(layers, engine.getWidth(), engine.getHeight());
  if (!composite) return "error";
  let r: { status: FacadeRouteStatus; count?: number };
  try {
    r = await commitFacadeFlatten(engine as never, mergedId);
  } catch {
    return "error";
  }
  if (r.status === "legacy") return "legacy";
  engine.setLayerImageBitmap(mergedId, composite);
  renderer.uploadImage(mergedId, composite);
  // Mirror legacy flattenLayers: the flattened result is the new bottom Background
  // layer, carrying the locks the app's real Background layers carry.
  const merged = engine.getLayer(mergedId);
  if (merged) {
    merged.isBackground = true;
    merged.lockPosition = true;
    merged.lockRotation = true;
    merged.type = "raster";
  }
  engine.setActiveLayer(mergedId);
  return "applied";
}

// ── Rasterize (shape/text -> raster; bitmap already rasterized host-side) ─────
// The native arm is an identity type-flip (drops shape/text params, derives
// name/placement); the pixel rasterization stays host-side. The projected node
// keeps its bitmap (applyFacadeSnapshot preserves it for an existing id), so we
// only flip the type and drop the parametric payloads to match
// shapeLayerToRaster / textLayerToRaster.
export async function routeRasterize(
  engine: DocumentEngine,
  _history: unknown,
  renderer: WebGL2Backend,
  id: string,
): Promise<StructuralRouteStatus> {
  if (!isFacadeEnabled()) return "legacy";
  const layer = engine.getLayer(id);
  if (!layer) return "legacy";
  await ensureSeeded(engine);
  let r: { status: FacadeRouteStatus; count?: number };
  try {
    r = await commitFacadeRasterize(engine as never, id);
  } catch {
    return "error";
  }
  if (r.status === "legacy") return "legacy";
  const l = engine.getLayer(id);
  if (l) {
    l.type = "raster";
    delete l.shapeParams;
    delete l.textData;
  }
  return "applied";
}

// Wire-style helpers for call sites: drive the route, then apply the legacy
// fallback or UI feedback. Keeps the production handlers terse and uniform.
export function finishStructuralRoute(
  status: StructuralRouteStatus,
  onLegacy: () => boolean,
  onApplied: () => void,
  legacyLabel: string,
): boolean {
  if (status === "applied") {
    onApplied();
    return true;
  }
  if (status === "mixed-rejected") {
    showToast(
      "Cannot apply this to a mixed selection: selected layers span both migrated (Rust) and legacy layers. Select one group at a time.",
      "error",
    );
    return false;
  }
  if (status === "legacy") {
    return onLegacy();
  }
  showToast(`Could not ${legacyLabel}`, "warn");
  return false;
}
