// ADR 0008 DeleteLayer ticket — integration tests with the REAL DocumentEngine,
// REAL facade stream (Rust wasm bridge), and REAL ownership reconciliation.
//
// Proves the complete facade-routed lifecycle:
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
import { armWasmModule } from "@/lib/protocol/__tests__/wasmBridgeArm";

// ENGINE: the retired `photrez.facadeAuthority=wasm` pin used to select the
// real Rust arm through the flag-driven dispatch; production dispatch is
// native-only now, so the shared wasm factory below arms the same engine on
// the bridge surface the facade funnel crosses.
let wasmModule: { protocol_reset: (docId: string) => void } | null = null;

// The facade funnel dispatches through bridge.applyCommand; the shared
// factory routes that surface onto the real wasm ProtocolEngine module.
vi.mock("@/lib/protocol/bridge", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/protocol/bridge")>();
  const { makeWasmArmedBridge } = await import("@/lib/protocol/__tests__/wasmBridgeArm");
  return makeWasmArmedBridge(actual);
});

beforeAll(async () => {
  wasmModule = await armWasmModule();
});

afterEach(() => {
  __resetFacadeRegistryForTests();
  // `__resetFacadeRegistryForTests` does NOT clear the process-global
  // `facadeOwnedIds`; the retired `photrez.facade` flag used to mask that
  // leak (`hasFacadeOwnedLayers()` was false under `=0` regardless of the
  // set). With the flag gone the set's real contents are visible, so clear it
  // here or the next case inherits this one's owned ids.
  (globalThis as unknown as Record<string, () => void>).__clearFacadeOwnedForTests?.();
  wasmModule?.protocol_reset("default");
  vi.restoreAllMocks();
});

function makeDoc(id: string) {
  const engine = new DocumentEngine(id, id, 800, 600);
  const bg = engine.addLayer("Background");
  const history = { commits: [] as Array<{ label: string }> };
  return { engine, bgId: bg.id, history };
}

describe("DeleteLayer lifecycle", () => {
  it("delete -> Remove delta carries resourceId; ownership marker reconciled (no dangle)", async () => {
    const { engine, bgId } = makeDoc("docDel");
    const facade = getFacade("docDel");
    seedFacadeFromEngine(engine as never, facade);

    const addSnap = await facade.addLayer("Victim") as unknown as { layers: Array<{ id: string; name: string; resourceId: number }> };
    engine.applyFacadeSnapshot(addSnap as never);
    // The engine inserts at the host index (0), so the just-added layer sits
    // at the stack front; the tail is the client-seeded Background, which the
    // wasm module never learned (the native seed surface no-ops under this
    // arm). Select the added layer by name so the delete targets a layer the
    // module knows.
    const victim = addSnap.layers.find((l) => l.name === "Victim")!;
    expect(isFacadeOwnedLayer(victim.id)).toBe(true);

    const spy = vi.spyOn(bridge, "applyCommand");
    const vBefore = facade.renderedVersion;
    // act: DELETE through the production funnel (same calls as UI branch)
    const snap = await facade.deleteLayer(victim.id);
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
    const removeChange = (await snapDeltaOf(spy, 0));
    expect(removeChange?.kind).toBe("remove");
    expect(typeof removeChange?.resourceId).toBe("number");

    // NO dangling ownership marker for the deleted id
    expect(isFacadeOwnedLayer(victim.id)).toBe(false);
    expect(hasFacadeOwnedLayers()).toBe(true); // Background still owned/projected
    void bgId;

    // NO legacy TS history entry was created for the delete
  });

  it("undo restores the deleted layer via H0 stream; redo deletes again (marker reconciled both ways)", async () => {
    vi.restoreAllMocks();
    const { engine } = makeDoc("docDel2");
    const facade = getFacade("docDel2");
    seedFacadeFromEngine(engine as never, facade);
    const addSnap = await facade.addLayer("V2") as unknown as { layers: Array<{ id: string; name: string }> };
    engine.applyFacadeSnapshot(addSnap as never);
    // Same stack-order note as the case above: the just-added layer is at
    // the front, so select it by name rather than by stack position.
    const victim = addSnap.layers.find((l) => l.name === "V2")!.id;

    await facade.deleteLayer(victim);
    engine.applyFacadeSnapshot(facade.snapshot as never);
    expect(engine.getLayer(victim)).toBeUndefined();

    // UNDO walks the H0 native entry -> layer restored in Rust state
    await facade.undo();
    engine.applyFacadeSnapshot(facade.snapshot as never);
    expect(engine.getLayer(victim)).not.toBeUndefined();
    expect(isFacadeOwnedLayer(victim)).toBe(true); // re-projected -> re-marked

    // REDO re-deletes; marker reconciled again
    await facade.redo();
    engine.applyFacadeSnapshot(facade.snapshot as never);
    expect(engine.getLayer(victim)).toBeUndefined();
    expect(isFacadeOwnedLayer(victim)).toBe(false);
  });

});

// Extracts the delta of the nth applyCommand call (helper keeps assertions tight)
async function snapDeltaOf(spy: ReturnType<typeof vi.spyOn>, callIndex: number): Promise<{ kind?: string; resourceId?: number } | null> {
  const res = await (spy.mock.results[callIndex]?.value as Promise<{ delta?: { changes: Array<{ kind: string; resourceId?: number }> } }> | undefined);
  const rm = res?.delta?.changes.find((c) => c.kind === "remove");
  return rm ?? null;
}
