// Single-owner (Rust-only) history stack drains fully linearly through
// the facade funnel. The retired mixed-history interleaved-sequence
// test (legacy op -> facade transform -> legacy op -> undo x3) pinned
// the TRANSITIONAL Ticket 2.2 routing; its "A pinned forever" outcome
// was documented in that suite's own header as non-final, so the
// transitional block is retired and deleted here. The facade funnel
// dispatches through bridge.applyCommand; the shared factory routes
// that surface onto the real wasm ProtocolEngine module.
import { describe, it, expect, afterEach, beforeAll, vi } from "vitest";
import { DocumentEngine } from "@/engine/document";
import { CommandHistory } from "@/engine/history";
import { EditorFacade } from "@/lib/protocol/editorFacade";
import { seedFacadeFromEngine, __resetFacadeRegistryForTests } from "@/lib/protocol/facadeRegistry";
import { armWasmModule } from "@/lib/protocol/__tests__/wasmBridgeArm";

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
  wasmModule?.protocol_reset("default");
});

describe("single-owner (Rust-only) history stack drains fully linearly", () => {
  it("single-owner (Rust-only) stack drains fully linearly — sanity contrast", async () => {
    const engine = new DocumentEngine("docM2", "M2", 800, 600);
    const history = new CommandHistory();
    const facade = new EditorFacade();

    seedFacadeFromEngine(engine as never, facade);
    const s1 = await facade.addLayer("L1") as unknown as { version: number; layers: Array<{ id: string }> };
    engine.applyFacadeSnapshot(s1 as never);
    const id = s1.layers[s1.layers.length - 1].id;

    const gesture = async (toX: number) => {
      facade.beginTransform(id, { x: facadeSnapshotX(facade, id) ?? 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0 });
      facade.updateTransform({ x: toX, y: 0, scaleX: 1, scaleY: 1, rotation: 0 });
      engine.applyFacadeSnapshot(await facade.commitTransform() as never);
    };
    await gesture(10);
    await gesture(30);
    expect(engine.getLayer(id)?.transform.x).toBe(30);

    // undo -> x=10 ; undo -> x=0 ; undo -> removes L1 ; undo -> Rust exhausted.
    await facade.undo();
    engine.applyFacadeSnapshot(facade.snapshot as never);
    expect(engine.getLayer(id)?.transform.x).toBe(10);

    await facade.undo();
    engine.applyFacadeSnapshot(facade.snapshot as never);
    expect(engine.getLayer(id)?.transform.x).toBe(0);

    await facade.undo(); // pops pre-add entry -> layer removed
    engine.applyFacadeSnapshot(facade.snapshot as never);
    expect(engine.getLayer(id)).toBeUndefined();

    await facade.undo(); // Rust stack now truly empty
    expect(facade.lastHistoryDeltaWasEmpty).toBe(true);
    void history;
  });
});

function facadeSnapshotX(facade: EditorFacade, id: string): number | undefined {
  return facade.snapshot.layers.find((l) => l.id === id)?.x;
}
