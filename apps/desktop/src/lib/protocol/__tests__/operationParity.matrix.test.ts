// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Operation Parity Matrix — metadata-parity measurement between the user-validated
// TypeScript DocumentEngine (authoritative oracle) and the SAME ProtocolEngine code
// compiled to wasm (independent native-path oracle). The two engines are each
// other's oracle; NO hand-written expected values are used.
//
// For every ARMED op the operation is driven TWICE: once through the TS
// DocumentEngine (which delegates graph ops to the real armed wasm mirror) and once
// by driving a real wasm DocumentEngine directly. The two resulting graphs are
// compared field-by-field on the RenderLayer overlap (name / visible / opacity /
// x / y / scaleX / scaleY / rotation). One un-armed op (setLayerBlendMode) is driven
// through the TS engine with the USE_RUST_SSOT graph mirror and its mirrored graph
// is compared against the TS-set value (this measures the mirror, not a command arm).
//
// Anti-masking (the lesson from document-rust-path.test.ts): every armed-op test
// spies on the wasm prototype method it must delegate to and asserts the TS engine's
// specific call reached it. Without that, a silent TS fallback (wasm not loaded)
// would still "pass" against a diff of two TS results. If the wasm module is
// unavailable, each test fails LOUD via requireWasm() — no TS fallback is allowed.

import { describe, it, expect, beforeAll, beforeEach, afterEach, vi } from "vitest";
import { DocumentEngine } from "@/engine/document";
import { CommandHistory } from "@/engine/history";
import { getWasmExportModule } from "@/components/editor/wasmExport";

let wasmMod: any = null;

beforeAll(async () => {
  wasmMod = await getWasmExportModule();
});

beforeEach(() => {
  // Photrez.facade stays OFF — these tests measure the always-on wasm graph
  // mirror (USE_RUST_SSOT=true), not the facade-gated ownership stream.
  localStorage.removeItem("photrez.facade");
});

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  localStorage.removeItem("photrez.facade");
});

// Fails LOUD when the native engine is not present, so no test silently runs the
// TS fallback and reports a false parity pass.
function requireWasm(): any {
  expect(wasmMod).toBeTruthy();
  expect(wasmMod?.DocumentEngine).toBeTruthy();
  return wasmMod;
}

// Normalize a layer to the RenderLayer overlap used for parity comparison.
// `id` is engine-assigned (non-deterministic between the two engines) and is NOT
// compared — the matrix measures graph-state metadata parity, not id minting.
function overlap(layer: any): any {
  const t = layer.transform ?? {};
  return {
    name: layer.name,
    visible: layer.visible,
    opacity: layer.opacity,
    x: t.x,
    y: t.y,
    scaleX: t.scaleX ?? t.scale_x,
    scaleY: t.scaleY ?? t.scale_y,
    rotation: t.rotation,
  };
}

// Read the real wasm engine's graph (independent native oracle).
function wasmLayers(engine: any): any[] {
  return JSON.parse(engine.get_layers_json());
}

// Read the TS engine's projected graph (authoritative oracle).
function tsLayers(ts: DocumentEngine): any[] {
  return (ts.getLayers() as unknown as any[]).map(overlap);
}

// Assert the TS engine's specific op (identified by its layer id) reached the wasm
// prototype method. Catches the silent TS-fallback masking case.
function delegatedTo(spy: any, layerId: string): boolean {
  return spy.mock.calls.some((c: any[]) => c[0] === layerId);
}

function newWasm(docId: string): any {
  return new wasmMod.DocumentEngine(docId, docId, 200, 200);
}

describe("operation parity matrix — armed ops (TS engine vs real wasm)", () => {
  it("addLayer: TS-delegated graph equals direct-wasm graph", () => {
    const m = requireWasm();
    const ts = new DocumentEngine("op-add-ts", "A", 200, 200);
    const wasm = newWasm("op-add-w");
    const spy = vi.spyOn(m.DocumentEngine.prototype, "add_layer");

    const l = ts.addLayer("Layer 1");
    wasm.add_layer(l.id, "Layer 1", 200, 200);

    expect(delegatedTo(spy, l.id)).toBe(true);
    expect(tsLayers(ts)).toEqual(wasmLayers(wasm).map(overlap));
  });

  it("deleteLayer: TS-delegated graph equals direct-wasm graph", () => {
    const m = requireWasm();
    const ts = new DocumentEngine("op-del-ts", "D", 200, 200);
    const wasm = newWasm("op-del-w");
    const spy = vi.spyOn(m.DocumentEngine.prototype, "delete_layer");

    const l1 = ts.addLayer("Layer 1");
    ts.addLayer("Layer 2");
    wasm.add_layer(l1.id, "Layer 1", 200, 200);
    wasm.add_layer("Layer 2", "Layer 2", 200, 200);

    ts.deleteLayer(l1.id);
    wasm.delete_layer(l1.id);

    expect(delegatedTo(spy, l1.id)).toBe(true);
    expect(tsLayers(ts)).toEqual(wasmLayers(wasm).map(overlap));
  });

  it("transform move: TS-delegated graph equals direct-wasm graph", () => {
    const m = requireWasm();
    const ts = new DocumentEngine("op-move-ts", "M", 200, 200);
    const wasm = newWasm("op-move-w");
    const spy = vi.spyOn(m.DocumentEngine.prototype, "move_layer");

    const l1 = ts.addLayer("Layer 1");
    wasm.add_layer(l1.id, "Layer 1", 200, 200);

    ts.moveLayer(l1.id, 10, 20);
    wasm.move_layer(l1.id, 10, 20);

    expect(delegatedTo(spy, l1.id)).toBe(true);
    expect(tsLayers(ts)).toEqual(wasmLayers(wasm).map(overlap));
  });

  it("transform scale: TS-delegated graph equals direct-wasm graph", () => {
    const m = requireWasm();
    const ts = new DocumentEngine("op-scale-ts", "S", 200, 200);
    const wasm = newWasm("op-scale-w");
    const spy = vi.spyOn(m.DocumentEngine.prototype, "transform_layer");

    const l1 = ts.addLayer("Layer 1");
    wasm.add_layer(l1.id, "Layer 1", 200, 200);

    ts.transformLayer(l1.id, { scaleX: 2, scaleY: 3 });
    wasm.transform_layer(l1.id, null, null, 2, 3, null, null, null);

    expect(delegatedTo(spy, l1.id)).toBe(true);
    expect(tsLayers(ts)).toEqual(wasmLayers(wasm).map(overlap));
  });

  it("transform rotate: TS-delegated graph equals direct-wasm graph", () => {
    const m = requireWasm();
    const ts = new DocumentEngine("op-rot-ts", "R", 200, 200);
    const wasm = newWasm("op-rot-w");
    const spy = vi.spyOn(m.DocumentEngine.prototype, "transform_layer");

    const l1 = ts.addLayer("Layer 1");
    wasm.add_layer(l1.id, "Layer 1", 200, 200);

    ts.transformLayer(l1.id, { rotation: 45 });
    wasm.transform_layer(l1.id, null, null, null, null, 45, null, null);

    expect(delegatedTo(spy, l1.id)).toBe(true);
    expect(tsLayers(ts)).toEqual(wasmLayers(wasm).map(overlap));
  });

  it("setOpacity: TS-delegated graph equals direct-wasm graph", () => {
    const m = requireWasm();
    const ts = new DocumentEngine("op-op-ts", "O", 200, 200);
    const wasm = newWasm("op-op-w");
    const spy = vi.spyOn(m.DocumentEngine.prototype, "set_layer_opacity");

    const l1 = ts.addLayer("Layer 1");
    wasm.add_layer(l1.id, "Layer 1", 200, 200);

    ts.setLayerOpacity(l1.id, 0.5);
    wasm.set_layer_opacity(l1.id, 0.5);

    expect(delegatedTo(spy, l1.id)).toBe(true);
    expect(tsLayers(ts)).toEqual(wasmLayers(wasm).map(overlap));
  });

  it("undo/redo after-add: TS snapshot-undo graph equals wasm snapshot-restore graph", () => {
    const m = requireWasm();
    const ts = new DocumentEngine("op-ua-ts", "U", 200, 200);
    const wasm = newWasm("op-ua-w");
    const hist = new CommandHistory();
    const spy = vi.spyOn(m.DocumentEngine.prototype, "add_layer");

    const l1 = ts.addLayer("Layer 1");
    wasm.add_layer(l1.id, "Layer 1", 200, 200);

    // Commit the PRE-op snapshot (before adding Layer 2) on both paths.
    hist.commit(ts.snapshot());
    const preAddL2Wasm = wasm.snapshot_json(); // [Layer 1]

    ts.addLayer("Layer 2");
    wasm.add_layer("Layer 2", "Layer 2", 200, 200); // [Layer 2, Layer 1]

    // Undo the add (remove Layer 2) on both paths.
    const prev = hist.undo(ts.snapshot());
    if (prev) ts.restore(prev);
    wasm.restore_snapshot(preAddL2Wasm);

    expect(delegatedTo(spy, l1.id)).toBe(true);
    expect(tsLayers(ts)).toEqual(wasmLayers(wasm).map(overlap));
  });

  it("undo/redo after-delete: TS snapshot-undo graph equals wasm snapshot-restore graph", () => {
    const m = requireWasm();
    const ts = new DocumentEngine("op-ud-ts", "U", 200, 200);
    const wasm = newWasm("op-ud-w");
    const hist = new CommandHistory();
    const spy = vi.spyOn(m.DocumentEngine.prototype, "delete_layer");

    const l1 = ts.addLayer("Layer 1");
    ts.addLayer("Layer 2");
    wasm.add_layer(l1.id, "Layer 1", 200, 200);
    wasm.add_layer("Layer 2", "Layer 2", 200, 200);

    // Commit the PRE-op snapshot (before delete) on both paths.
    hist.commit(ts.snapshot()); // [Layer 2, Layer 1]
    const preDeleteWasm = wasm.snapshot_json(); // [Layer 2, Layer 1]

    ts.deleteLayer(l1.id); // [Layer 2]
    wasm.delete_layer(l1.id); // [Layer 2]

    // Undo restores the deleted layer on both paths.
    const prev = hist.undo(ts.snapshot());
    if (prev) ts.restore(prev);
    wasm.restore_snapshot(preDeleteWasm);

    expect(delegatedTo(spy, l1.id)).toBe(true);
    expect(tsLayers(ts)).toEqual(wasmLayers(wasm).map(overlap));
  });

  it("undo/redo forward transform then undo: equal forward and equal undone graphs", () => {
    const m = requireWasm();
    const ts = new DocumentEngine("op-uf-ts", "U", 200, 200);
    const wasm = newWasm("op-uf-w");
    const hist = new CommandHistory();
    const spy = vi.spyOn(m.DocumentEngine.prototype, "move_layer");

    const l1 = ts.addLayer("Layer 1");
    ts.addLayer("Layer 2");
    wasm.add_layer(l1.id, "Layer 1", 200, 200);
    wasm.add_layer("Layer 2", "Layer 2", 200, 200);

    // Commit the PRE-op snapshot (before the move) on both paths.
    hist.commit(ts.snapshot()); // [Layer 2, Layer 1 @ (0,0)]
    const preMoveWasm = wasm.snapshot_json(); // [Layer 2, Layer 1 @ (0,0)]

    // Forward: move Layer 1 on both engines.
    ts.moveLayer(l1.id, 10, 20);
    wasm.move_layer(l1.id, 10, 20);

    expect(delegatedTo(spy, l1.id)).toBe(true);
    // Forward graphs must match.
    expect(tsLayers(ts)).toEqual(wasmLayers(wasm).map(overlap));

    // Undo the move on both paths.
    const prev = hist.undo(ts.snapshot());
    if (prev) ts.restore(prev);
    wasm.restore_snapshot(preMoveWasm);

    // Undone graphs must match.
    expect(tsLayers(ts)).toEqual(wasmLayers(wasm).map(overlap));
  });
});

describe("operation parity matrix — un-armed op via mirror (TS engine only)", () => {
  it("setLayerBlendMode: TS-set value reaches the wasm graph mirror", () => {
    const m = requireWasm();
    const ts = new DocumentEngine("op-bm-ts", "B", 200, 200);
    const spy = vi.spyOn(m.DocumentEngine.prototype, "set_layer_blend_mode");

    const l = ts.addLayer("Layer 1");
    ts.setLayerBlendMode(l.id, "multiply");

    // The TS engine wrote into the wasm graph mirror (no command arm exists).
    expect(delegatedTo(spy, l.id)).toBe(true);

    const tsValue = ts.getLayer(l.id)!.blendMode;
    const mirrorValue = (wasmLayers((ts as any).rustEngine) as any[])[0].blendMode;
    // TS oracle vs mirrored native graph — proves the mirror carried the value.
    expect(tsValue).toBe("multiply");
    expect(tsValue).toBe(mirrorValue);
  });
});
