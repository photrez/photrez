// Guarded commit funnel for the document-level SetBackgroundFlag op: routes a
// layer's background-flag mutation through the protocol facade (one native
// apply per commit) while the per-op photrez.bgFlagRoute guard is armed
// (default OFF), and reports "legacy" otherwise so callers keep the direct
// engine setter byte-for-byte.
//
// This op is document-level single-layer work, not a selection route: the
// factory-created Background layer is never facade-owned, so the funnel skips
// resolveSelectionRoute (same stance as commitFacadeCrop) and gates only on
// the guard readers below. The membership + settle checks mirror
// commitFacadeRename: the native arm accepts an unknown id with an empty
// delta, so a sent flag that never landed must throw instead of passing.

import type { DocumentEngine } from "@/engine/document";
import { getLayerIds, isFacadeEnabled, isNativeAuthority } from "./bridge";
import { EditorFacade } from "./editorFacade";
import { getFacade, seedFacadeFromEngine } from "./facadeRegistry";
import type { FacadeRouteStatus } from "./facadeRegistry";
import type { FacadeProjectionSink } from "./selectionMirror";

const BG_FLAG_ROUTE_KEY = "photrez.bgFlagRoute";

// Per-op migration guard: default OFF. Only an explicit "1" arms routing, so
// a missing or unreadable localStorage keeps the legacy path.
export function isBackgroundFlagRouteEnabled(): boolean {
  try {
    if (typeof localStorage === "undefined") return false;
    return localStorage.getItem(BG_FLAG_ROUTE_KEY) === "1";
  } catch {
    return false;
  }
}

export function isBackgroundFlagRouteArmed(): boolean {
  return isBackgroundFlagRouteEnabled() && isFacadeEnabled() && isNativeAuthority();
}

export async function commitFacadeBackgroundFlag(
  engine: FacadeProjectionSink,
  ids: string[],
  facadeOverride?: EditorFacade,
): Promise<{ status: FacadeRouteStatus; count?: number }> {
  if (ids.length === 0) return { status: "empty" };
  if (!isBackgroundFlagRouteArmed()) return { status: "legacy" };
  const f = facadeOverride ?? getFacade(engine.getId());
  if (f.snapshot.layers.length === 0) {
    try {
      await seedFacadeFromEngine(engine as never, f);
    } catch {
      // Seeding failure is non-fatal here; the command envelope surfaces a
      // version/open error which the caller logs and falls back on.
    }
  }
  let last: unknown = null;
  for (const id of ids) {
    last = await f.setBackgroundFlag(id);
  }
  if (last) engine.applyFacadeSnapshot(last, { dimsAuthoritative: f.lastProjectionDimsAuthoritative });
  // Membership first because a sent flag that already equals the settled
  // value would otherwise pass the settle check below; the native arm has no
  // unknown-id error branch, so this gate is what fails loud.
  const heldBackgroundFlag = new Set(await getLayerIds(f.docId));
  for (const id of ids) {
    if (!heldBackgroundFlag.has(id)) {
      throw new Error(
        `Background-flag commit for layer ${id} did not land; the native engine does not hold this layer`,
      );
    }
  }
  for (const id of ids) {
    const s = f.snapshot.layers.find((l) => l.id === id);
    if (!s || !s.isBackground || !s.lockPosition || !s.lockRotation) {
      throw new Error(
        `Background-flag commit for layer ${id} did not land (isBackground=${s?.isBackground}, lockPosition=${s?.lockPosition}, lockRotation=${s?.lockRotation}); the native engine does not hold this layer`,
      );
    }
  }
  return { status: "applied", count: ids.length };
}
