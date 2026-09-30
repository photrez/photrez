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
  const key = docId === "" ? "default" : docId;
  bgFlagCommitByDoc.delete(key);
  delete openBaselineByDoc[key];
}

/** Test-only reset of the baseline record, so each case starts clean. */
export function __clearOpenBaselineForTests(docId: string): void {
  delete openBaselineByDoc[docId === "" ? "default" : docId];
}

/**
 * Did this document actually get a document-open baseline history entry?
 *
 * Set only when `commitFacadeBackgroundFlag` reports "applied" - i.e. the
 * factory's own SetBackgroundFlag apply landed in Rust's stream as history
 * position 1. That entry is not a user edit, so the undo cursor must not step
 * past it.
 *
 * It is NOT recorded when the route is off, when authority is not native, or
 * when the apply rejected and the factory fell back to the direct setter: in
 * those cases no Rust entry exists, so position 1 (if anything) IS user work.
 * The undo dispatch reads this to know whether its "nothing left to undo"
 * floor is 1 or 0 - assuming a floor of 1 unconditionally would refuse a
 * legitimate undo on any document that never had one.
 */
const openBaselineByDoc: Record<string, true> = Object.create(null);

export function hasOpenBaselineEntry(docId: string): boolean {
  return openBaselineByDoc[docId === "" ? "default" : docId] === true;
}

/**
 * Record that this document's open-time baseline entry exists.
 *
 * Called by `commitFacadeBackgroundFlag` when the factory's apply landed. A
 * test that drives the undo dispatch directly calls this too, because the gate
 * asks "does this doc HAVE a baseline" rather than assuming one - so a test
 * that does NOT call it exercises the "no baseline, floor is 0" branch, which
 * is the one that must never refuse a legitimate undo.
 */
export function recordOpenBaselineEntry(docId: string): void {
  openBaselineByDoc[docId === "" ? "default" : docId] = true;
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
  // This apply landed, so the document now HAS a history entry at position 1
  // that the user never made. Recording it is what lets the undo dispatch
  // refuse a press that would walk the cursor in front of that entry - without
  // guessing, and without refusing a document that never got one.
  recordOpenBaselineEntry(f.docId);
  return { status: "applied", count: ids.length };
}
