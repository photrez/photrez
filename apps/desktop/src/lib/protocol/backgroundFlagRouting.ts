// Guarded commit funnel for the document-level SetBackgroundFlag op: routes a
// layer's background-flag mutation through the protocol facade (one native
// apply per commit) while the per-op photrez.bgFlagRoute guard is armed
// (default ON; "0" opts out), and reports "legacy" otherwise so callers keep
// the direct engine setter byte-for-byte.
//
// This op is document-level single-layer work, not a selection route: the
// factory-created Background layer is never facade-owned, so the funnel skips
// resolveSelectionRoute (same stance as commitFacadeCrop) and gates only on
// the guard readers below. The membership + settle checks mirror
// commitFacadeRename: the native arm accepts an unknown id with an empty
// delta, so a sent flag that never landed must throw instead of passing.

import type { DocumentEngine } from "@/engine/document";
import { getLayerIds, isFacadeEnabled, isNativeAuthority, setExternalTransitionPending } from "./bridge";
import { EditorFacade } from "./editorFacade";
import { getFacade, seedFacadeFromEngine } from "./facadeRegistry";
import type { FacadeRouteStatus } from "./facadeRegistry";
import type { FacadeProjectionSink } from "./selectionMirror";

const BG_FLAG_ROUTE_KEY = "photrez.bgFlagRoute";

// Per-op migration guard: DEFAULT ON. An explicit "0" is the rollback switch
// (restores the legacy direct setter byte-for-byte); any other value, a missing
// key, or an unreadable localStorage arms routing.
export function isBackgroundFlagRouteEnabled(): boolean {
  try {
    if (typeof localStorage === "undefined") return true;
    return localStorage.getItem(BG_FLAG_ROUTE_KEY) !== "0";
  } catch {
    return true;
  }
}

export function isBackgroundFlagRouteArmed(): boolean {
  return isBackgroundFlagRouteEnabled() && isFacadeEnabled() && isNativeAuthority();
}

// In-flight factory flag commits per document. The factory fires the funnel
// before it returns the session, so the open path (addDocument) can hold the
// canonical shadow seed behind the commit. The entry is dropped on workspace
// close so a closed engine is not retained by the settled promise's closure.
const bgFlagCommitByDoc = new Map<string, Promise<unknown>>();

export function trackBackgroundFlagCommit(docId: string, commit: Promise<unknown>): void {
  bgFlagCommitByDoc.set(docId === "" ? "default" : docId, commit);
}

// Resolves once the tracked commit for this doc has settled; immediately when
// none was tracked (guard opted out, or a session not created by the factory).
export async function awaitBackgroundFlagCommit(docId: string): Promise<void> {
  await bgFlagCommitByDoc.get(docId === "" ? "default" : docId);
}

export function clearBackgroundFlagCommit(docId: string): void {
  bgFlagCommitByDoc.delete(docId === "" ? "default" : docId);
}

// Optimistic TS-model projection of the background flag, used by the document
// factories at fire time: synchronous readers (property panel, reorder clamp)
// and the canonical payload must see the flag while the native apply is still
// in flight. Model-only by design - no dirty mark, no notify, no native write;
// the single native apply stays inside commitFacadeBackgroundFlag.
export function markBackgroundFlagOnModel(engine: DocumentEngine, layerId: string): void {
  const layer = engine.getLayer(layerId);
  if (!layer) return;
  layer.isBackground = true;
  layer.lockPosition = true;
  layer.lockRotation = true;
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
    const applying = f.setBackgroundFlag(id);
    // Register the in-flight apply on the external-transition barrier BEFORE
    // awaiting, so any other command's syncFromEngine -> flushExternalTransitions
    // reads the version only after this apply's documentVersion bump (closes the
    // read-then-dispatch E_VERSION_MISMATCH interleave). This line runs
    // synchronously right after setBackgroundFlag suspends inside this commit's
    // OWN flush - that flush read the barrier before this entry existed, so the
    // commit can never end up awaiting its own promise.
    setExternalTransitionPending(f.docId, applying.then(() => {}));
    last = await applying;
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
