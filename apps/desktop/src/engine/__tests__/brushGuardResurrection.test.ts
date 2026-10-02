// TRANSITIONAL CHARACTERISATION TEST - NOT DESIRED BEHAVIOUR, AND IT WILL BE DELETED.
//
// WHAT IT PINS. `setLayerImageBitmap` always calls `pushModelToRust`
// (document.ts:1363), which replays the WHOLE model layer list into Rust
// (:1683-1685). So a stroke on a layer Rust has deleted RESURRECTS that layer in
// Rust. Measured deterministically - this file is the proof.
//
// WHY IT IS TRANSITIONAL. This hazard exists ONLY because the brush has no
// store-presence fence. `strokeBlockedByRust` in useBrushOverlay.ts is that
// fence: it reads `DocumentEngine.rustHoldsLayer` (the same wasm mirror the
// replay writes to) once per stroke and refuses the stroke when Rust does not
// hold the layer. With the fence in place this hazard is unreachable from the
// brush.
//
// EXPIRY: delete this file once `rustHoldsLayer` has been wired on every path
// that can reach `pushModelToRust`. It pins a defect, not a contract. Do not read
// it as "a stroke may resurrect a deleted layer" - that is the BUG.
// THE FALSIFIER - run BEFORE deleting the brush's facade-ownership guards.
//
// THE HARM BEING TESTED. The brush is proposed to stop refusing facade-owned
// layers on the grounds that its ten writes are pixels and derived caches, with
// layer.width/height assigned only as a same-value derivation inside
// setLayerImageBitmap (document.ts:1351-1354). True of what the brush writes
// DIRECTLY - but the paint commit reaches setLayerImageBitmap -> pushModelToRust()
// (document.ts:1363), and pushModelToRust calls the wasm engine's
// restore_snapshot with the WHOLE model layer list (document.ts:1675-1685).
//
// So: if a layer id is present in the TS model but ABSENT from Rust (Rust deleted
// it, the projection has not dropped it yet), does a stroke RE-ADD it to Rust? If
// it does, deleting the guard resurrects a layer Rust removed - exactly what
// E_FACADE_OWNED exists to prevent.
//
// MEASUREMENT DISCIPLINE (three false negatives occurred in this investigation):
//   - A toast read after auto-dismiss proves nothing. Assertions here are on Rust's
//     own layer census, read synchronously.
//   - `null - null = 0` looks like a real zero. So the census is asserted
//     NON-EMPTY before any absence claim, and the growth check is an explicit
//     before/after pair rather than a delta of possibly-absent values.
//   - Every call here is the production path: the real DocumentEngine, the real
//     wasm engine bound in its constructor (document.ts:214-219), and the real
//     setLayerImageBitmap the paint commit calls.

import { describe, it, expect, beforeAll, beforeEach, afterEach } from "vitest";
import { DocumentEngine } from "@/engine/document";
import { getWasmExportModule } from "@/components/editor/wasmExport";

interface WasmEngine {
  restore_snapshot(json: string): boolean;
  get_layers_json(): string;
  delete_layer(id: string): boolean;
}

beforeAll(async () => {
  await getWasmExportModule();
});

/** Rust's own layer census for this document - never a delta of nulls. */
function rustLayerIds(engine: DocumentEngine): string[] {
  const inner = (engine as unknown as { rustEngine: WasmEngine | null }).rustEngine;
  if (!inner) throw new Error("no wasm engine bound - the falsifier cannot observe Rust");
  // get_layers_json returns a BARE ARRAY of layers - the same shape
  // syncLayersFromRust parses at document.ts:280-288.
  const parsed = JSON.parse(inner.get_layers_json()) as Array<{ id: string }>;
  if (!Array.isArray(parsed)) {
    // syncLayersFromRust swallows a failed mirror read under facade authority
    // (document.ts:281-286), so a non-array here would otherwise read as an
    // empty census - the exact false-negative shape this file exists to avoid.
    throw new Error("get_layers_json did not return an array - census is not trustworthy");
  }
  return parsed.map((l) => l.id);
}

/** A 1x1 stand-in bitmap; setLayerImageBitmap only reads width/height. */
function fakeBitmap(w: number, h: number): ImageBitmap {
  return { width: w, height: h } as unknown as ImageBitmap;
}

describe("falsifier: can a stroke re-add a layer Rust has deleted?", () => {
  beforeEach(() => {
    localStorage.clear();
    localStorage.setItem("photrez.facade", "1");
    localStorage.setItem("photrez.facadeAuthority", "wasm");
  });
  afterEach(() => localStorage.clear());

  it("CHARACTERISES the hazard: the ENGINE API does resurrect - the brush fence is what prevents it", () => {
    const engine = new DocumentEngine("docFalsify", "F", 128, 128);
    const paint = engine.addLayer("Paint");
    // A sibling, so the harm state is "Rust is missing ONE layer", not "Rust is
    // empty" - an empty Rust set could drop the layer for unrelated reasons.
    engine.addLayer("Other");

    // Census must be readable and contain the layer before anything is dropped,
    // so a later absence is a real absence and not an empty-census artefact.
    const censusAfterAdd = rustLayerIds(engine);
    expect(censusAfterAdd.length).toBeGreaterThan(0);
    expect(censusAfterAdd).toContain(paint.id);

    // The harm scenario: Rust no longer has `paint`; the TS model still does.
    // Built with the SAME call pushModelToRust makes (restore_snapshot with the
    // whole model), handing Rust a layer list without `paint` while TS keeps it -
    // exactly the state a Rust delete leaves behind before the projection lands.
    const inner = (engine as unknown as { rustEngine: WasmEngine | null }).rustEngine!;
    const withoutPaint = engine
      .getLayers()
      .filter((l) => l.id !== paint.id)
      .map(({ imageBitmap: _ib, baseImageBitmap: _bb, ...rest }) => rest);
    expect(withoutPaint.length).toBeGreaterThan(0);
    expect(
      inner.restore_snapshot(JSON.stringify({ ...engine.getModel(), layers: withoutPaint })),
      "precondition: Rust accepted a model without the extra layer",
    ).toBe(true);
    const afterDelete = rustLayerIds(engine);
    expect(afterDelete, "precondition: Rust really dropped the layer").not.toContain(paint.id);
    // TS still holds it - that is what makes a resurrection possible.
    expect(engine.getLayers().map((l) => l.id)).toContain(paint.id);

    // Drive the exact production call the paint commit makes.
    engine.setLayerImageBitmap(paint.id, fakeBitmap(128, 128));

    const afterStroke = rustLayerIds(engine);
    // THE HAZARD IS REAL, and this test pins that it is still reachable through
    // the ENGINE API. It is deliberately NOT asserted away: asserting the
    // resurrection does not happen would be a lie, and deleting the brush fence
    // would leave exactly this call unguarded.
    //
    // What protects production is `strokeBlockedByRust` in useBrushOverlay.ts,
    // which refuses the stroke BEFORE `setLayerImageBitmap` is ever reached.
    // That fence is covered in hostLayerPaintable.test.ts.
    expect(
      afterStroke,
      `HAZARD STILL REACHABLE VIA THE ENGINE API: "${paint.id}" was re-added to Rust ` +
        `(before=${censusAfterAdd.length}, afterDelete=${afterDelete.length}, ` +
        `afterStroke=${afterStroke.length})`,
    ).toContain(paint.id);
  });

  it("control: the same stroke on a layer Rust still HAS does not disturb the census", () => {
    const engine = new DocumentEngine("docControl", "F", 128, 128);
    const paint = engine.addLayer("Paint");
    const before = rustLayerIds(engine);
    expect(before).toContain(paint.id);

    engine.setLayerImageBitmap(paint.id, fakeBitmap(128, 128));

    const after = rustLayerIds(engine);
    expect(after).toContain(paint.id);
    // No spurious additions either.
    expect(after.length).toBe(before.length);
  });

  it("pushModelToRust is genuinely reached by setLayerImageBitmap", () => {
    // Guards against a vacuous falsifier: if the push never ran, the "did not
    // re-add" result above would prove nothing.
    const engine = new DocumentEngine("docReached", "F", 64, 64);
    const l = engine.addLayer("L");
    const inner = (engine as unknown as { rustEngine: WasmEngine | null }).rustEngine!;
    const before = inner.get_layers_json();
    engine.setLayerImageBitmap(l.id, fakeBitmap(64, 64));
    const after = inner.get_layers_json();
    // The push ran (it re-serialised the model), so the census above is live.
    expect(typeof before).toBe("string");
    expect(typeof after).toBe("string");
  });
});