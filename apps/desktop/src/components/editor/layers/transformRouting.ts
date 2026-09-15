// Routing seam for COMMITTED numeric layer-transform edits (position, nudge,
// rotation, reset, align, flip). The native TransformLayer command arm owns the
// model field and writes its own history entry, so a routed commit must not also
// mutate the TS engine or write a TS history entry.
//
// The decision is synchronous and the dispatch is async ON PURPOSE. Callers branch
// on `canRouteTransform` / `decideTransformSetRoute` inside the event handler and
// fire-and-forget the dispatcher. Awaiting the decision would push the flag-OFF
// legacy `history.commit` one microtask past the click handler.
//
// Flip has no dedicated funnel: it rides the numeric funnel with a single-key
// patch, expressed as a function of the transform the funnel reads INSIDE its hop
// (`(current) => ({ flipH: !current.flipH })`). Deciding the flag at click time
// would make a second click queued behind the first re-send the same flag, hit the
// unchanged-value guard, and disappear without a trace. The native arm reads the
// flags when the envelope carries them and the snapshot projection writes them
// back, so a routed flip survives the next numeric commit.
//
// Undo granularity differs by design: the native arm labels every entry
// "Transform Layer" (so a routed reset/flip/align loses the legacy label), a
// routed multi-layer commit writes one entry per layer where the legacy path wrote
// a single combined entry, and a routed nudge writes one entry per keypress
// because the native arm has no coalescing (legacy recorded one per key repeat
// burst, on the first press only).

import type { DocumentEngine } from "@/engine/document";
import { isFacadeOwnedLayer } from "@/engine/document";
import type { LayerNode, Transform2D } from "@/engine/types";
import { getLayerAabb } from "@/viewport/transformGeometry";
import {
  MIXED_OWNERSHIP_MESSAGE,
  facadeCommitNumericTransform,
  isFacadeEnabled,
  resolveSelectionRoute,
  type NumericTransformPatch,
} from "@/lib/protocol/facadeRegistry";
import { showToast } from "../Toast";

export type TransformRouteStatus =
  | "applied"
  | "noop"
  | "legacy"
  | "mixed-rejected"
  | "error";

export interface TransformEdit {
  layerId: string;
  // Relative and repeatable callers pass a resolver so each hop adds its step to
  // the transform the PREVIOUS hop projected (see the arrow-key nudge).
  patch: NumericTransformPatch;
}

// A batch member whose patch is a whole computed transform. Callers that can fall
// through to the legacy mutators need this: on a "legacy" return the dispatcher
// writes nothing, so the caller replays the patch itself and a resolver has no
// value to replay from. Assignable to `TransformEdit`.
export interface WholeTransformEdit {
  layerId: string;
  patch: Partial<Transform2D>;
}

// View-side effects the dispatcher runs once a routed commit has landed.
// notifyVisualChange is optional because bare mock workspaces omit it.
export interface TransformRouteRefresh {
  requestRender(): void;
  notifyVisualChange?(): void;
}

export function canRouteTransform(layerId: string): boolean {
  return isFacadeEnabled() && isFacadeOwnedLayer(layerId);
}

// Three-way sync decision for a set: "route" only when the flag is on AND every id
// is facade-owned. "mixed-rejected" must never fall through to the legacy mutators:
// a facade-owned layer rejects a legacy write (E_FACADE_OWNED) after the caller has
// already changed the others.
export function decideTransformSetRoute(layerIds: string[]): "route" | "legacy" | "mixed-rejected" {
  const route = resolveSelectionRoute(layerIds);
  if (route.mode === "mixed-rejected") return "mixed-rejected";
  if (route.mode === "facade" && isFacadeEnabled()) return "route";
  return "legacy";
}

function describeError(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

function refresh(refresh: TransformRouteRefresh): void {
  refresh.requestRender();
  refresh.notifyVisualChange?.();
}

// Overlapping commits are serialized inside `facadeCommitNumericTransform`, which
// owns the facade's single transform slot. A rejection it surfaces (version
// conflict, a live pointer gesture holding the slot, or a layer that disappeared
// while the edit was queued) is a visible failure here.
export async function routeNumericTransform(
  engine: DocumentEngine,
  layerId: string,
  patch: NumericTransformPatch,
  afterCommit: TransformRouteRefresh,
): Promise<TransformRouteStatus> {
  try {
    if (!(await facadeCommitNumericTransform(engine, layerId, patch))) return "noop";
    refresh(afterCommit);
    return "applied";
  } catch (e) {
    showToast(`Cannot apply transform: ${describeError(e)}`, "error");
    return "error";
  }
}

// Whole-set dispatch: a mixed selection is rejected before anything is written, and
// each edit awaits its own command. Every edit is one serialized hop on the same
// document chain, so a concurrent single edit can only land between two members of
// this batch - the batch is ordered, not atomic, which matches the one native
// history entry per layer it produces.
//
// A "legacy" return writes nothing: the caller replays the edits itself, which only
// works for `WholeTransformEdit`. A caller that passes a resolver must have decided
// to route before it gets here.
export async function routeNumericTransformBatch(
  engine: DocumentEngine,
  edits: TransformEdit[],
  afterCommit: TransformRouteRefresh,
): Promise<TransformRouteStatus> {
  const decision = decideTransformSetRoute(edits.map((edit) => edit.layerId));
  if (decision === "mixed-rejected") {
    showToast(MIXED_OWNERSHIP_MESSAGE, "error");
    return "mixed-rejected";
  }
  if (decision === "legacy") return "legacy";

  let applied = 0;
  try {
    for (const edit of edits) {
      if (await facadeCommitNumericTransform(engine, edit.layerId, edit.patch)) {
        applied++;
      }
    }
  } catch (e) {
    if (applied > 0) refresh(afterCommit);
    showToast(`Cannot apply transform: ${describeError(e)}`, "error");
    return "error";
  }
  if (applied === 0) return "noop";
  refresh(afterCommit);
  return "applied";
}

export type AlignMode = "left" | "center-h" | "right" | "top" | "center-v" | "bottom";

// What an align/distribute click would write. `memberCount` counts the selection
// members that survive the guards even when none of them moves; the callers label
// their legacy history entry from it.
export interface TransformEditSet {
  memberCount: number;
  edits: WholeTransformEdit[];
}

// The guards an align/distribute action applies to its selection, unchanged from the
// inline form the panel actions used: a locked, position-locked or background layer
// never moves in either the routed or the legacy path.
function editableTransformMembers(
  engine: DocumentEngine,
  layerIds: string[],
): Array<{ id: string; layer: LayerNode }> {
  return layerIds
    .map((id) => ({ id, layer: engine.getLayer(id) }))
    .filter(
      (item): item is { id: string; layer: LayerNode } =>
        Boolean(item.layer) && !item.layer!.locked && !item.layer!.lockPosition && !item.layer!.isBackground,
    );
}

// Alignment targets are absolute (canvas edges and the canvas center), computed from
// the model before any of them is written, so the whole set can be refused without
// leaving a half-aligned stack.
export function alignTransformEdits(
  engine: DocumentEngine,
  layerIds: string[],
  type: AlignMode,
  docW: number,
  docH: number,
): TransformEditSet {
  const members = editableTransformMembers(engine, layerIds);
  const edits: WholeTransformEdit[] = [];

  for (const { id, layer } of members) {
    const next = { ...layer.transform };
    const layerW = Math.round(layer.width * layer.transform.scaleX);
    const layerH = Math.round(layer.height * layer.transform.scaleY);

    switch (type) {
      case "left":
        next.x = 0;
        break;
      case "center-h":
        next.x = Math.round((docW - layerW) / 2);
        break;
      case "right":
        next.x = docW - layerW;
        break;
      case "top":
        next.y = 0;
        break;
      case "center-v":
        next.y = Math.round((docH - layerH) / 2);
        break;
      case "bottom":
        next.y = docH - layerH;
        break;
    }

    if (next.x !== layer.transform.x || next.y !== layer.transform.y) {
      edits.push({ layerId: id, patch: next });
    }
  }

  return { memberCount: members.length, edits };
}

// Distribution keeps the outermost members of the selection where they are and spreads
// the free space between every pair evenly. Bounds come from the rotated bounding box,
// so a rotated member is spread by the space it visually occupies.
export function distributeTransformEdits(
  engine: DocumentEngine,
  layerIds: string[],
  axis: "h" | "v",
): TransformEditSet {
  const members = editableTransformMembers(engine, layerIds).map((item) => ({
    id: item.id,
    layer: item.layer,
    aabb: getLayerAabb(item.layer.transform, item.layer.width, item.layer.height),
  }));
  const edits: WholeTransformEdit[] = [];
  // Both axis branches divide by (count - 1): fewer than two members has no gap to
  // compute, and every caller treats fewer than three as a no-op anyway.
  if (members.length < 2) return { memberCount: members.length, edits };

  if (axis === "h") {
    members.sort((a, b) => a.aabb.x - b.aabb.x);
    const first = members[0];
    const last = members[members.length - 1];
    const totalSpan = last.aabb.x + last.aabb.width - first.aabb.x;
    const totalLayersWidth = members.reduce((sum, item) => sum + item.aabb.width, 0);
    const gap = (totalSpan - totalLayersWidth) / (members.length - 1);

    let currentX = first.aabb.x;
    for (const item of members) {
      const dx = Math.round(currentX - item.aabb.x);
      if (dx !== 0) {
        edits.push({ layerId: item.id, patch: { ...item.layer.transform, x: item.layer.transform.x + dx } });
      }
      currentX += item.aabb.width + gap;
    }
  } else {
    members.sort((a, b) => a.aabb.y - b.aabb.y);
    const first = members[0];
    const last = members[members.length - 1];
    const totalSpan = last.aabb.y + last.aabb.height - first.aabb.y;
    const totalLayersHeight = members.reduce((sum, item) => sum + item.aabb.height, 0);
    const gap = (totalSpan - totalLayersHeight) / (members.length - 1);

    let currentY = first.aabb.y;
    for (const item of members) {
      const dy = Math.round(currentY - item.aabb.y);
      if (dy !== 0) {
        edits.push({ layerId: item.id, patch: { ...item.layer.transform, y: item.layer.transform.y + dy } });
      }
      currentY += item.aabb.height + gap;
    }
  }

  return { memberCount: members.length, edits };
}
