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
import type { Transform2D } from "@/engine/types";
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
