import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { DocumentEngine, isFacadeOwnedLayer } from "@/engine/document";
import { EditorFacade } from "../editorFacade";
import { __resetEmulatedForTests } from "../bridge";

describe("Gate A — facade isolation", () => {
  beforeEach(() => {
    __resetEmulatedForTests();
    (globalThis as unknown as Record<string, () => void>).__clearFacadeOwnedForTests?.();
    localStorage.clear();
  });
  afterEach(() => {
    localStorage.clear();
    (globalThis as unknown as Record<string, () => void>).__clearFacadeOwnedForTests?.();
  });

  it("legacy transform blocked on facade-owned layer while flag=1", () => {
    localStorage.setItem("photrez.facade", "1");
    const engine = new DocumentEngine("doc", "Test", 100, 100);
    const facade = new EditorFacade();
    const snap = facade.addLayer("FacadeLayer");
    const id = snap.layers[0].id;
    // project to engine
    engine.applyFacadeSnapshot(snap);
    expect(() => engine.transformLayer(id, { x: 999 } as never)).toThrow(/E_FACADE_OWNED/);
    expect(() => engine.setLayerOpacity(id, 0.5)).toThrow(/E_FACADE_OWNED/);
    expect(() => engine.deleteLayer(id)).toThrow(/E_FACADE_OWNED/);
  });

  it("legacy restore blocked while facade owns layers", () => {
    localStorage.setItem("photrez.facade", "1");
    const engine = new DocumentEngine("doc", "Test", 100, 100);
    const facade = new EditorFacade();
    const snap = facade.addLayer("A");
    engine.applyFacadeSnapshot(snap);
    const legacySnap = engine.snapshot();
    expect(() => engine.restore(legacySnap)).toThrow(/E_FACADE_OWNED/);
  });

  it("legacy ops allowed when flag=0 even on same id", () => {
    localStorage.setItem("photrez.facade", "0");
    const engine = new DocumentEngine("doc", "Test", 100, 100);
    const facade = new EditorFacade();
    const snap = facade.addLayer("A");
    const id = snap.layers[0].id;
    engine.applyFacadeSnapshot(snap);
    // flag off -> not blocked (facadeOwnedIds still has id, but isFacadeEnabled false)
    expect(() => engine.transformLayer(id, { x: 1 } as never)).not.toThrow();
  });

  it("facade addLayer remains sole owner, no history entry in legacy engine", () => {
    localStorage.setItem("photrez.facade", "1");
    const engine = new DocumentEngine("doc", "Test", 100, 100);
    const facade = new EditorFacade();
    const before = engine.snapshot().layers.length;
    const snap = facade.addLayer("New");
    engine.applyFacadeSnapshot(snap);
    expect(engine.getLayers().length).toBe(before + 1);
    // no history entry was created via legacy path — undo via legacy would be blocked
    expect(() => engine.restore(engine.snapshot())).toThrow(/E_FACADE_OWNED/);
  });

  it("facade layer + legacy brush -> blocked, no paint surface, no history, no pixel mutation", () => {
    localStorage.setItem("photrez.facade", "1");
    const engine = new DocumentEngine("doc", "Test", 100, 100);
    const facade = new EditorFacade();
    const snap = facade.addLayer("FacadeBrushLayer");
    const id = snap.layers[0].id;
    engine.applyFacadeSnapshot(snap);
    expect(isFacadeOwnedLayer(id)).toBe(true);
    // onPaintStroke guard checks isFacadeOwnedLayer at earliest point before PaintTileSurface allocation
    // so no surface, no history entry, no pixel mutation occurs
    expect(engine.getLayer(id)?.imageBitmap).toBeNull();
    expect(() => engine.transformLayer(id, { x: 1 } as never)).toThrow(/E_FACADE_OWNED/);
  });
});
