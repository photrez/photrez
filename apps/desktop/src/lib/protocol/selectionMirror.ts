// Selection commit mirrors (create / clear / selectAll / invert) plus the shared
// per-document facade store they dispatch through.
//
// Extracted from facadeRegistry.ts to keep that module under the file-size guard.
// The facade store (map + key normalization + getFacade) lives here so this module
// can resolve a facade WITHOUT importing facadeRegistry, which re-exports this
// module's public API - importing it back would create a cycle.
//
// Selection is document-scoped engine-local UI state, NOT layer ownership: the
// native arms emit an EMPTY delta and commit NO history entry, and the
// projection write-back carries only layers + document size (it never writes
// model.selection). The routing convention is therefore INVERTED relative to
// the metadata funnels in facadeRegistry:
//   - The caller ALWAYS runs its host engine selection mutation first; the host
//     stays the visual authority for model.selection.
//   - When the facade flag is on AND native authority is active, the caller
//     ADDITIONALLY dispatches here so the native engine's shadow selection stays
//     in sync (selectAll/invert read the engine's seeded document dims and are
//     needed for parity).
//   - This funnel never calls applyFacadeSnapshot: the returned snapshot is
//     ignored, so it cannot clobber the host selection or drop layers the engine
//     never learned.
// resolveSelectionRoute is deliberately NOT used here (selection is not
// ownership-gated) and there is no mixed-rejected path.
import type { SelectionState } from "./types";
import { EditorFacade } from "./editorFacade";
import { isFacadeEnabled, isNativeAuthority } from "./bridge";

// Projection sink: the TS engine a funnel pushes the facade snapshot into. The
// optional flag tells the engine whether the projection is authoritative for the
// document width/height (see EditorFacade.lastProjectionDimsAuthoritative).
export type FacadeProjectionSink = {
  getId(): string;
  applyFacadeSnapshot(s: unknown, opts?: { dimsAuthoritative?: boolean }): void;
};

// -- Shared per-document facade store -------------------------------------
const facadeByDoc = new Map<string, EditorFacade>();

// Normalize an empty doc id to the reserved "default" key. The native-authority
// engine (bridge/native client) resolves "" -> "default"; on that path the facade
// registry must agree so a facade keyed by the native engine's id meets the bridge.
// On the default (wasm) path this normalization is deliberately NOT applied:
// getFacade/removeFacade use the raw doc id so the wasm default path is
// byte-identical to before the native-authority reroute.
export function resolveFacadeDocKey(docId: string): string {
  return docId === "" ? "default" : docId;
}

// Resolve the facade map key for the active authority. On the native-authority
// path empty ids normalize to "default" (matching the native engine); on the
// default wasm path the raw doc id is used unchanged (byte-identical behavior).
function facadeKey(docId: string): string {
  return isNativeAuthority() ? resolveFacadeDocKey(docId) : docId;
}

export function getFacade(docId: string): EditorFacade {
  const key = facadeKey(docId);
  let f = facadeByDoc.get(key);
  if (!f) {
    f = new EditorFacade(undefined, key);
    facadeByDoc.set(key, f);
  }
  return f;
}

export function peekFacade(docId: string): EditorFacade | undefined {
  return facadeByDoc.get(facadeKey(docId));
}

// Evict a doc's facade (called on document close, alongside clearNativeSeed) so a
// reopened doc id gets a FRESH facade seeded from the freshly-reseeded native
// engine, never a stale one. Gated callers (WorkspaceManager.removeDocument)
// decide when this runs; the registry itself stays authority-agnostic.
export function removeFacade(docId: string): void {
  facadeByDoc.delete(facadeKey(docId));
}

// -- Selection commit mirrors (create / clear / selectAll / invert) --------
export type SelectionRouteStatus = "applied" | "legacy";

export async function commitFacadeSetSelection(
  engine: FacadeProjectionSink,
  selection: SelectionState,
  facadeOverride?: EditorFacade,
): Promise<{ status: SelectionRouteStatus }> {
  if (!isFacadeEnabled() || !isNativeAuthority()) return { status: "legacy" };
  const f = facadeOverride ?? getFacade(engine.getId());
  await f.setSelection(selection);
  return { status: "applied" };
}

export async function commitFacadeClearSelection(
  engine: FacadeProjectionSink,
  facadeOverride?: EditorFacade,
): Promise<{ status: SelectionRouteStatus }> {
  if (!isFacadeEnabled() || !isNativeAuthority()) return { status: "legacy" };
  const f = facadeOverride ?? getFacade(engine.getId());
  await f.clearSelection();
  return { status: "applied" };
}

export async function commitFacadeSelectAll(
  engine: FacadeProjectionSink,
  facadeOverride?: EditorFacade,
): Promise<{ status: SelectionRouteStatus }> {
  if (!isFacadeEnabled() || !isNativeAuthority()) return { status: "legacy" };
  const f = facadeOverride ?? getFacade(engine.getId());
  await f.selectAll();
  return { status: "applied" };
}

export async function commitFacadeInvertSelection(
  engine: FacadeProjectionSink,
  facadeOverride?: EditorFacade,
): Promise<{ status: SelectionRouteStatus }> {
  if (!isFacadeEnabled() || !isNativeAuthority()) return { status: "legacy" };
  const f = facadeOverride ?? getFacade(engine.getId());
  await f.invertSelection();
  return { status: "applied" };
}

// Serialized best-effort dispatch for a selection mirror command. Selection is
// not canonical (the host stays the authority), so a failed shadow sync must
// never surface as an unhandled rejection or affect the host mutation, which
// always runs first. The gate lives here so every call site shares one behavior.
//
// WHY SERIALIZE (per doc): a facade command bumps the facade's renderedVersion
// only in a post-await microtask, so two mirrors dispatched in the same tick
// would both read the SAME expectedVersion; the engine accepts the first and
// rejects the second with E_VERSION_MISMATCH. An absolute op (setSelection)
// would heal at the next absolute op, but the native invert arm TOGGLES the
// shadow's inverted flag, so a lost invert leaves host and shadow permanently
// opposite. Chaining each dispatch onto the previous one for the same doc makes
// the second run start only after the first has bumped the version - no race,
// and an invert applies in order after its preceding setSelection.
//
// The caller still does not await: the return value stays void and each run's
// rejection is swallowed by the chain's .catch, so the UI never blocks and a
// real rejection never propagates.
//
// NOT registered with setExternalTransitionPending: that barrier is awaited by
// syncFromEngine -> flushExternalTransitions, which every selection facade
// method calls. Registering this chain there would make a mirror await the very
// run that is currently executing (self-deadlock). Selection also commits no
// external record, so the E_EXTERNAL_PENDING interplay that barrier protects
// does not apply here. The per-doc tail below already provides the ordering.
const selectionMirrorTailByDoc = new Map<string, Promise<void>>();

export function mirrorSelectionCommand(
  engine: { getId(): string },
  run: () => Promise<unknown>,
): void {
  if (!isFacadeEnabled()) return;
  const key = resolveFacadeDocKey(engine.getId());
  const prev = selectionMirrorTailByDoc.get(key) ?? Promise.resolve();
  const next = prev.then(async () => {
    try {
      await run();
    } catch (err) {
      // Named heal window: a rejected mirror leaves the native shadow behind the
      // host until the next absolute selection op (setSelection / clearSelection
      // / selectAll) re-syncs it. Invert is the one op that cannot self-heal,
      // which is why the serialization above exists.
      console.warn(
        "[selection-mirror] native shadow sync failed (heals on next absolute selection op)",
        err,
      );
    }
  });
  selectionMirrorTailByDoc.set(key, next);
  // Keep the map bounded: drop the entry once this chain has settled, unless a
  // newer dispatch already replaced it. `next` never rejects (the .catch above
  // swallows run()'s rejection), so the cleanup always runs.
  void next.then(() => {
    if (selectionMirrorTailByDoc.get(key) === next) selectionMirrorTailByDoc.delete(key);
  });
}

// Undo/redo shadow sync for a restored host selection. engine.restore() replaces
// model.selection from the popped snapshot, but the native shadow's selection is
// only touched by a selection command, so without this the shadow keeps the
// pre-undo rect and the next native selection op (selectAll / invert read the
// engine's active selection) diverges. Dispatch through the SAME serialized
// funnel the routed selection ops use: a non-null restored selection mirrors
// setSelection, a cleared one mirrors clearSelection. Gated here on facade+native
// before any engine read, so flag OFF is a no-op and the default path stays
// byte-identical (the funnel re-checks the same gate).
//
// The native heal re-push (repushCanonicalDocument) does NOT cover this: it
// refreshes the canonical SHADOW document (which carries selection) but leaves
// the engine's own active selection untouched, and a subsequent native invert
// reads that active selection. So the mirror is required on both the
// handoff-fallthrough and the plain legacy-restore paths.
export function mirrorRestoredSelection(engine: {
  getId(): string;
  getSelection(): SelectionState | null;
}): void {
  // Default-path guard: without this the helper reads engine.getSelection()
  // BEFORE the funnel's own gate, so a flag-OFF undo/redo would still touch the
  // engine (not byte-identical) and any minimal engine fake would break. Only
  // the facade+native path mirrors; the funnel re-checks the same gate.
  if (!isFacadeEnabled() || !isNativeAuthority()) return;
  const sel = engine.getSelection();
  if (sel) {
    mirrorSelectionCommand(engine, () => commitFacadeSetSelection(engine as never, sel));
  } else {
    mirrorSelectionCommand(engine, () => commitFacadeClearSelection(engine as never));
  }
}

// -- Test-only store reset ------------------------------------------------
// The facade map and the mirror tail map both live in this module, so the test
// reset helper is exported here and composed by facadeRegistry's reset.
export function facadeDocIdsForTests(): string[] {
  return [...facadeByDoc.keys()];
}

export function clearFacadeStoreForTests(): void {
  facadeByDoc.clear();
  selectionMirrorTailByDoc.clear();
}
