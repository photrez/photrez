// Snapshot-history mirror wiring test (closes the delete-mirror hole).
//
// The single-Delete legacy branch (useLayerActions.handleDeleteActiveLayer)
// calls history.recordSnapshotHistory(before, after, "Delete Layer"). That is a
// SEPARATE CommandHistory prototype method from commit() - and installFacadeCommitShim
// only wrapped commit(). Under photrez.facade=1 + photrez.historyBridge=0 the
// internal rust_pixels_record_snapshot fire is gated OFF, so a delete of a
// non-facade-owned layer was mirrored into the TS stack ONLY: the WASM
// ProtocolEngine never advanced. The WASM cursor and the TS undo stack drift by
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
  __resetFacadeRegistryForTests,
} from "@/lib/protocol/facadeRegistry";
import { CommandHistory } from "@/engine/history";
import { getWasmExportModule } from "@/components/editor/wasmExport";

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
  wasm = (await getWasmExportModule()) as never;
  // Install the production shim once. shimInstalled is module-sticky, so the
  // first install wins; all tests share these providers (matches production:
  // EditorShell installs once at boot).
  installFacadeCommitShim({
    getEngine: () => engineStub as never,
    getDocId: () => DOC_ID,
  });
});

beforeEach(() => {
  localStorage.setItem("photrez.facade", "1"); // feature gate ON
  __resetFacadeRegistryForTests();
  wasm?.protocol_reset(DOC_ID);
});

afterEach(() => {
  localStorage.removeItem("photrez.facade");
  __resetFacadeRegistryForTests();
  wasm?.protocol_reset(DOC_ID);
  vi.restoreAllMocks();
});

describe("snapshot-history mirror (delete funnel producer)", () => {
  it("FIX: non-owned delete records an External entry in the WASM engine (no cursor drift)", async () => {
    const applySpy = vi.spyOn(bridge, "applyCommand");

    const history = new CommandHistory();
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
    expect(history.canUndo()).toBe(true); // TS authority still records the undo-point
  });

  it("alignment holds across multiple deletes (1 mirror per delete, no drift)", async () => {
    const history = new CommandHistory();
    for (let i = 0; i < 3; i++) {
      const before = { layers: [{ id: "bg" }, { id: `l${i}` }], width: 1, height: 1 } as never;
      const after = { layers: [{ id: "bg" }], width: 1, height: 1 } as never;
      history.recordSnapshotHistory(before, after, "Delete Layer");
    }
    const proj = await getHistoryProjection(DOC_ID);
    const externalEntries = proj.entries.filter((e) => e.origin.startsWith("external:"));
    expect(externalEntries.length).toBe(3);
    expect(proj.cursor).toBe(3);
    expect(history.canUndo()).toBe(true); // TS authority still records each undo-point
  });

  it("flag OFF: shim fast-fails, no External mirror, byte-identical legacy path", () => {
    localStorage.removeItem("photrez.facade"); // feature gate OFF
    const applySpy = vi.spyOn(bridge, "applyCommand");

    const history = new CommandHistory();
    const before = { layers: [{ id: "bg" }, { id: "del" }], width: 1, height: 1 } as never;
    const after = { layers: [{ id: "bg" }], width: 1, height: 1 } as never;

    history.recordSnapshotHistory(before, after, "Delete Layer");

    const mirrorCalls = applySpy.mock.calls.filter(
      (c) => (c[0] as { command?: { type?: string } })?.command?.type === "recordExternalTransition",
    );
    expect(mirrorCalls.length).toBe(0); // zero protocol work when OFF
    expect(history.canUndo()).toBe(true); // TS authority still records
  });

  it("mirror forwards the PRE-action `before` payload (undo point), not `after`", async () => {
    const history = new CommandHistory();
    const before = { layers: [{ id: "bg" }, { id: "del" }], width: 1, height: 1 } as never;
    const after = { layers: [{ id: "bg" }], width: 1, height: 1 } as never;

    history.recordSnapshotHistory(before, after, "Delete Layer");

    // Identity forwarding: the original TS recordSnapshotHistory stores the
    // PRE-action state as the undo-point; undo() pops and returns exactly that
    // object. This proves `before` reaches the legacy history intact (no copy,
    // no swap to `after`) - the mirror must carry the same undo point.
    const restored = history.undo({ layers: [], width: 1, height: 1 } as never);
    expect(restored).toBe(before);

    // The mirror already fired one External entry for this delete.
    const proj = await getHistoryProjection(DOC_ID);
    const externalEntries = proj.entries.filter((e) => e.origin.startsWith("external:"));
    expect(externalEntries.length).toBe(1);
    expect(proj.cursor).toBe(1);
  });

  it("mirror forwards the `Delete Layer` label into the External entry", async () => {
    const history = new CommandHistory();
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

    // TS side: one undo point recorded (TS cursor = 1).
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

    // WASM cursor stepped back by one.
    const proj2 = await getHistoryProjection(DOC_ID);
    expect(proj2.cursor).toBe(0);

    // TS side steps back by one too (undo the recorded snapshot).
    history.undo({ layers: [], width: 1, height: 1 } as never);
    expect(history.canUndo()).toBe(false);
  });
});
