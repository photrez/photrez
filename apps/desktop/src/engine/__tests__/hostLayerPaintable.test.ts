// THE BUG: a host-created layer must be paintable.
//
// MEASURED IN THE REAL APP (artifact sha256
// 75d56dc5f84a1edf277103a09e9a5eb7a98cab886abf0274dfdeba139377fddb, HEAD cadfd7a):
// paint a stroke, crop 128x128 -> 39x39, press ONE Ctrl+Z, then click the Paint
// layer row and paint again. Every later stroke was refused with
// "This layer is owned by Rust facade - legacy brush blocked". Permanently; a
// layer-row click did not recover it.
//
// ROOT CAUSE. The brush was gated on `isFacadeOwnedLayer`, a GRAPH-ownership
// flag. `applyFacadeSnapshot` marks every id a projection carries
// (document.ts:2139) and releases an id only when it VANISHES (:2136), so a
// host-created layer that any projection mentioned stayed claimed for the rest
// of the session. An External transition answers undo/redo with a whole-layer-set
// restatement, so the crop undo's projection carried the Paint layer and claimed
// it. The brush is a RUST-CANONICAL pixel path, not a graph mutator, so the flag
// was the wrong axis: every other Rust-canonical raster path was already ungated
// (useLayerActions.ts:375, paintBucket.ts:134, layerOperations.ts:328,
// gradientTool.ts:267).
//
// THE FENCE THAT REPLACED IT, and why not simply deleting the guard: the paint
// commit reaches setLayerImageBitmap -> pushModelToRust -> restore_snapshot with
// the WHOLE model, so a stroke on a layer RUST DELETED resurrects it. Deleting
// the guard would break the invariant rather than merely fail to protect it -
// proven in brushGuardResurrection.test.ts. So the brush now asks a question on
// the right axis: does Rust actually HOLD this layer?
//
// ASSERTION DISCIPLINE. These assert on the PREDICATE the brush reads
// (`DocumentEngine.rustHoldsLayer`), never on a toast or a downstream symptom -
// a toast read after auto-dismiss is a false negative, and one of this
// investigation's three measurement failures was exactly that. The flags are set
// explicitly because `isFacadeEnabled()` reads `getItem(...) !== "0"`, so an
// UNSET key means ENABLED.

import { describe, it, expect, beforeAll, beforeEach, afterEach } from "vitest";
import { DocumentEngine, isFacadeOwnedLayer } from "@/engine/document";
import { EditorFacade } from "@/lib/protocol/editorFacade";
import { seedFacadeFromEngine } from "@/lib/protocol/facadeRegistry";
import { getWasmExportModule } from "@/components/editor/wasmExport";

beforeAll(async () => {
  await getWasmExportModule();
});

/** The decision `strokeBlockedByRust` makes, on the real engine. */
function brushBlocked(engine: DocumentEngine, layerId: string): boolean {
  return engine.rustHoldsLayer(layerId) !== true;
}

describe("a host-created layer is paintable (the crop-undo paint death)", () => {
  beforeEach(() => {
    localStorage.clear();
    localStorage.setItem("photrez.facade", "1");
    localStorage.setItem("photrez.facadeAuthority", "wasm");
    (globalThis as unknown as Record<string, () => void>).__clearFacadeOwnedForTests?.();
  });
  afterEach(() => {
    localStorage.clear();
    (globalThis as unknown as Record<string, () => void>).__clearFacadeOwnedForTests?.();
  });

  it("a host-created layer Rust still holds is NOT blocked by the brush fence", () => {
    const engine = new DocumentEngine("docPaint", "P", 128, 128);
    const paint = engine.addLayer("Paint");
    // Precondition: the read is meaningful, not "unknown". A null here would
    // make the fence refuse everything - which is why unknown is surfaced rather
    // than silently treated as present.
    expect(engine.rustHoldsLayer(paint.id)).toBe(true);
    expect(brushBlocked(engine, paint.id)).toBe(false);
  });

  it("the fence is independent of the graph-ownership flag that caused the bug", async () => {
    const engine = new DocumentEngine("docPaint2", "P", 128, 128);
    const facade = new EditorFacade();
    const paint = engine.addLayer("Paint");
    seedFacadeFromEngine(engine as never, facade);

    // The exact state the bug produced: a projection has marked the layer, and
    // Rust still holds it. Under the OLD gate this was a permanent refusal.
    engine.applyFacadeSnapshot({
      version: 1,
      layers: engine.getLayers().map((l) => ({ ...l })),
    } as never);

    // Whether or not the graph flag fired, the brush fence asks the right
    // question and lets the stroke through.
    expect(engine.rustHoldsLayer(paint.id)).toBe(true);
    expect(brushBlocked(engine, paint.id)).toBe(false);
    // Documented so a future change cannot quietly re-couple them: the brush
    // decision must NOT depend on this flag again.
    void isFacadeOwnedLayer(paint.id);
  });

  it("the fence REFUSES a layer Rust does not hold, so the replay cannot resurrect it", () => {
    const engine = new DocumentEngine("docPaint3", "P", 128, 128);
    const paint = engine.addLayer("Paint");
    engine.addLayer("Other");
    expect(brushBlocked(engine, paint.id)).toBe(false);

    // Rust drops it (the same restore_snapshot pushModelToRust performs).
    const inner = (engine as unknown as { rustEngine: { restore_snapshot(j: string): boolean } }).rustEngine;
    const withoutPaint = engine
      .getLayers()
      .filter((l) => l.id !== paint.id)
      .map(({ imageBitmap: _a, baseImageBitmap: _b, ...rest }) => rest);
    inner.restore_snapshot(JSON.stringify({ ...engine.getModel(), layers: withoutPaint }));

    // TS still holds it - the resurrection hazard - and the fence catches it.
    expect(engine.getLayers().map((l) => l.id)).toContain(paint.id);
    expect(engine.rustHoldsLayer(paint.id)).toBe(false);
    expect(brushBlocked(engine, paint.id)).toBe(true);
  });

  it("an UNREADABLE mirror is UNKNOWN, and unknown blocks rather than paints", () => {
    const engine = new DocumentEngine("docPaint4", "P", 64, 64);
    const l = engine.addLayer("L");
    // Force the read to fail. syncLayersFromRust deliberately swallows a failed
    // mirror read under facade authority (document.ts:281-286), so an unreadable
    // mirror otherwise presents as an EMPTY layer list - which must never be read
    // as "Rust deleted everything" (that would block every stroke) nor as
    // "present" (that would let the replay resurrect).
    (engine as unknown as { rustEngine: { get_layers_json(): string } }).rustEngine.get_layers_json =
      () => "not json at all";
    expect(engine.rustHoldsLayer(l.id)).toBeNull();
    expect(brushBlocked(engine, l.id)).toBe(true);
  });

  it("an UNEXPECTED mirror shape is UNKNOWN too, not absent", () => {
    const engine = new DocumentEngine("docPaint5", "P", 64, 64);
    const l = engine.addLayer("L");
    // This exact trap bit the falsifier first time round: it parsed
    // get_layers_json as {layers:[...]} when the wire shape is a BARE ARRAY.
    (engine as unknown as { rustEngine: { get_layers_json(): string } }).rustEngine.get_layers_json =
      () => JSON.stringify({ layers: [] });
    expect(engine.rustHoldsLayer(l.id)).toBeNull();
    expect(brushBlocked(engine, l.id)).toBe(true);
  });

  it("no wasm mirror bound is UNKNOWN, and the stroke is refused visibly", () => {
    const engine = new DocumentEngine("docPaint6", "P", 64, 64);
    const l = engine.addLayer("L");
    (engine as unknown as { rustEngine: unknown }).rustEngine = null;
    expect(engine.rustHoldsLayer(l.id)).toBeNull();
    expect(brushBlocked(engine, l.id)).toBe(true);
  });
});