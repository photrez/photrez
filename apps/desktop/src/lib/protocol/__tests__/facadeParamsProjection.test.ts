// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Facade projection: textData round trip.
//
// A native SetLayerParams result reaches the TS model as an Upsert whose layer
// carries textData. applyFacadeSnapshot is the only writer the TS model gets for
// that field, so without projecting it a routed params commit leaves the model
// on the pre-edit value while the native engine holds the new one - the next
// restatement of the layer then silently reverts the edit.
//
// The field rides the same serde-Option convention as the flip flags: an arm
// that does not touch params omits it on the wire, so an ABSENT field must leave
// the model value alone. Clearing on absence would drop a text edit as soon as
// any other metadata arm restated the layer.
//
// Layer width/height are NOT part of that convention: for a layer the model
// already has they are model-owned (the pixel path produces them) and
// applyFacadeSnapshot never writes them, so a restatement cannot clobber the size.
//
// shapeParams and layerType (the wire name for the model's `type`) ride that
// same convention. One consequence is pinned below and is NOT a defect this
// projection can fix: the RasterizeLayer arm clears shape_params by setting it
// to None, and a None Option serializes as an absent key
// (crates/core/src/model.rs - skip_serializing_if), so the wire cannot tell
// "cleared" apart from "this arm never mentioned the field". The production
// rasterize route compensates host-side (see routeRasterize in
// components/editor/layers/structuralRouting.ts).

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { DocumentEngine } from "@/engine/document";
import { DEFAULT_TEXT_DATA, type TextData } from "@/engine/textTypes";

// jsdom has no OffscreenCanvas; stub the seam the text rasterizer uses so the
// typed add produces a bitmap instead of falling through to a null 2d context.
function stubOffscreenCanvas(): void {
  const Mock = function (this: any, w: number, h: number) {
    this.width = w;
    this.height = h;
    const ctx: any = {
      font: "",
      fillStyle: "",
      strokeStyle: "",
      lineWidth: 0,
      lineJoin: "miter",
      miterLimit: 10,
      globalAlpha: 1,
      globalCompositeOperation: "source-over",
      textBaseline: "alphabetic",
      letterSpacing: undefined,
      measureText: (s: string) => ({
        width: s.length * 10,
        actualBoundingBoxAscent: 80,
        actualBoundingBoxDescent: 24,
        fontBoundingBoxAscent: 80,
        fontBoundingBoxDescent: 24,
      }),
      fillText: () => {},
      strokeText: () => {},
      drawImage: () => {},
      save: () => {},
      restore: () => {},
      translate: () => {},
      scale: () => {},
      rotate: () => {},
      fillRect: () => {},
    };
    this.getContext = () => ctx;
    this.transferToImageBitmap = () => ({ width: this.width, height: this.height, close: () => {} });
  } as unknown as typeof OffscreenCanvas;
  vi.stubGlobal("OffscreenCanvas", Mock);
}

/**
 * One layer descriptor the way a native restatement serializes it: camelCase,
 * Option fields omitted when unset. `any` on purpose - the descriptor may carry a
 * field the projection signature does not declare yet, and this file is the
 * failing proof that it must.
 */
function restated(layerId: string, over: Record<string, unknown> = {}): any {
  return {
    id: layerId,
    name: "Text",
    visible: true,
    opacity: 1,
    resourceId: 0,
    x: 0,
    y: 0,
    scaleX: 1,
    scaleY: 1,
    rotation: 0,
    ...over,
  };
}

describe("applyFacadeSnapshot textData projection", () => {
  beforeEach(() => {
    stubOffscreenCanvas();
  });
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("writes textData carried by a native SetLayerParams restatement into the model", () => {
    const engine = new DocumentEngine("docParamsPresent", "Doc", 800, 600);
    const layer = engine.addTextLayer("Text", { ...DEFAULT_TEXT_DATA, content: "before" });
    const committed: TextData = {
      ...DEFAULT_TEXT_DATA,
      content: "after",
      boxMode: "area",
      boxWidth: 240,
      boxHeight: 120,
    };

    engine.applyFacadeSnapshot({
      version: 5,
      layers: [restated(layer.id, { textData: committed })],
      width: 800,
      height: 600,
    });

    expect(engine.getLayer(layer.id)!.textData).toEqual(committed);
  });

  it("leaves the model textData alone when the restatement omits it", () => {
    const engine = new DocumentEngine("docParamsAbsent", "Doc", 800, 600);
    const layer = engine.addTextLayer("Text", { ...DEFAULT_TEXT_DATA, content: "keep me" });

    engine.applyFacadeSnapshot({
      version: 2,
      layers: [restated(layer.id)],
      width: 800,
      height: 600,
    });

    expect(engine.getLayer(layer.id)!.textData!.content).toBe("keep me");
  });
});

describe("applyFacadeSnapshot shapeParams + layerType projection", () => {
  const params = {
    kind: "rect" as const,
    width: 100,
    height: 50,
    radius: 8,
    fill: { kind: "solid" as const, color: "#ff0000" },
    stroke: { enabled: true, color: "#00ff00", width: 6 },
    arrowHead: false,
  };

  function shapeModel(): { engine: DocumentEngine; id: string } {
    // The model layer is assembled directly rather than through
    // addShapeLayer: the projection consumes the descriptor, and building the
    // layer without the rasterizer keeps jsdom's missing 2d context out of the
    // test. Same shape the coverage guard in engine/__tests__/document.test.ts
    // uses.
    const engine = new DocumentEngine("docShapeProjection", "Doc", 800, 600);
    const layer = engine.addLayer("S", 100, 100);
    const model = engine.getLayer(layer.id)!;
    model.type = "shape";
    model.shapeParams = { ...params };
    return { engine, id: layer.id };
  }

  it("writes shapeParams + layerType carried by a native restatement into the model", () => {
    const { engine, id } = shapeModel();
    const committed = { ...params, kind: "star" as const, radius: 4 };

    engine.applyFacadeSnapshot({
      version: 3,
      layers: [restated(id, { layerType: "shape", shapeParams: committed })],
      width: 800,
      height: 600,
    });

    expect(engine.getLayer(id)!.shapeParams).toEqual(committed);
    expect(engine.getLayer(id)!.type).toBe("shape");
  });

  it("leaves both fields alone when the restatement omits them", () => {
    const { engine, id } = shapeModel();
    const before = engine.getLayer(id)!.shapeParams;

    engine.applyFacadeSnapshot({
      version: 4,
      layers: [restated(id)],
      width: 800,
      height: 600,
    });

    expect(engine.getLayer(id)!.shapeParams).toBe(before);
    expect(engine.getLayer(id)!.type).toBe("shape");
  });

  it("rasterize: a restated layerType flips type, and the cleared shapeParams cannot ride the wire (absent means keep)", () => {
    const { engine, id } = shapeModel();

    // What apply_rasterize emits: layer_type Some(Raster) and shape_params None,
    // which serializes as an ABSENT key (crates/core/src/model.rs). The delta
    // shape asserted here is the one bridgeEmuArms.test.ts pins for the
    // rasterize arm.
    engine.applyFacadeSnapshot({
      version: 5,
      layers: [restated(id, { layerType: "raster" })],
      width: 800,
      height: 600,
    });

    // The kind travels, so the model stops being a shape layer.
    expect(engine.getLayer(id)!.type).toBe("raster");
    // The params do NOT travel: absence means keep, and the arm has no way to
    // say "cleared". routeRasterize deletes them on the model after the command.
    expect(engine.getLayer(id)!.shapeParams).toEqual(params);
  });
});
