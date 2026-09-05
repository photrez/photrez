// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Arm the Rust graph engine before constructing DocumentEngine so these tests
// exercise the Rust mirror, not the TS fallback. The live Rust SSOT path was
// never exercised by document.test.ts, because that suite constructed
// `DocumentEngine` WITHOUT awaiting `getWasmExportModule()` first, so
// `this.rustEngine === null` and every graph-op test silently ran the TS
// fallback. That hiding gap is exactly why the just-fixed `duplicate_layer`
// position/name/lock divergences shipped.
//
// This harness arms the wasm module in beforeAll (mirroring the production
// loader used by facadeDelete.test.ts:26-40) BEFORE constructing the TS
// DocumentEngine, so `rustEngine` is armed and graph ops delegate to the REAL
// Rust mirror. It then asserts Rust-path ORDER / POSITION / NAME / LOCK
// behavior - including the two regressions that the TS-fallback tests missed.

import { describe, it, expect, beforeAll, afterEach, vi, beforeEach } from "vitest";
import { DocumentEngine } from "../document";
import { getWasmExportModule } from "@/components/editor/wasmExport";
import { DEFAULT_TEXT_DATA } from "../textTypes";
import { rasterizeText } from "../textRasterizer";

// jsdom has no OffscreenCanvas; stub it so the engine's composite step produces
// a non-null bitmap and the merge graph op routes to the Rust mirror (the merge
// ORDER assertions are what these tests guard). Stubbed per-test (not globally)
// so the text-duplicate test keeps its native (HTMLCanvasElement) bitmap path.
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

let wasmMod: any = null;

beforeAll(async () => {
  wasmMod = await getWasmExportModule();
});

beforeEach(() => {
  // Photrez.facade stays OFF - these tests are about the always-on Rust SSOT
  // graph mirror (USE_RUST_SSOT=true), not the facade-gated ownership stream.
  localStorage.removeItem("photrez.facade");
});

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  localStorage.removeItem("photrez.facade");
});

// Proves the Rust path is actually wired: constructs the TS DocumentEngine with
// the wasm module already loaded, then asserts the duplicate op is delegated to
// the Rust `duplicate_layer` (not the TS fallback).
it("arms the Rust engine and delegates duplicateLayer to it", () => {
  expect(wasmMod?.DocumentEngine).toBeTruthy();
  const duplicateSpy = vi.spyOn(wasmMod.DocumentEngine.prototype, "duplicate_layer");
  const engine = new DocumentEngine("doc-rust-armed", "Rust Armed", 800, 600);
  const l1 = engine.addLayer("Layer 1");
  const dup = engine.duplicateLayer(l1.id);
  expect(duplicateSpy).toHaveBeenCalledTimes(1);
  // The "-copy" id shape is minted only by the Rust duplicate_layer
  // (document.rs:249); the TS fallback mints "layer-<uuid>". Asserting this
  // proves the Rust result was actually used, not just attempted.
  expect(dup.id).toBe(l1.id + "-copy");
  expect(dup.id).toBeTruthy();
  expect(engine.getActiveLayerId()).toBe(dup.id);
});

// Rust path: duplicate of a locked layer is unlocked; duplicate of
// the Background is NOT a background and exactly one background remains.
it("Rust path: duplicate is unlocked and does not inherit background", () => {
  const engine = new DocumentEngine("doc-rust-locks", "Rust Locks", 800, 600);
  const bg = engine.addLayer("Background");
  engine.markLayerAsBackground(bg.id); // flags isBackground + position/rotation locks (Rust)
  const l1 = engine.addLayer("Layer 1");
  engine.setLayerLocked(l1.id, true);

  const dupBg = engine.duplicateLayer(bg.id);
  const dupBgNode = engine.getLayer(dupBg.id)!;
  expect(dupBgNode.isBackground).toBeUndefined();
  expect(dupBgNode.locked).toBe(false);

  const dupL1 = engine.duplicateLayer(l1.id);
  const dupL1Node = engine.getLayer(dupL1.id)!;
  expect(dupL1Node.locked).toBe(false);

  // Exactly one background must remain in the document.
  const bgCount = engine.getLayers().filter((l) => l.isBackground).length;
  expect(bgCount).toBe(1);
});

// Rust path: duplicate name follows the TS numeric-suffix
// convention ("Layer 1" -> "Layer 2" -> "Layer 3"), not a literal " copy".
it("Rust path: duplicate name uses numeric suffix", () => {
  const engine = new DocumentEngine("doc-rust-name", "Rust Name", 800, 600);
  const l1 = engine.addLayer("Layer 1");
  const d1 = engine.duplicateLayer(l1.id);
  const d2 = engine.duplicateLayer(l1.id);
  expect(d1.name).toBe("Layer 2");
  expect(d2.name).toBe("Layer 3");
  const names = engine.getLayers().map((l) => l.name);
  expect(names).not.toContain("Layer 1 copy");
});

// Rust-path ORDER assertion: duplicate sits immediately ABOVE the source in the
// top-indexed stack (index 0 = top), matching addLayer / duplicateLayer order.
it("Rust path: duplicate sits immediately above the source", () => {
  const engine = new DocumentEngine("doc-rust-order", "Rust Order", 800, 600);
  engine.addLayer("Layer 1");
  const l2 = engine.addLayer("Layer 2");
  const l3 = engine.addLayer("Layer 3");
  // Stack (top -> bottom): Layer 3, Layer 2, Layer 1
  const dup = engine.duplicateLayer(l2.id);
  const order = engine.getLayers().map((l) => l.id);
  const srcIdx = order.indexOf(l2.id);
  const dupIdx = order.indexOf(dup.id);
  expect(dupIdx).toBe(srcIdx - 1);
});

// Rust-path POSITION/ORDER assertion for reorder: reordering is delegated to the
// Rust engine and the resulting order is observable from the TS model.
it("Rust path: reorder delegates and reorders the stack", () => {
  expect(wasmMod?.DocumentEngine).toBeTruthy();
  const reorderSpy = vi.spyOn(wasmMod.DocumentEngine.prototype, "reorder_layer");
  const engine = new DocumentEngine("doc-rust-reorder", "Rust Reorder", 800, 600);
  engine.addLayer("Layer 1");
  engine.addLayer("Layer 2");
  engine.addLayer("Layer 3");
  // Stack: [Layer 3, Layer 2, Layer 1]; move top (idx 0) to bottom (idx 2).
  engine.reorderLayer(0, 2);
  expect(reorderSpy).toHaveBeenCalledTimes(1);
  // The true result proves the Rust reorder_layer was used (not the TS fallback,
  // which would also reorder but hide a Rust regression).
  expect(reorderSpy.mock.results[0].value).toBe(true);
  const order = engine.getLayers().map((l) => l.name);
  expect(order).toEqual(["Layer 2", "Layer 1", "Layer 3"]);
});

// Rust-path ORDER assertion: reordering a layer UP (bottom -> top) is delegated
// to the Rust engine and the resulting stack order is observable from the TS model.
it("Rust path: reorder up moves a layer to the top", () => {
  expect(wasmMod?.DocumentEngine).toBeTruthy();
  const reorderSpy = vi.spyOn(wasmMod.DocumentEngine.prototype, "reorder_layer");
  const engine = new DocumentEngine("doc-rust-reorder-up", "Rust Reorder Up", 800, 600);
  engine.addLayer("Layer 1");
  engine.addLayer("Layer 2");
  engine.addLayer("Layer 3");
  // Stack (top -> bottom): [Layer 3, Layer 2, Layer 1]; move bottom (idx 2) to top (idx 0).
  engine.reorderLayer(2, 0);
  expect(reorderSpy).toHaveBeenCalledTimes(1);
  expect(reorderSpy.mock.results[0].value).toBe(true);
  const order = engine.getLayers().map((l) => l.name);
  expect(order).toEqual(["Layer 1", "Layer 3", "Layer 2"]);
});

// Rust-path ORDER assertion: the Background is pinned to the bottom — a reorder
// that would land a layer beneath it is corrected so the Background stays last.
it("Rust path: background stays pinned to the bottom after reorder", () => {
  expect(wasmMod?.DocumentEngine).toBeTruthy();
  const reorderSpy = vi.spyOn(wasmMod.DocumentEngine.prototype, "reorder_layer");
  const engine = new DocumentEngine("doc-rust-bg-pin", "Rust BG Pin", 800, 600);
  const bg = engine.addLayer("Background");
  engine.markLayerAsBackground(bg.id);
  const l1 = engine.addLayer("Layer 1");
  const l2 = engine.addLayer("Layer 2");
  // Stack (top -> bottom): [Layer 2, Layer 1, Background]; try to push Layer 2 below Background.
  engine.reorderLayer(0, 2);
  expect(reorderSpy).toHaveBeenCalledTimes(1);
  expect(reorderSpy.mock.results[0].value).toBe(true);
  const layers = engine.getLayers();
  const ids = layers.map((l) => l.id);
  // Full order must be [Layer 1, Layer 2, Background]; asserting only the last
  // slot would let a wrong-but-bg-last order pass.
  expect(ids).toEqual([l1.id, l2.id, bg.id]);
  expect(layers.filter((l) => l.isBackground).length).toBe(1);
});

// Rust-path ORDER/POSITION assertion for mergeDown: the merged node lands at the
// pair's stack position and the two source layers are removed.
it("Rust path: mergeDown lands the merged node at the pair position", () => {
  expect(wasmMod?.DocumentEngine).toBeTruthy();
  stubOffscreenCanvas();
  const mergeSpy = vi.spyOn(wasmMod.DocumentEngine.prototype, "merge_down");
  const engine = new DocumentEngine("doc-rust-mergedown", "Rust MergeDown", 800, 600);
  const l1 = engine.addLayer("Layer 1");
  const l2 = engine.addLayer("Layer 2");
  const l3 = engine.addLayer("Layer 3");
  // Stack (top -> bottom): [Layer 3, Layer 2, Layer 1]; merge Layer 2 down into Layer 1.
  engine.mergeDown(l2.id);
  expect(mergeSpy).toHaveBeenCalledTimes(1);
  const mergedId = mergeSpy.mock.calls[0][1] as string;
  const ids = engine.getLayers().map((l) => l.id);
  // Merged node replaces the L2/L1 pair at the pair's stack position (below L3).
  expect(ids).toEqual([l3.id, mergedId]);
  expect(ids).not.toContain(l2.id);
  expect(ids).not.toContain(l1.id);
  expect(engine.getLayer(mergedId)).toBeTruthy();
});

// Rust-path ORDER/POSITION assertion for mergeSelectedLayers: the merged node
// lands at the highest stack position of the selection and sources are removed.
it("Rust path: mergeSelected lands at the topmost selected position", () => {
  expect(wasmMod?.DocumentEngine).toBeTruthy();
  stubOffscreenCanvas();
  const mergeSpy = vi.spyOn(wasmMod.DocumentEngine.prototype, "merge_selected");
  const engine = new DocumentEngine("doc-rust-mergesel", "Rust MergeSelected", 800, 600);
  const l1 = engine.addLayer("Layer 1");
  const l2 = engine.addLayer("Layer 2");
  const l3 = engine.addLayer("Layer 3");
  // Stack (top -> bottom): [Layer 3, Layer 2, Layer 1]; merge Layer 3 + Layer 1.
  engine.mergeSelectedLayers([l3.id, l1.id]);
  expect(mergeSpy).toHaveBeenCalledTimes(1);
  const mergedId = mergeSpy.mock.calls[0][1] as string;
  const ids = engine.getLayers().map((l) => l.id);
  // Merged node replaces the selection at the topmost selected position (Layer 3).
  expect(ids).toEqual([mergedId, l2.id]);
  expect(ids).not.toContain(l3.id);
  expect(ids).not.toContain(l1.id);
  expect(engine.getLayer(mergedId)).toBeTruthy();
});

// Pixel-epoch preservation: a Rust graph op must keep the layer's Rust pixel-store epoch so
// ensureBitmapCurrent does not take a redundant full readback afterwards.
it("Rust path: graph op preserves the layer bitmap epoch", () => {
  const engine = new DocumentEngine("doc-rust-epoch", "Rust Epoch", 800, 600);
  const l1 = engine.addLayer("Layer 1");
  const l2 = engine.addLayer("Layer 2");
  // Simulate a prior ensureBitmapCurrent that stamped the layer as current.
  engine.getLayer(l1.id)!.bitmapEpoch = 7;
  // A real Rust-path reorder rebuilds the model via syncLayersFromRust.
  engine.reorderLayer(0, 1);
  expect(engine.getLayer(l1.id)!.bitmapEpoch).toBe(7);
});

// Text-layer duplicate on the Rust path must clone at the source bitmap's 2x
// resolution (not doc-space layer dims), so it is not baked at 1x nor clipped
// to the top-left quadrant. Stubs OffscreenCanvas so the Rust clone path runs
// instead of silently throwing into the TS fallback.
it("Rust path: text duplicate keeps the 2x raster resolution", () => {
  stubOffscreenCanvas();
  const engine = new DocumentEngine("doc-rust-textdup", "Rust Text Dup", 800, 600);
  const src = engine.addTextLayer("Text", { ...DEFAULT_TEXT_DATA, content: "Hi" });
  const dup = engine.duplicateLayer(src.id);
  // Rust graph op minted the "-copy" id; the TS fallback mints "layer-<uuid>",
  // so this proves the Rust path actually ran (not the silent TS fallback).
  expect(dup.id).toBe(src.id + "-copy");
  expect(engine.getLayers().length).toBe(2);
  expect(src.imageBitmap).toBeTruthy();
  expect(dup.imageBitmap).toBeTruthy();
  // Clone must match the source bitmap's 2x dimensions (doc-space layer dims
  // are 1x). Before the fix this copied at 1x and clipped the text.
  expect(dup.imageBitmap!.width).toBe(src.imageBitmap!.width);
  expect(dup.imageBitmap!.height).toBe(src.imageBitmap!.height);
  // Matches what the TS path re-rasters at (2x), so no 1x softening.
  const reRastered = rasterizeText(src.textData!).imageBitmap;
  expect(dup.imageBitmap!.width).toBe(reRastered.width);
});
