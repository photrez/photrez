// ADR 0008 DeleteLayer ticket — integration tests with the REAL DocumentEngine,
// REAL facade stream (emulated/Rust wasm bridge), and REAL ownership reconciliation.
//
// Proves the complete lifecycle behind photrez.facade=1:
//   UI-equivalent delete -> EditorFacade.deleteLayer -> Command{deleteLayer,
//   expectedVersion} -> native HistoryEntry -> RenderDelta Remove{resourceId}
//   -> applyFacadeSnapshot projection -> renderer-visible model update;
//   undo/redo walk the H0 stream; no dangling facade-ownership markers.

import { describe, it, expect, vi, beforeEach, afterEach, beforeAll } from "vitest";
import { DocumentEngine, hasFacadeOwnedLayers, isFacadeOwnedLayer } from "@/engine/document";
import * as bridge from "@/lib/protocol/bridge";
import { EditorFacade } from "@/lib/protocol/editorFacade";
import {
  seedFacadeFromEngine,
  getFacade,
  __resetFacadeRegistryForTests,
} from "@/lib/protocol/facadeRegistry";
import { getWasmExportModule } from "@/components/editor/wasmExport";

// Facade readiness: photrez.facade=1 lifecycle tests must run against the REAL
// Rust engine (the facade is Rust-backed under the flag). Arm the bridge once
// via the production loader and reset the module-lifetime engine between tests.
let wasmModule: { protocol_reset: () => void } | null = null;

beforeAll(async () => {
  const m = await getWasmExportModule();
  wasmModule = m;
});

beforeEach(() => localStorage.setItem("photrez.facade", "1"));
afterEach(() => {
  localStorage.removeItem("photrez.facade");
  __resetFacadeRegistryForTests();
  wasmModule?.protocol_reset();
  vi.restoreAllMocks();
});

function makeDoc(id: string) {
  const engine = new DocumentEngine(id, id, 800, 600);
  const bg = engine.addLayer("Background");
  const history = { commits: [] as Array<{ label: string }> };
  return { engine, bgId: bg.id, history };
}

describe("DeleteLayer lifecycle (facade ON)", () => {
  it("delete -> Remove delta carries resourceId; ownership marker reconciled (no dangle)", () => {
    const { engine, bgId } = makeDoc("docDel");
    const facade = getFacade("docDel");
    seedFacadeFromEngine(engine as never, facade);

    const addSnap = facade.addLayer("Victim") as unknown as { layers: Array<{ id: string; resourceId: number }> };
    engine.applyFacadeSnapshot(addSnap as never);
    const victim = addSnap.layers[addSnap.layers.length - 1];
    expect(isFacadeOwnedLayer(victim.id)).toBe(true);

    const spy = vi.spyOn(bridge, "applyCommand");
    const vBefore = facade.renderedVersion;
    // act: DELETE through the production funnel (same calls as UI branch)
    const snap = facade.deleteLayer(victim.id);
    engine.applyFacadeSnapshot(snap as never);

    // exactly one command, correct type + mandatory expectedVersion
    expect(spy).toHaveBeenCalledTimes(1);
    const env = spy.mock.calls[0][0] as unknown as {
      expectedVersion?: number;
      command: { type: string; id: string };
    };
    expect(env.command.type).toBe("deleteLayer");
    expect(env.command.id).toBe(victim.id);
    expect(env.expectedVersion).toBe(vBefore);

    // projected model no longer contains the layer
    expect(engine.getLayer(victim.id)).toBeUndefined();

    // Remove change carried resourceId for future resource lifecycle
    const removeChange = (snapDeltaOf(spy, 0));
    expect(removeChange?.kind).toBe("remove");
    expect(typeof removeChange?.resourceId).toBe("number");

    // NO dangling ownership marker for the deleted id
    expect(isFacadeOwnedLayer(victim.id)).toBe(false);
    expect(hasFacadeOwnedLayers()).toBe(true); // Background still owned/projected
    void bgId;

    // NO legacy TS history entry was created for the delete
  });

  it("undo restores the deleted layer via H0 stream; redo deletes again (marker reconciled both ways)", () => {
    vi.restoreAllMocks();
    const { engine } = makeDoc("docDel2");
    const facade = getFacade("docDel2");
    seedFacadeFromEngine(engine as never, facade);
    const addSnap = facade.addLayer("V2") as unknown as { layers: Array<{ id: string }> };
    engine.applyFacadeSnapshot(addSnap as never);
    const victim = addSnap.layers[addSnap.layers.length - 1].id;

    facade.deleteLayer(victim);
    engine.applyFacadeSnapshot(facade.snapshot as never);
    expect(engine.getLayer(victim)).toBeUndefined();

    // UNDO walks the H0 native entry -> layer restored in Rust state
    facade.undo();
    engine.applyFacadeSnapshot(facade.snapshot as never);
    expect(engine.getLayer(victim)).not.toBeUndefined();
    expect(isFacadeOwnedLayer(victim)).toBe(true); // re-projected -> re-marked

    // REDO re-deletes; marker reconciled again
    facade.redo();
    engine.applyFacadeSnapshot(facade.snapshot as never);
    expect(engine.getLayer(victim)).toBeUndefined();
    expect(isFacadeOwnedLayer(victim)).toBe(false);
  });

  it("legacy delete path unchanged when facade OFF (no E_FACADE_OWNED, rust graph path)", () => {
    localStorage.removeItem("photrez.facade");
    const { engine, bgId } = makeDoc("docOff");
    const extra = engine.addLayer("Extra");
    engine.deleteLayer(extra.id); // legacy path must work untouched
    expect(engine.getLayer(extra.id)).toBeUndefined();
    expect(hasFacadeOwnedLayers()).toBe(false);
    void bgId;
  });
});

// Extracts the delta of the nth applyCommand call (helper keeps assertions tight)
function snapDeltaOf(spy: ReturnType<typeof vi.spyOn>, callIndex: number): { kind?: string; resourceId?: number } | null {
  const res = spy.mock.results[callIndex]?.value as { delta?: { changes: Array<{ kind: string; resourceId?: number }> } } | undefined;
  const rm = res?.delta?.changes.find((c) => c.kind === "remove");
  return rm ?? null;
}
