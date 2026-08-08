import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { WorkspaceManager } from "../workspace";
import type { LayerNode } from "../types";
import { DEFAULT_TEXT_DATA, normalizeTextData } from "../textTypes";
import type { TextData } from "../textTypes";
import { stubTextOffscreenCanvas } from "../../__tests__/test-builders";

function makeSession() {
  return WorkspaceManager.createBlankDocument("text-test", "Text Test", 400, 300);
}

// jsdom provides no OffscreenCanvas; both the text rasterizer and
// duplicateLayerNode rely on it. Shared text-capable seam (10px/char,
// live-dims transferToImageBitmap) — see stubTextOffscreenCanvas.
beforeEach(() => stubTextOffscreenCanvas());
afterEach(() => vi.unstubAllGlobals());

// DEFAULT_TEXT_DATA, fontSize 48 (fontPx 96 at RASTER_SCALE 2), lineHeight 1.4:
// canvas height = ceil(1*1.4*96 + 80 + 24 + 4) = 243 -> doc height 121.5.
const helloData: TextData = { ...DEFAULT_TEXT_DATA, content: "Hello" };

describe("engine text layer ops", () => {
  it("addTextLayer creates a text layer with valid bitmap and normalized textData", () => {
    const s = makeSession();
    const layer = s.engine.addTextLayer("Text 1", helloData);
    expect(layer.type).toBe("text");
    expect(layer.textData).toEqual(normalizeTextData(helloData));
    expect(layer.imageBitmap).not.toBeNull();
    // 5 chars * 10 = 50 wide + 4 padding = 54 canvas -> 27 doc px.
    expect(layer.width).toBe(27);
    expect(layer.height).toBe(121.5);
    expect(s.engine.getActiveLayerId()).toBe(layer.id);
  });

  it("addTextLayer with invalid input normalizes (never throws)", () => {
    const s = makeSession();
    expect(() => s.engine.addTextLayer("T", { fontSize: NaN } as unknown as TextData)).not.toThrow();
    const layer = s.engine.getLayer(s.engine.getActiveLayerId()!)!;
    expect(layer.type).toBe("text");
    expect(layer.textData!.fontSize).toBe(DEFAULT_TEXT_DATA.fontSize);
    expect(layer.imageBitmap).not.toBeNull();
  });

  it("updateTextData re-rasterizes and updates layer size", () => {
    const s = makeSession();
    const layer = s.engine.addTextLayer("Text 1", helloData);
    const newData = { ...DEFAULT_TEXT_DATA, content: "Hello World" };
    s.engine.updateTextData(layer.id, newData);
    const updated = s.engine.getLayer(layer.id)!;
    expect(updated.textData).toEqual(newData);
    // 11 chars * 10 = 110 wide + 4 padding = 114 canvas -> 57 doc px.
    expect(updated.width).toBe(57);
    expect(updated.height).toBe(121.5);
    expect(updated.imageBitmap).not.toBeNull();
  });

  it("updateTextData on non-text layer is a no-op (does not throw)", () => {
    const s = makeSession();
    s.engine.addLayer("Raster 1", 100, 100);
    const rasterId = s.engine.getActiveLayerId()!;
    expect(() => s.engine.updateTextData(rasterId, DEFAULT_TEXT_DATA)).not.toThrow();
    expect(s.engine.getLayer(rasterId)!.type).toBe("raster");
  });

  it("updateTextData with invalid input normalizes instead of throwing", () => {
    const s = makeSession();
    const layer = s.engine.addTextLayer("Text 1", helloData);
    expect(() =>
      s.engine.updateTextData(layer.id, { content: 42, fontSize: "48" } as unknown as TextData),
    ).not.toThrow();
    const after = s.engine.getLayer(layer.id)!;
    expect(after.textData!.fontSize).toBe(DEFAULT_TEXT_DATA.fontSize);
    expect(after.textData!.content).toBe("");
    expect(after.imageBitmap).not.toBeNull();
  });

  it("textLayerToRaster drops textData but keeps the bitmap", () => {
    const s = makeSession();
    const layer = s.engine.addTextLayer("Text 1", helloData);
    s.engine.textLayerToRaster(layer.id);
    const converted = s.engine.getLayer(layer.id)!;
    expect(converted.type).toBe("raster");
    expect(converted.textData).toBeUndefined();
    expect(converted.imageBitmap).not.toBeNull();
  });

  it("isTextLayer detects text layers only", () => {
    const s = makeSession();
    const text = s.engine.addTextLayer("T", helloData);
    const raster = s.engine.addLayer("R", 10, 10);
    expect(s.engine.isTextLayer(text.id)).toBe(true);
    expect(s.engine.isTextLayer(raster.id)).toBe(false);
    expect(s.engine.isTextLayer("missing")).toBe(false);
  });

  it("duplicateLayerNode copies textData and re-rasterizes the bitmap", () => {
    const s = makeSession();
    const layer = s.engine.addTextLayer("Text 1", helloData);
    const dup = s.engine.duplicateLayer(layer.id);
    expect(dup.type).toBe("text");
    expect(dup.textData).toEqual(normalizeTextData(helloData));
    expect(dup.imageBitmap).not.toBeNull();
    expect((dup.imageBitmap as unknown as { width: number }).width).toBe(54);
  });

  it("snapshot/restore roundtrip preserves textData", () => {
    const s = makeSession();
    const layer = s.engine.addTextLayer("Text 1", helloData);
    s.engine.restore(s.engine.snapshot());
    const after = s.engine.getLayer(layer.id)!;
    expect(after.type).toBe("text");
    expect(after.textData).toEqual(normalizeTextData(helloData));
  });

  it("undo of addTextLayer removes the text; redo restores it with textData intact", () => {
    const s = makeSession();
    const preAdd = s.engine.snapshot();

    const layer = s.engine.addTextLayer("Text 1", helloData);
    s.history.commit(preAdd, "Add Text");
    expect(s.engine.isTextLayer(layer.id)).toBe(true);

    // Undo → layer gone.
    const prev = s.history.undo(s.engine.snapshot());
    s.engine.restore(prev!);
    expect(s.engine.getLayer(layer.id)).toBeUndefined();

    // Redo → layer back with textData (snapshot must carry them).
    const next = s.history.redo(s.engine.snapshot());
    s.engine.restore(next!);
    const restored = s.engine.getLayer(layer.id)!;
    expect(restored.type).toBe("text");
    expect(restored.textData).toEqual(normalizeTextData(helloData));
    expect(restored.imageBitmap).not.toBeNull();
  });

  it("undo of an edit session restores the pre-edit textData (updateTextData roundtrip)", () => {
    const s = makeSession();
    const layer = s.engine.addTextLayer("Text 1", helloData);
    const preEdit = s.engine.snapshot();

    s.engine.updateTextData(layer.id, { ...DEFAULT_TEXT_DATA, content: "Edited" });
    s.history.commit(preEdit, "Edit Text");
    expect(s.engine.getLayer(layer.id)!.textData!.content).toBe("Edited");

    const prev = s.history.undo(s.engine.snapshot());
    s.engine.restore(prev!);
    expect(s.engine.getLayer(layer.id)!.textData!.content).toBe("Hello");
  });

  it("textLayerToRaster is a no-op on a raster layer (type stays raster)", () => {
    const s = makeSession();
    const raster = s.engine.addLayer("R", 10, 10);
    const layer = s.engine.getLayer(raster.id)! as LayerNode;
    s.engine.textLayerToRaster(layer.id);
    expect(s.engine.getLayer(raster.id)!.type).toBe("raster");
  });
});
