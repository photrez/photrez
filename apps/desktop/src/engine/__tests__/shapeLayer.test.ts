import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { WorkspaceManager } from "../workspace";
import type { ShapeParams, LayerNode } from "../types";

const params: ShapeParams = {
  kind: "rect", width: 100, height: 50, radius: 0,
  fill: { kind: "solid", color: "#E15A17" },
  stroke: { enabled: true, color: "#000000", width: 4 },
  arrowHead: false,
};

function makeSession() {
  return WorkspaceManager.createBlankDocument("shape-test", "Shape Test", 400, 300);
}

// jsdom provides no OffscreenCanvas; both the shape rasterizer and
// duplicateLayerNode rely on it. Mirror the shapeRaster.test.ts mock so the
// rasterizer bakes dims and bitmap cloning works in jsdom.
function stubOffscreenCanvas() {
  const Mock = function (this: any, w: number, h: number) {
    this.width = w;
    this.height = h;
    const calls: string[] = [];
    const ctx = {
      translate: () => calls.push("translate"),
      fillStyle: undefined,
      strokeStyle: undefined,
      lineWidth: undefined,
      lineCap: undefined,
      beginPath: () => calls.push("beginPath"),
      rect: () => calls.push("rect"),
      roundRect: () => calls.push("roundRect"),
      ellipse: () => calls.push("ellipse"),
      moveTo: () => calls.push("moveTo"),
      lineTo: () => calls.push("lineTo"),
      closePath: () => calls.push("closePath"),
      fill: () => calls.push("fill"),
      stroke: () => calls.push("stroke"),
      drawImage: () => undefined,
    };
    this.getContext = () => ctx;
    this.transferToImageBitmap = () => ({ width: w, height: h });
  } as any;
  vi.stubGlobal("OffscreenCanvas", Mock);
}

beforeEach(() => stubOffscreenCanvas());
afterEach(() => vi.unstubAllGlobals());

describe("engine shape layer ops", () => {
  it("addShapeLayer creates a shape layer with valid bitmap and params", () => {
    const s = makeSession();
    const layer = s.engine.addShapeLayer("Shape 1", params);
    expect(layer.type).toBe("shape");
    expect(layer.shapeParams).toEqual(params);
    expect(layer.imageBitmap).not.toBeNull();
    // bitmap includes stroke margin: 100+4*2 x 50+4*2
    expect(layer.width).toBe(108);
    expect(layer.height).toBe(58);
    expect(s.engine.getActiveLayerId()).toBe(layer.id);
  });

  it("updateShapeParams re-rasterizes and updates layer size", () => {
    const s = makeSession();
    const layer = s.engine.addShapeLayer("Shape 1", params);
    const newParams = { ...params, width: 200, height: 80, stroke: { enabled: false, color: "#000", width: 4 } };
    s.engine.updateShapeParams(layer.id, newParams);
    const updated = s.engine.getLayer(layer.id)!;
    expect(updated.shapeParams).toEqual(newParams);
    expect(updated.width).toBe(200);
    expect(updated.height).toBe(80);
    expect(updated.imageBitmap).not.toBeNull();
  });

  it("updateShapeParams on non-shape layer is a no-op (does not throw)", () => {
    const s = makeSession();
    s.engine.addLayer("Raster 1", 100, 100);
    const rasterId = s.engine.getActiveLayerId()!;
    expect(() => s.engine.updateShapeParams(rasterId, params)).not.toThrow();
    expect(s.engine.getLayer(rasterId)!.type).toBe("raster");
  });

  it("updateShapeParams with invalid dims (width 0) throws and prior state is intact", () => {
    const s = makeSession();
    const layer = s.engine.addShapeLayer("Shape 1", params);
    expect(() => s.engine.updateShapeParams(layer.id, { ...params, width: 0 })).toThrow();
    const after = s.engine.getLayer(layer.id)!;
    expect(after.shapeParams).toEqual(params);
    expect(after.width).toBe(108);
    expect(after.height).toBe(58);
  });

  it("shapeLayerToRaster drops shapeParams but keeps the bitmap", () => {
    const s = makeSession();
    const layer = s.engine.addShapeLayer("Shape 1", params);
    s.engine.shapeLayerToRaster(layer.id);
    const converted = s.engine.getLayer(layer.id)!;
    expect(converted.type).toBe("raster");
    expect(converted.shapeParams).toBeUndefined();
    expect(converted.imageBitmap).not.toBeNull();
  });

  it("isShapeLayer detects shape layers only", () => {
    const s = makeSession();
    const shape = s.engine.addShapeLayer("S", params);
    const raster = s.engine.addLayer("R", 10, 10);
    expect(s.engine.isShapeLayer(shape.id)).toBe(true);
    expect(s.engine.isShapeLayer(raster.id)).toBe(false);
    expect(s.engine.isShapeLayer("missing")).toBe(false);
  });

  it("duplicateLayerNode copies shapeParams", () => {
    const s = makeSession();
    const layer = s.engine.addShapeLayer("Shape 1", params);
    const dup = s.engine.duplicateLayer(layer.id);
    expect(dup.shapeParams).toEqual(params);
    expect(dup.type).toBe("shape");
  });

  it("snapshot/restore roundtrip preserves shapeParams", () => {
    const s = makeSession();
    const layer = s.engine.addShapeLayer("Shape 1", params);
    s.engine.restore(s.engine.snapshot());
    const after = s.engine.getLayer(layer.id)!;
    expect(after.type).toBe("shape");
    expect(after.shapeParams).toEqual(params);
  });

  it("undo of addShapeLayer removes the shape; redo restores it with params intact", () => {
    const s = makeSession();
    const preAdd = s.engine.snapshot();

    const layer = s.engine.addShapeLayer("Shape 1", params);
    s.history.commit(preAdd, "Add Shape");
    expect(s.engine.isShapeLayer(layer.id)).toBe(true);

    // Undo → layer gone.
    const prev = s.history.undo(s.engine.snapshot());
    s.engine.restore(prev!);
    expect(s.engine.getLayer(layer.id)).toBeUndefined();

    // Redo → layer back with shapeParams (snapshot must carry them).
    const next = s.history.redo(s.engine.snapshot());
    s.engine.restore(next!);
    const restored = s.engine.getLayer(layer.id)!;
    expect(restored.type).toBe("shape");
    expect(restored.shapeParams).toEqual(params);
    expect(restored.imageBitmap).not.toBeNull();
  });
});