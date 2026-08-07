import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { renderShapeToBitmap } from "../shapeRaster";
import type { ShapeParams } from "../types";

function setupOffscreenCanvasMock() {
  const instances: { width: number; height: number; calls: string[]; roundRectRadius: (number | undefined)[] }[] = [];
  const MockOffscreenCanvas = function (this: any, w: number, h: number) {
    this.width = w;
    this.height = h;
    const record = { width: w, height: h, calls: [] as string[], roundRectRadius: [] as (number | undefined)[] };
    instances.push(record);
    const ctx = {
      translate: () => record.calls.push("translate"),
      fillStyle: undefined,
      strokeStyle: undefined,
      lineWidth: undefined,
      lineCap: undefined,
      beginPath: () => record.calls.push("beginPath"),
      rect: () => record.calls.push("rect"),
      roundRect: (_x: number, _y: number, _w: number, _h: number, radius?: number) => {
        record.calls.push("roundRect");
        record.roundRectRadius.push(radius);
      },
      ellipse: () => record.calls.push("ellipse"),
      moveTo: () => record.calls.push("moveTo"),
      lineTo: () => record.calls.push("lineTo"),
      closePath: () => record.calls.push("closePath"),
      fill: () => record.calls.push("fill"),
      stroke: () => record.calls.push("stroke"),
    };
    this.getContext = () => ctx;
    this.transferToImageBitmap = () => ({ width: w, height: h });
  } as any;
  vi.stubGlobal("OffscreenCanvas", MockOffscreenCanvas);
  return instances;
}

const rectParams: ShapeParams = {
  kind: "rect", width: 100, height: 50, radius: 10,
  fill: { kind: "solid", color: "#E15A17" },
  stroke: { enabled: true, color: "#000000", width: 6 },
  arrowHead: false,
};

describe("renderShapeToBitmap", () => {
  let instances: { width: number; height: number; calls: string[]; roundRectRadius: (number | undefined)[] }[];
  beforeEach(() => { instances = setupOffscreenCanvasMock(); });
  afterEach(() => vi.unstubAllGlobals());

  it("bakes bitmap sized (w+strokeWidth) x (h+strokeWidth) so stroke overhang is not clipped", () => {
    const bitmap = renderShapeToBitmap(rectParams) as unknown as { width: number; height: number };
    expect(bitmap.width).toBe(112);
    expect(bitmap.height).toBe(62);
  });

  it("uses roundRect for rect with radius, translate for stroke margin", () => {
    renderShapeToBitmap(rectParams);
    const i = instances[0];
    expect(i.calls).toContain("translate");
    expect(i.calls).toContain("roundRect");
  });

  it("uses ellipse for kind=ellipse", () => {
    renderShapeToBitmap({ ...rectParams, kind: "ellipse" });
    expect(instances[0].calls).toContain("ellipse");
  });

  it("clamps stroke width < 1 to 1", () => {
    const bitmap = renderShapeToBitmap({ ...rectParams, stroke: { enabled: true, color: "#000000", width: 0.4 } }) as unknown as { width: number };
    expect(bitmap.width).toBe(102);
  });

  it("throws on width <= 0 (guard before allocation)", () => {
    expect(() => renderShapeToBitmap({ ...rectParams, width: 0 })).toThrow();
    expect(() => renderShapeToBitmap({ ...rectParams, width: -5 })).toThrow();
    expect(() => renderShapeToBitmap({ ...rectParams, height: 0 })).toThrow();
    expect(() => renderShapeToBitmap({ ...rectParams, kind: "line", width: 0, height: 0 })).toThrow();
    expect(() => renderShapeToBitmap({ ...rectParams, kind: "line", width: 0, height: -3 })).toThrow();
  });

  it("clamps radius to min(w,h)/2", () => {
    const i = setupOffscreenCanvasMock();
    renderShapeToBitmap({ ...rectParams, radius: 9999 });
    expect(i[0].calls).toContain("roundRect");
    expect(i[0].roundRectRadius).toEqual([25]);
  });

  it("no stroke → no stroke() call and bitmap sized w x h", () => {
    const noStroke = { ...rectParams, stroke: { enabled: false, color: "#000000", width: 6 } };
    const bitmap = renderShapeToBitmap(noStroke) as unknown as { width: number; height: number };
    expect(bitmap.width).toBe(100);
    expect(bitmap.height).toBe(50);
    expect(instances[0].calls).not.toContain("stroke");
  });

  it("arrowHead line draws a triangle path at end", () => {
    const line = { ...rectParams, kind: "line" as const, height: 0, arrowHead: true,
      fill: { kind: "none" as const, color: "#000000" } };
    const bitmap = renderShapeToBitmap(line) as unknown as { width: number };
    expect(bitmap.width).toBe(136);
    expect(instances[0].calls).toContain("moveTo");
    expect(instances[0].calls).toContain("lineTo");
  });
});
