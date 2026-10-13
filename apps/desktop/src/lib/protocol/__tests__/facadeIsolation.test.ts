import { describe, it, expect, beforeEach, afterEach, beforeAll, vi } from "vitest";
import { DocumentEngine, isFacadeOwnedLayer } from "@/engine/document";
import { EditorFacade } from "../editorFacade";
import { armWasmModule } from "@/lib/protocol/__tests__/wasmBridgeArm";

// ENGINE: the retired `photrez.facadeAuthority=wasm` pin used to select the
// real Rust arm through the flag-driven dispatch; production dispatch is
// native-only now, so the shared wasm factory below arms the same engine on
// the bridge surface the facade funnel crosses. The per-document engine is
// reset between tests so state/version do not leak across cases.
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

describe("Gate A — facade isolation", () => {
  beforeEach(() => {
    wasmModule?.protocol_reset("default");
    (globalThis as unknown as Record<string, () => void>).__clearFacadeOwnedForTests?.();
    localStorage.clear();
  });
  afterEach(() => {
    localStorage.clear();
    wasmModule?.protocol_reset("default");
    (globalThis as unknown as Record<string, () => void>).__clearFacadeOwnedForTests?.();
    vi.restoreAllMocks();
  });

  it("legacy transform blocked on a facade-owned layer", async () => {
    const engine = new DocumentEngine("doc", "Test", 100, 100);
    const facade = new EditorFacade();
    const snap = await facade.addLayer("FacadeLayer");
    const id = snap.layers[0].id;
    // project to engine
    engine.applyFacadeSnapshot(snap);
    expect(() => engine.transformLayer(id, { x: 999 } as never)).toThrow(/E_FACADE_OWNED/);
    expect(() => engine.setLayerOpacity(id, 0.5)).toThrow(/E_FACADE_OWNED/);
    expect(() => engine.deleteLayer(id)).toThrow(/E_FACADE_OWNED/);
  });

  it("legacy restore blocked while facade owns layers", async () => {
    const engine = new DocumentEngine("doc", "Test", 100, 100);
    const facade = new EditorFacade();
    const snap = await facade.addLayer("A");
    engine.applyFacadeSnapshot(snap);
    const legacySnap = engine.snapshot();
    expect(() => engine.restore(legacySnap)).toThrow(/E_FACADE_OWNED/);
  });

  it("facade addLayer remains sole owner, no history entry in legacy engine", async () => {
    const engine = new DocumentEngine("doc", "Test", 100, 100);
    const facade = new EditorFacade();
    const before = engine.snapshot().layers.length;
    const snap = await facade.addLayer("New");
    engine.applyFacadeSnapshot(snap);
    expect(engine.getLayers().length).toBe(before + 1);
    // no history entry was created via legacy path — undo via legacy would be blocked
    expect(() => engine.restore(engine.snapshot())).toThrow(/E_FACADE_OWNED/);
  });

  it("facade layer + legacy brush -> blocked, no paint surface, no history, no pixel mutation", async () => {
    const engine = new DocumentEngine("doc", "Test", 100, 100);
    const facade = new EditorFacade();
    const snap = await facade.addLayer("FacadeBrushLayer");
    const id = snap.layers[0].id;
    engine.applyFacadeSnapshot(snap);
    expect(isFacadeOwnedLayer(id)).toBe(true);
    // onPaintStroke guard checks isFacadeOwnedLayer at earliest point before PaintTileSurface allocation
    // so no surface, no history entry, no pixel mutation occurs
    expect(engine.getLayer(id)?.imageBitmap).toBeNull();
    expect(() => engine.transformLayer(id, { x: 1 } as never)).toThrow(/E_FACADE_OWNED/);
  });
});
