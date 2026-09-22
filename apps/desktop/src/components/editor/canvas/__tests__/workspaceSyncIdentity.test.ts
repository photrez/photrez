// Identity contract for workspaceSync layer projection.
// Same id sequence (toggles, param edits) reuses previous row objects so the
// layers panel keeps DOM rows and drop math stable; any id-sequence change
// (add/remove/reorder) recreates every row, preserving the recreation the
// reorder drop math relies on.
import { describe, it, expect, vi, afterEach } from "vitest";
import { setupWorkspaceSync } from "../workspaceSync";
import { WorkspaceManager } from "@/engine/workspace";
import type { LayerNode } from "@/engine/types";
import type { ShapeParams } from "@/engine/types";

function stubOffscreenCanvas() {
  const Mock = function (this: any, w: number, h: number) {
    this.width = w;
    this.height = h;
    const ctx = {
      translate: () => {},
      beginPath: () => {},
      rect: () => {},
      roundRect: () => {},
      ellipse: () => {},
      moveTo: () => {},
      lineTo: () => {},
      closePath: () => {},
      fill: () => {},
      stroke: () => {},
      drawImage: () => undefined,
    };
    this.getContext = () => ctx;
    this.transferToImageBitmap = () => ({ width: w, height: h });
  } as any;
  vi.stubGlobal("OffscreenCanvas", Mock);
}

afterEach(() => vi.unstubAllGlobals());

function makeHarness(docId: string) {
  const ws = new WorkspaceManager();
  const session = WorkspaceManager.createBlankDocument(docId, "Ident", 400, 300);
  ws.addDocument(session);
  const seen: LayerNode[][] = [];
  const sync = setupWorkspaceSync({
    workspace: ws,
    camera: { setState: vi.fn() } as never,
    setDocuments: () => {},
    setActiveDocumentId: () => {},
    setLayers: (rows: LayerNode[]) => {
      seen.push(rows);
    },
    setActiveLayerId: () => {},
    setSelectedLayerId: () => {},
    setSelection: () => {},
    setSelectionEditMode: () => {},
    setDocWidth: () => {},
    setDocHeight: () => {},
    setZoom: () => {},
    setPan: () => {},
    scheduler: { requestRender: vi.fn() } as never,
    setHistoryItems: () => {},
    setActiveHistoryIndex: () => {},
  });
  const resync = (): LayerNode[] => {
    sync.syncState();
    return seen[seen.length - 1];
  };
  return { engine: session.engine, resync };
}

const shapeParams: ShapeParams = {
  kind: "rect",
  width: 100,
  height: 50,
  radius: 6,
  fill: { kind: "solid", color: "#E15A17" },
  stroke: { enabled: true, color: "#000000", width: 4 },
  arrowHead: false,
};

describe("workspaceSync layer identity", () => {
  it("toggling one of 32 layers keeps every other row stable and still fires the signal", () => {
    const h = makeHarness("ws-ident-toggle");
    for (let i = 0; i < 31; i++) h.engine.addLayer(`L${i}`, 100, 100);
    const before = h.resync();
    expect(before.length).toBe(32);

    const target = before[5];
    h.engine.setLayerVisibility(target.id, !target.visible);
    const after = h.resync();

    // New array every sync so the signal fires.
    expect(after).not.toBe(before);
    expect(after.length).toBe(32);
    for (let i = 0; i < 32; i++) {
      if (i === 5) continue;
      expect(after[i]).toBe(before[i]);
    }
    expect(after[5]).not.toBe(before[5]);
    expect(after[5].id).toBe(before[5].id);
    expect(after[5].visible).toBe(!before[5].visible);
  });

  it("nested params survive an unrelated toggle, and a nested edit replaces exactly that row", () => {
    stubOffscreenCanvas();
    const h = makeHarness("ws-ident-nested");
    const shape = h.engine.addShapeLayer("Shape 1", shapeParams);
    const raster = h.engine.addLayer("Raster 1", 100, 100);
    const unrelated = h.engine.addLayer("Unrelated", 100, 100);
    const adj = { brightness: 20, contrast: -10, saturation: 5 };
    const liveDesc = (l: LayerNode, extra: Record<string, unknown> = {}) => ({
      id: l.id,
      name: l.name,
      visible: l.visible,
      opacity: l.opacity,
      x: l.transform.x,
      y: l.transform.y,
      scaleX: l.transform.scaleX,
      scaleY: l.transform.scaleY,
      rotation: l.transform.rotation,
      resourceId: 0,
      ...extra,
    });

    // Toggle first: before any routed projection the legacy visibility path
    // owns the layers. Nested rows must keep identity across it.
    const r1 = h.resync();
    const idxOf = (rows: LayerNode[], id: string) => rows.findIndex((r) => r.id === id);
    expect(r1[idxOf(r1, shape.id)].shapeParams).toEqual(shapeParams);

    // Unrelated toggle: nested rows keep identity.
    h.engine.setLayerVisibility(unrelated.id, !unrelated.visible);
    const r2 = h.resync();
    expect(r2).not.toBe(r1);
    expect(r2[idxOf(r2, shape.id)]).toBe(r1[idxOf(r1, shape.id)]);
    expect(r2[idxOf(r2, raster.id)]).toBe(r1[idxOf(r1, raster.id)]);
    expect(r2[idxOf(r2, unrelated.id)]).not.toBe(r1[idxOf(r1, unrelated.id)]);

    // First routed projection normalizes facade-defaulted flags
    // (isBackground, hasAdjustments: undefined -> false) on every row. It
    // also moves the layers under facade ownership, so it runs after the
    // legacy-path toggle above. Resync after it; later steps measure only
    // their own edits against this baseline.
    h.engine.applyFacadeSnapshot({
      version: 1,
      layers: h.engine.getLayers().map((l) => liveDesc(l)),
    } as never);
    const r3base = h.resync();
    expect(r3base[idxOf(r3base, shape.id)].shapeParams).toEqual(shapeParams);

    // Routed adjustment write: exactly the raster row is recreated.
    h.engine.applyFacadeSnapshot({
      version: 2,
      layers: h.engine
        .getLayers()
        .map((l) => liveDesc(l, l.id === raster.id ? { basicAdjustment: adj, hasAdjustments: true } : {})),
    } as never);
    expect(h.engine.getLayer(raster.id)!.basicAdjustment).toEqual(adj);
    const r3 = h.resync();
    expect(r3[idxOf(r3, raster.id)]).not.toBe(r3base[idxOf(r3base, raster.id)]);
    expect(r3[idxOf(r3, raster.id)].basicAdjustment).toEqual(adj);
    expect(r3[idxOf(r3, shape.id)]).toBe(r3base[idxOf(r3base, shape.id)]);
    expect(r3[idxOf(r3, unrelated.id)]).toBe(r3base[idxOf(r3base, unrelated.id)]);

    // Nested shape edit: exactly the shape row is recreated.
    h.engine.updateShapeParams(shape.id, { ...shapeParams, radius: 18 });
    const r4 = h.resync();
    expect(r4[idxOf(r4, shape.id)]).not.toBe(r3[idxOf(r3, shape.id)]);
    expect(r4[idxOf(r4, shape.id)].shapeParams!.radius).toBe(18);
    expect(r4[idxOf(r4, raster.id)]).toBe(r3[idxOf(r3, raster.id)]);
    expect(r4[idxOf(r4, unrelated.id)]).toBe(r3[idxOf(r3, unrelated.id)]);
  });

  it("reordering two layers recreates every row and publishes the new order", () => {
    // Pins preserved behavior: the reorder path keeps recreation (row moves
    // must not serve pre-reorder identities to drop math). Passes both before
    // and after the identity cache; the toggle tests above are the ones that
    // prove the cache.
    const h = makeHarness("ws-ident-reorder");
    h.engine.addLayer("Bottom", 100, 100);
    h.engine.addLayer("Top", 100, 100);
    const r1 = h.resync();
    const names1 = r1.map((r) => r.name);

    h.engine.reorderLayer(0, 1);
    const r2 = h.resync();

    expect(r2).not.toBe(r1);
    expect(r2.map((r) => r.name)).not.toEqual(names1);
    expect(r2.map((r) => r.name).sort()).toEqual(names1.slice().sort());
    for (const row of r2) {
      for (const prev of r1) expect(row).not.toBe(prev);
    }
  });
});
