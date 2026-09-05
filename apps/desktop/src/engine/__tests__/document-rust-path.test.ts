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
import { setDeviceMaxTextureSize, MAX_CANVAS_DIM, getEffectiveMaxDim, MAX_LAYERS } from "../types";
import * as layerOps from "../layerOps";

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
  // Reset the device-adaptive dimension cap so a test that lowers it does not
  // leak into siblings (the default is MAX_CANVAS_DIM).
  setDeviceMaxTextureSize(MAX_CANVAS_DIM);
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

// Rust-path ORDER assertion: the Background is pinned to the bottom - a reorder
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

// Regression for the hardcoded-dim-cap divergence: a GPU reporting
// MAX_TEXTURE_SIZE < 16384 clamps the effective dim via setDeviceMaxTextureSize,
// so an addLayer whose side exceeds the EFFECTIVE limit must be rejected on the
// Rust path with the same "device limit" error as the baseline. Before the fix
// the Rust mirror accepted up to its fixed 16384 ceiling (can_accept_layer never
// saw the device limit), so this call silently added an oversized layer instead
// of throwing - the test fails before, passes after.
it("Rust path: addLayer rejects a layer above the effective device dim limit", () => {
  setDeviceMaxTextureSize(4096); // simulate a <16384 GPU (webgl2 tests mock 3379)
  expect(getEffectiveMaxDim()).toBe(4096);
  const engine = new DocumentEngine("doc-rust-devicelimit", "Rust DeviceLimit", 800, 600);
  expect(() => engine.addLayer("Oversized", 5000, 4000)).toThrow(
    /Layer dimensions exceed device limit 4096px per side/,
  );
  // The rejected layer must not have been added to the Rust mirror.
  expect(engine.getLayers().length).toBe(0);
});

// Regression for the opaque-rejection divergence: addLayer past MAX_LAYERS must
// throw the descriptive "Maximum layer limit" error on the Rust path, not leak
// the opaque TypeError that the Rust mirror's void return produced (markLayer-
// Dirty(undefined.id)). Before the fix this threw a TypeError (no message match);
// the test fails before, passes after.
it("Rust path: addLayer past MAX_LAYERS throws the descriptive error", () => {
  const engine = new DocumentEngine("doc-rust-maxlayers", "Rust MaxLayers", 800, 600);
  for (let i = 0; i < MAX_LAYERS; i++) engine.addLayer(`Layer ${i}`);
  expect(engine.getLayers().length).toBe(MAX_LAYERS);
  expect(() => engine.addLayer("Overflow")).toThrow(/Maximum layer limit of 200 reached/);
  // No layer was added by the rejected call.
  expect(engine.getLayers().length).toBe(MAX_LAYERS);
});

// Regression for the duplicate-skips-can_accept_layer divergence: duplicating
// past MAX_LAYERS must throw the descriptive error on the Rust path. Before the
// fix the Rust duplicate_layer skipped the check and added a 201st layer (newId
// truthy -> early return), so the call succeeded instead of throwing - the test
// fails before, passes after.
it("Rust path: duplicateLayer past MAX_LAYERS throws the descriptive error", () => {
  const engine = new DocumentEngine("doc-rust-dupmax", "Rust DupMax", 800, 600);
  for (let i = 0; i < MAX_LAYERS; i++) engine.addLayer(`Layer ${i}`);
  const srcId = engine.getLayers()[0].id;
  expect(() => engine.duplicateLayer(srcId)).toThrow(/Maximum layer limit of 200 reached/);
  // No layer was added by the rejected duplicate.
  expect(engine.getLayers().length).toBe(MAX_LAYERS);
});

// Regression for the budget-guard gap on the armed Rust-delegate path:
// addLayer must reject with the descriptive E_RESOURCE_LIMIT message when the
// pixel memory budget is exceeded, and it must do so BEFORE the Rust mirror is
// mutated. The guard calls canAddLayer (aliased canFitLayer) before
// rustEngine.add_layer, so forcing that predicate to false trips the rejection
// deterministically without allocating a large fixture. Without the guard,
// rustEngine would accept the layer and no error would throw - the test fails
// before, passes after.
it("Rust path: addLayer over budget throws E_RESOURCE_LIMIT and does not mutate Rust", () => {
  const fitSpy = vi.spyOn(layerOps, "canAddLayer").mockReturnValue(false);
  const engine = new DocumentEngine("doc-rust-addbudget", "Rust AddBudget", 800, 600);
  expect(() => engine.addLayer("OverBudget")).toThrow(
    /E_RESOURCE_LIMIT: Adding this layer exceeds maximum pixel memory budget\./,
  );
  // The rejected layer must not have been added to the Rust mirror.
  expect(engine.getLayers().length).toBe(0);
  fitSpy.mockRestore();
});

// Regression for the budget-guard gap on the armed Rust-delegate path:
// duplicateLayer must reject with the descriptive E_RESOURCE_LIMIT message when
// duplicating would exceed the pixel memory budget, and it must do so BEFORE the
// Rust mirror is mutated. The guard calls canAddLayer (aliased canFitLayer) with
// the source dims before rustEngine.duplicate_layer, so forcing that predicate
// to false trips the rejection deterministically. Without the guard,
// rustEngine would add the duplicate and no error would throw - the test fails
// before, passes after.
it("Rust path: duplicateLayer over budget throws E_RESOURCE_LIMIT and does not mutate Rust", () => {
  const engine = new DocumentEngine("doc-rust-dupbudget", "Rust DupBudget", 800, 600);
  const src = engine.addLayer("Layer 1");
  const fitSpy = vi.spyOn(layerOps, "canAddLayer").mockReturnValue(false);
  expect(() => engine.duplicateLayer(src.id)).toThrow(
    /E_RESOURCE_LIMIT: Duplicating this layer exceeds maximum pixel memory budget\./,
  );
  // The rejected duplicate must not have been added to the Rust mirror.
  expect(engine.getLayers().length).toBe(1);
  fitSpy.mockRestore();
});
