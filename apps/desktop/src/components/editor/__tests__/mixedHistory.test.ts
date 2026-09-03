// Ticket 2.2 refinement 3: MIXED HISTORY interleaved-sequence test.
//
// Required scenario: legacy op -> facade transform -> legacy op -> undo x3.
// Documents the ACTUAL resulting behavior under the transitional Rust-first
// routing (useEditorCommands.restoreHistorySnapshot) + Gate A guards.
//
// ACTUAL BEHAVIOR VERDICT (asserted below): NOT a true linear user-history.
//
//   1. Legacy op A (pre-facade)          -> TS history [A]
//   2. Facade transform T                -> Rust history [post-add, pre-add];
//      first projection marks EVERY projected layer facade-owned (incl.
//      Background) -> from here on, ALL legacy mutations/undo-restores that
//      touch those ids throw E_FACADE_OWNED.
//   3. Legacy op B                       -> IMPOSSIBLE: engine.transformLayer
//      throws E_FACADE_OWNED before mutating (asserted). B never enters any
//      history.
//   4. undo1 -> Rust reverts T           (facade route)
//      undo2 -> Rust removes the facade-added layer (its own addLayer entry)
//      undo3 -> Rust stack EMPTY: no-op success with EMPTY delta ->
//               production routing falls through to legacy TS history ->
//               engine.restore(pre-A snapshot) THROWS E_FACADE_OWNED because
//               facadeOwnedIds still contains ids (the set is mark-only and
//               accumulates; stale-id cleanup is deferred) -> BLOCKED.
//
//   Net: user-linear order would be B,T,A reversed = T,B,A. Actual drained
//   order = T, (remove layer), then A PINNED forever while owned-ids persist.
//   This is TRANSITIONAL, not the final history architecture. Documented in
//   AI_HISTORY; single-owner stacks remain fully linear (second test).

import { describe, it, expect, beforeEach, afterEach, beforeAll } from "vitest";
import { DocumentEngine, hasFacadeOwnedLayers } from "@/engine/document";
import { CommandHistory } from "@/engine/history";
import { EditorFacade } from "@/lib/protocol/editorFacade";
import { seedFacadeFromEngine, __resetFacadeRegistryForTests } from "@/lib/protocol/facadeRegistry";
import { getWasmExportModule } from "@/components/editor/wasmExport";

// Facade readiness: photrez.facade=1 mixed-history tests must run against the
// REAL Rust engine. Arm the bridge once via the production loader and reset the
// module-lifetime engine between tests.
let wasmModule: { protocol_reset: () => void } | null = null;

beforeAll(async () => {
  const m = await getWasmExportModule();
  wasmModule = m;
});

beforeEach(() => {
  localStorage.setItem("photrez.facade", "1");
});
afterEach(() => {
  localStorage.removeItem("photrez.facade");
  __resetFacadeRegistryForTests();
  wasmModule?.protocol_reset();
});

// Production routing, replicated verbatim from useEditorCommands (the hook
// closure itself is not exported; kept in lockstep — see branch there).
function routeUndo(engine: DocumentEngine, history: CommandHistory, facade: EditorFacade): "facade" | "legacy" | "blocked" {
  if (hasFacadeOwnedLayers()) {
    const snap = facade.undo();
    if (!facade.lastHistoryDeltaWasEmpty) {
      engine.applyFacadeSnapshot(snap as never);
      return "facade";
    }
    // Rust had nothing — fall through to legacy.
  }
  const prev = history.undo(engine.snapshot());
  if (!prev) return "blocked";
  try {
    engine.restore(prev);
    return "legacy";
  } catch {
    return "blocked"; // E_FACADE_OWNED while owned ids exist
  }
}

describe("Ticket 2.2 mixed history: legacy A -> facade T -> legacy B(?) -> undo x3", () => {
  it("documents actual behavior: B blocked at mutation time; undos drain Rust then pin A", () => {
    const engine = new DocumentEngine("docM", "Mixed", 800, 600);
    const history = new CommandHistory();
    const facade = new EditorFacade();

    const bg = engine.addLayer("Background");

    // ── Legacy op A (pre-facade) ──
    history.commit(engine.snapshot(), "A: move bg");
    engine.transformLayer(bg.id, { x: 10 });
    expect(engine.getLayer(bg.id)?.transform.x).toBe(10);

    // ── Facade transform T ──
    seedFacadeFromEngine(engine as never, facade);
    const addSnap = facade.addLayer("Owned") as unknown as { version: number; layers: Array<{ id: string }> };
    engine.applyFacadeSnapshot(addSnap as never);
    const ownedId = addSnap.layers[addSnap.layers.length - 1].id;
    expect(hasFacadeOwnedLayers()).toBe(true);
    facade.beginTransform(ownedId, { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0 });
    facade.updateTransform({ x: 77, y: 5, scaleX: 1, scaleY: 1, rotation: 0 });
    const tSnap = facade.commitTransform();
    engine.applyFacadeSnapshot(tSnap as never);

    // ── Legacy op B: IMPOSSIBLE post-projection ──
    expect(() => engine.transformLayer(bg.id, { x: 20 })).toThrow(/E_FACADE_OWNED/);
    expect(engine.getLayer(bg.id)?.transform.x).toBe(10); // B never happened

    const versionAfterOps = facade.renderedVersion;

    // ── undo x3 ──
    const r1 = routeUndo(engine, history, facade);
    expect(r1).toBe("facade"); // reverts T
    expect(facade.renderedVersion).toBe(versionAfterOps + 1);

    const r2 = routeUndo(engine, history, facade);
    expect(r2).toBe("facade"); // removes the facade-added layer

    const r3 = routeUndo(engine, history, facade);
    expect(r3).toBe("blocked"); // Rust exhausted -> legacy fallback pinned by Gate A

    // A remains STRANDED: bg keeps its post-A position; no TS undo occurred.
    expect(engine.getLayer(bg.id)?.transform.x).toBe(10);
    // Owned-id set persists (mark-only accumulation; stale-id cleanup deferred).
    expect(hasFacadeOwnedLayers()).toBe(true);
  });

  it("single-owner (Rust-only) stack drains fully linearly — sanity contrast", () => {
    const engine = new DocumentEngine("docM2", "M2", 800, 600);
    const history = new CommandHistory();
    const facade = new EditorFacade();

    seedFacadeFromEngine(engine as never, facade);
    const s1 = facade.addLayer("L1") as unknown as { version: number; layers: Array<{ id: string }> };
    engine.applyFacadeSnapshot(s1 as never);
    const id = s1.layers[s1.layers.length - 1].id;

    const gesture = (toX: number) => {
      facade.beginTransform(id, { x: facadeSnapshotX(facade, id) ?? 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0 });
      facade.updateTransform({ x: toX, y: 0, scaleX: 1, scaleY: 1, rotation: 0 });
      engine.applyFacadeSnapshot(facade.commitTransform() as never);
    };
    gesture(10);
    gesture(30);
    expect(engine.getLayer(id)?.transform.x).toBe(30);

    // undo -> x=10 ; undo -> x=0 ; undo -> removes L1 ; undo -> Rust exhausted.
    facade.undo();
    engine.applyFacadeSnapshot(facade.snapshot as never);
    expect(engine.getLayer(id)?.transform.x).toBe(10);

    facade.undo();
    engine.applyFacadeSnapshot(facade.snapshot as never);
    expect(engine.getLayer(id)?.transform.x).toBe(0);

    facade.undo(); // pops pre-add entry -> layer removed
    engine.applyFacadeSnapshot(facade.snapshot as never);
    expect(engine.getLayer(id)).toBeUndefined();

    facade.undo(); // Rust stack now truly empty
    expect(facade.lastHistoryDeltaWasEmpty).toBe(true);
    void history;
  });
});

function facadeSnapshotX(facade: EditorFacade, id: string): number | undefined {
  return facade.snapshot.layers.find((l) => l.id === id)?.x;
}
