// Snapshot-history mirror wiring test (closes the delete-mirror hole).
//
// The single-Delete legacy branch (useLayerActions.handleDeleteActiveLayer)
// calls history.recordSnapshotHistory(before, after, "Delete Layer"). That is a
// SEPARATE CommandHistory prototype method from commit() - and installFacadeCommitShim
// only wrapped commit(). The internal rust_pixels_record_snapshot fire has no
// production caller left, so a delete of a
// non-facade-owned layer was mirrored into the TS stack ONLY: the WASM
// DocumentCore never advanced. The WASM cursor and the TS undo stack drift by
// one per such delete, and once a projection marks layers owned,
// engine.restore() throws E_FACADE_OWNED for the stranded TS undo-point
// (mixedHistory.test.ts:115-121).
//
// FIX: installFacadeCommitShim now also wraps recordSnapshotHistory and mirrors
// it into the WASM engine via the EXISTING recordExternalTransitionFor path
// (metadata-only External marker, same as the commit wrapper). This test fires
// the real production producer (CommandHistory.recordSnapshotHistory, the exact
// line the delete funnel executes) and asserts the WASM engine receives the
// mirror entry and the cursor stays aligned.
//
// FAIL-before / PASS-after: before the shim wrapped recordSnapshotHistory, this
// producer emitted NO External transition - the projection below has zero
// external entries and the cursor drifts. After the fix it records one.

import { describe, it, expect, vi, beforeAll, beforeEach, afterEach } from "vitest";
import * as bridge from "@/lib/protocol/bridge";
import {
  getHistoryProjection,
  installFacadeCommitShim,
  confirmExternalCursor,
  getFacade,
  getExternalRecordSnapshot,
  __resetFacadeRegistryForTests,
} from "@/lib/protocol/facadeRegistry";
import { CommandHistory } from "@/engine/history";
import { armWasmModule } from "@/lib/protocol/__tests__/wasmBridgeArm";

vi.mock("@/lib/protocol/bridge", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/protocol/bridge")>();
  const { makeWasmArmedBridge } = await import("@/lib/protocol/__tests__/wasmBridgeArm");
  return makeWasmArmedBridge(actual);
});

// `resyncDerivedView` imports `getHistoryQuery` from `bridge_emu` DIRECTLY
// (not through `bridge`), so the wasm-armed `bridge` mock above never reaches
// the native-authority resync inside CommandHistory. Route this arm's query to
// the same armed wasm module so a standalone CommandHistory re-syncs the
// mirror stream this suite records (the production path the workspace drives).
vi.mock("@/lib/protocol/bridge_emu", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/protocol/bridge_emu")>();
  const { armWasmModule } = await import("@/lib/protocol/__tests__/wasmBridgeArm");
  const wasm = await armWasmModule();
  return {
    ...actual,
    getHistoryQuery: (async (docId?: string) =>
      JSON.parse(
        wasm.protocol_history_query_json(docId ?? "default"),
      )) as unknown as typeof actual.getHistoryQuery,
  };
});

const DOC_ID = "docSnapshotMirror";

// Stable engine provider for the shim (the shim only reads getId/getLayers;
// the WASM target is chosen by getDocId, which routes to the per-doc engine).
const engineStub = {
  getId: () => DOC_ID,
  getLayers: () => [{ id: "bg" }],
};

// Real wasm module (production boundary). Loaded once; the per-doc engine for
// DOC_ID is created on demand by the protocol bridge.
let wasm: { protocol_reset: (docId: string) => void } | null = null;

beforeAll(async () => {
  wasm = await armWasmModule();
  // Install the production shim once. shimInstalled is module-sticky, so the
  // first install wins; all tests share these providers (matches production:
  // EditorShell installs once at boot).
  installFacadeCommitShim({
    getEngine: () => engineStub as never,
    getDocId: () => DOC_ID,
  });
});

beforeEach(() => {
  __resetFacadeRegistryForTests();
  wasm?.protocol_reset(DOC_ID);
});

afterEach(() => {
  __resetFacadeRegistryForTests();
  wasm?.protocol_reset(DOC_ID);
  vi.restoreAllMocks();
});

describe("snapshot-history mirror (delete funnel producer)", () => {
  it("FIX: non-owned delete records an External entry in the WASM engine (no cursor drift)", async () => {
    const applySpy = vi.spyOn(bridge, "applyCommand");

    const history = new CommandHistory();
    history.attachDocIdGetter(() => DOC_ID);
    const before = { layers: [{ id: "bg" }, { id: "del" }], width: 1, height: 1 } as never;
    const after = { layers: [{ id: "bg" }], width: 1, height: 1 } as never;

    // The exact production call site from useLayerActions.handleDeleteActiveLayer.
    history.recordSnapshotHistory(before, after, "Delete Layer");

    // Wiring proof: the shim fired the External mirror command to the engine.
    const mirrorCalls = applySpy.mock.calls.filter(
      (c) => (c[0] as { command?: { type?: string } })?.command?.type === "recordExternalTransition",
    );
    expect(mirrorCalls.length).toBe(1);

    // Engine actually received the mirror: projection has exactly one external
    // entry and the cursor advanced by one - TS undo stack (1) aligns with WASM.
    const proj = await getHistoryProjection(DOC_ID);
    const externalEntries = proj.entries.filter((e) => e.origin.startsWith("external:"));
    expect(externalEntries.length).toBe(1);
    expect(proj.cursor).toBe(1);
    // Authoritative read: re-sync from the mirrored stream, then the native
    // count is live - the mirror advanced the cursor one past the open baseline.
    await history.resyncFromNativeQuery();
    expect(history.canUndo()).toBe(true);
  });

  it("alignment holds across multiple deletes (1 mirror per delete, no drift)", async () => {
    const history = new CommandHistory();
    history.attachDocIdGetter(() => DOC_ID);
    for (let i = 0; i < 3; i++) {
      const before = { layers: [{ id: "bg" }, { id: `l${i}` }], width: 1, height: 1 } as never;
      const after = { layers: [{ id: "bg" }], width: 1, height: 1 } as never;
      history.recordSnapshotHistory(before, after, "Delete Layer");
    }
    const proj = await getHistoryProjection(DOC_ID);
    const externalEntries = proj.entries.filter((e) => e.origin.startsWith("external:"));
    expect(externalEntries.length).toBe(3);
    expect(proj.cursor).toBe(3);
    // Authoritative read: three mirrored deletes live in the native stream, so
    // the cursor sits three past the open baseline - three undo points, not
    // the pre-mirror native default.
    await history.resyncFromNativeQuery();
    expect(history.canUndo()).toBe(true);
  });

  it("mirror forwards the PRE-action `before` payload (undo point), not `after`", async () => {
    const applySpy = vi.spyOn(bridge, "applyCommand");
    const history = new CommandHistory();
    history.attachDocIdGetter(() => DOC_ID);
    const before = { layers: [{ id: "bg" }, { id: "del" }], width: 1, height: 1 } as never;
    const after = { layers: [{ id: "bg" }], width: 1, height: 1 } as never;

    // The exact production call site from useLayerActions.handleDeleteActiveLayer.
    history.recordSnapshotHistory(before, after, "Delete Layer");

    // Identity forwarding: the recordSnapshotHistory shim parks the PRE-action
    // `before` model under the External entry's token (getExternalRecordSnapshot
    // returns the stashed object identity - `asDocumentModel` keeps the same
    // reference for a valid DocumentModel, no copy). This proves `before` reaches
    // the mirror intact, not a copy and not the post-action `after` - and that
    // `recordExternalTransitionFor` (not the host pop, now retired) forwards it.
    const record = applySpy.mock.calls
      .map((c) => c[0] as { command?: { type?: string; token?: string } })
      .find((c) => c?.command?.type === "recordExternalTransition");
    expect(record?.command?.token).toBeTruthy();
    const stashed = getExternalRecordSnapshot(record!.command!.token, "undo");
    expect(stashed).toBe(before);

    // The mirror already fired one External entry for this delete.
    const proj = await getHistoryProjection(DOC_ID);
    const externalEntries = proj.entries.filter((e) => e.origin.startsWith("external:"));
    expect(externalEntries.length).toBe(1);
    expect(proj.cursor).toBe(1);
  });


  it("mirror forwards the `Delete Layer` label into the External entry", async () => {
    const history = new CommandHistory();
    history.attachDocIdGetter(() => DOC_ID);
    const before = { layers: [{ id: "bg" }, { id: "del" }], width: 1, height: 1 } as never;
    const after = { layers: [{ id: "bg" }], width: 1, height: 1 } as never;

    history.recordSnapshotHistory(before, after, "Delete Layer");

    const proj = await getHistoryProjection(DOC_ID);
    const externalEntries = proj.entries.filter((e) => e.origin.startsWith("external:"));
    expect(externalEntries.length).toBe(1);
    expect(externalEntries[0].label).toBe("Delete Layer");
  });

  it("round-trip: mirrored delete lands as External, then undo+confirmExternalCursor clears the barrier and both cursors step back by one", async () => {
    const history = new CommandHistory();
    history.attachDocIdGetter(() => DOC_ID);
    const before = { layers: [{ id: "bg" }, { id: "del" }], width: 1, height: 1 } as never;
    const after = { layers: [{ id: "bg" }], width: 1, height: 1 } as never;

    // Production delete funnel: mirror the delete into the WASM stream.
    history.recordSnapshotHistory(before, after, "Delete Layer");

    // WASM side: one external entry recorded; cursor sits at 1.
    const proj = await getHistoryProjection(DOC_ID);
    const externalEntries = proj.entries.filter((e) => e.origin.startsWith("external:"));
    expect(externalEntries.length).toBe(1);
    const seq = externalEntries[0].seq;
    expect(proj.cursor).toBe(1);

    // Native side has one undo point (cursor sits at 1) - the authoritative
    // read, not the pre-mirror native default.
    await history.resyncFromNativeQuery();
    expect(history.canUndo()).toBe(true);

    // Real facade walk: undo lands on the mirrored external entry, so the facade
    // records a pending-external handoff (the wedge) keyed to that seq (no WASM
    // cursor move yet). This is exactly what the external-cursor handoff drives.
    await getFacade(DOC_ID).undo();
    expect(getFacade(DOC_ID).lastExternalHandoff).toEqual({ seq, direction: "undo" });
    expect(proj.cursor).toBe(1); // barrier does not itself move the cursor

    // External-cursor handoff: confirm the external cursor (undo). The real
    // mirror-plus-handoff interaction this change feeds - it clears the barrier
    // and steps the WASM cursor back by one.
    const res = await confirmExternalCursor(DOC_ID, seq, "undo");
    expect(res.ok).toBe(true);

    // Native cursor stepped back by one.
    const proj2 = await getHistoryProjection(DOC_ID);
    expect(proj2.cursor).toBe(0);

    // The mirror entry has been consumed by the handoff; the native cursor sits
    // back at the open baseline, so no undo point remains.
    await history.resyncFromNativeQuery();
    expect(history.canUndo()).toBe(false);
  });
});
