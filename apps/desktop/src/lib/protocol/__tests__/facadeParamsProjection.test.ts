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
