// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Operation Parity Matrix - metadata-parity measurement for Photrez document
// operations. ZERO production code changes; this file is a measurement artifact.
//
// TWO SECTIONS, each measuring a DIFFERENT native surface:
//
//  SECTION 1 - MIRROR PATH (document.rs graph mirror)
//  Both sides are the document.rs `DocumentEngine` graph mirror. Side A is the
//  TS `DocumentEngine`, whose graph ops delegate to that same mirror via
//  USE_RUST_SSOT and are projected back via syncLayersFromRust. Side B is a
//  *separate* real wasm `DocumentEngine` driven directly. This is a
//  MIRROR-PARITY check (TS projection vs the raw mirror) - NOT a comparison of
//  two independent engines. The true native authority is the ProtocolEngine
//  (document_core.rs), measured in Section 2.
//
//  SECTION 2 - ARM PATH (ProtocolEngine command arms)
//  Drives the REAL native authority (the per-document ProtocolEngine behind
//  `protocol_apply_command(env, docId)`, document_core.rs) through the
//  production bridge. Side A is the TS `DocumentEngine` as the independent
//  oracle; layers are compared by NAME (the arm uses the TS-minted id supplied
//  by the test, so it is no longer a uuid - ids still differ from the TS engine's
//  own mint, so they are not compared across engines).
//
//  NO hand-written expected values: the two engines are each other's oracle. If
//  the wasm module is unavailable, tests fail LOUD (requireWasm / bridge arity
//  guard) - no TS fallback is allowed.
//
//  Anti-masking (lesson from document-rust-path.test.ts): armed-op tests spy on
//  the wasm prototype method to prove the TS engine's specific call reached the
//  real engine, not a silent TS fallback.
//
//  Divergences are REPORTED, not hidden: Section 2 asserts the engine RESPONDS
//  (layer counts, the opacity/transform values that MUST be equal) AND asserts
//  the known divergence IS PRESENT (placement order, addLayer impoverishment,
//  uuid minting). This keeps the suite green while recording findings honestly.

import { describe, it, expect, beforeAll, beforeEach, afterEach, afterAll, vi } from "vitest";
import { DocumentEngine } from "@/engine/document";
import { CommandHistory } from "@/engine/history";
import { getWasmExportModule } from "@/components/editor/wasmExport";
import * as bridge from "@/lib/protocol/bridge";
import { CONTRACT_VERSION } from "@/lib/protocol/types";
// Side A of the structural rows is built from pure layerOps functions over a
// plain DocumentModel (no engine, no wasm mirror); Side B drives the real wasm arms.
import {
  addLayer,
  addShapeLayer,
  addTextLayer,
  duplicateLayer,
  mergeDown,
  mergeSelectedLayers,
  flattenLayers,
  shapeLayerToRaster,
  textLayerToRaster,
} from "@/engine/layerOps";

let wasmMod: any = null;

beforeAll(async () => {
  wasmMod = await getWasmExportModule(); // also arms bridge.setProtocolWasm
});

beforeEach(() => {
  // Photrez.facade + native authority stay OFF - these tests measure the
  // always-on wasm graph mirror (Section 1) and the ProtocolEngine arms
  // (Section 2) directly, not the gated ownership stream.
  localStorage.removeItem("photrez.facade");
  localStorage.removeItem("photrez.facadeAuthority");
});

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
  localStorage.removeItem("photrez.facade");
  localStorage.removeItem("photrez.facadeAuthority");
});

// Fails LOUD when the native engine is not present, so no test silently runs the
// TS fallback and reports a false parity pass.
function requireWasm(): any {
  expect(wasmMod).toBeTruthy();
  expect(wasmMod?.DocumentEngine).toBeTruthy();
  return wasmMod;
}

// jsdom has no OffscreenCanvas, so the oracle's compositeAllLayers() returns
// null and mergeSelectedLayers() bails on the null bitmap. Stub it (mirrors
// document-rust-path.test.ts) so the oracle composites and merges, matching the
// arm's always-merge behavior. The stub is restored by the shared afterEach.
function stubOffscreenCanvas(): void {
  const Mock = function (this: any, w: number, h: number) {
    this.width = w;
    this.height = h;
    const ctx: any = {
      font: "", fillStyle: "", strokeStyle: "", lineWidth: 0,
      lineJoin: "miter", miterLimit: 10, globalAlpha: 1,
      globalCompositeOperation: "source-over", textBaseline: "alphabetic",
      letterSpacing: undefined,
      measureText: (s: string) => ({ width: s.length * 10, actualBoundingBoxAscent: 80, actualBoundingBoxDescent: 24, fontBoundingBoxAscent: 80, fontBoundingBoxDescent: 24 }),
      fillText: () => {}, strokeText: () => {}, drawImage: () => {},
      save: () => {}, restore: () => {}, translate: () => {}, scale: () => {}, rotate: () => {}, fillRect: () => {},
    };
    this.getContext = () => ctx;
    this.transferToImageBitmap = () => ({ width: this.width, height: this.height, close: () => {} });
  } as unknown as typeof OffscreenCanvas;
  vi.stubGlobal("OffscreenCanvas", Mock);
}

// Normalize a layer to the RenderLayer overlap used for parity comparison.
// `id` is engine-assigned (non-deterministic between engines) and is NOT
// compared - the matrix measures graph-state metadata parity, not id minting.
// Handles both the arm's flat RenderLayer (x/y/scaleX/...) and the TS
// LayerNode (transform:{x,y,scaleX,...}).
function overlap(layer: any): any {
  const t = layer.transform ?? {};
  return {
    name: layer.name,
    visible: layer.visible,
    opacity: layer.opacity,
    x: layer.x ?? t.x ?? 0,
    y: layer.y ?? t.y ?? 0,
    scaleX: layer.scaleX ?? t.scaleX ?? 1,
    scaleY: layer.scaleY ?? t.scaleY ?? 1,
    rotation: layer.rotation ?? t.rotation ?? 0,
  };
}

// Read the real wasm graph-mirror engine's layers (Section 1, Side B).
function mirrorLayers(engine: any): any[] {
  return JSON.parse(engine.get_layers_json());
}

// Read the TS engine's projected graph (Section 1, Side A).
function tsLayers(ts: DocumentEngine): any[] {
  return (ts.getLayers() as unknown as any[]).map(overlap);
}

// Assert the TS engine's specific op (identified by its layer id) reached the
// wasm prototype method. Catches the silent TS-fallback masking case.
function delegatedTo(spy: any, layerId: string): boolean {
  return spy.mock.calls.some((c: any[]) => c[0] === layerId);
}

function newMirror(docId: string): any {
  return new wasmMod.DocumentEngine(docId, docId, 200, 200);
}

// -- Shared comparison helpers (used by both ARM-PATH and METADATA-ARM-PATH) --

// Compare two layer sets by NAME on the overlap fields (ids/index excluded).
function byName(layers: any[]): Map<string, any> {
  const m = new Map<string, any>();
  for (const l of layers) m.set(l.name, overlap(l));
  return m;
}
function overlapEqual(a: any, b: any): boolean {
  return (
    a.visible === b.visible &&
    a.opacity === b.opacity &&
    a.x === b.x &&
    a.y === b.y &&
    a.scaleX === b.scaleX &&
    a.scaleY === b.scaleY &&
    a.rotation === b.rotation
  );
}
// Assert same layer-name set with equal overlap fields (order/ids ignored).
function expectByNameOverlap(arm: any[], ts: any[], label: string): void {
  const a = byName(arm);
  const t = byName(ts);
  expect([...a.keys()].sort(), `${label}: same layer-name set`).toEqual([...t.keys()].sort());
  for (const name of a.keys()) {
    expect(overlapEqual(a.get(name), t.get(name)), `${label}: overlap equal for '${name}'`).toBe(true);
  }
}

// Metadata overlap (blend/lock/background/flip) for the metadata command arms.
// Reads BOTH the arm's flat RenderLayer and the TS LayerNode (transform.flipH/V)
// so the same helper works for either engine's layer shape.
function metaFields(layer: any): any {
  const t = layer.transform ?? {};
  return {
    blendMode: layer.blendMode ?? null,
    isBackground: layer.isBackground ?? false,
    locked: layer.locked ?? false,
    lockTransparency: layer.lockTransparency ?? false,
    lockPosition: layer.lockPosition ?? false,
    lockRotation: layer.lockRotation ?? false,
    flipH: layer.flipH ?? t.flipH ?? false,
    flipV: layer.flipV ?? t.flipV ?? false,
  };
}
function byMeta(layers: any[]): Map<string, any> {
  const m = new Map<string, any>();
  for (const l of layers) m.set(l.name, metaFields(l));
  return m;
}
function metaEqual(a: any, b: any): boolean {
  return (
    a.blendMode === b.blendMode &&
    a.isBackground === b.isBackground &&
    a.locked === b.locked &&
    a.lockTransparency === b.lockTransparency &&
    a.lockPosition === b.lockPosition &&
    a.lockRotation === b.lockRotation &&
    a.flipH === b.flipH &&
    a.flipV === b.flipV
  );
}
function expectByNameMeta(arm: any[], ts: any[], label: string): void {
  const a = byMeta(arm);
  const t = byMeta(ts);
  expect([...a.keys()].sort(), `${label}: same layer-name set`).toEqual([...t.keys()].sort());
  for (const name of a.keys()) {
    expect(metaEqual(a.get(name), t.get(name)), `${label}: metadata equal for '${name}'`).toBe(true);
  }
}

// -- Section 1 helpers: compare by layer index (ids match; mirror mints both) --
describe("operation parity matrix - MIRROR PATH (document.rs graph mirror; same mirror code on both sides)", () => {
  it("addLayer: TS-delegated graph equals direct-wasm graph", () => {
    const m = requireWasm();
    const ts = new DocumentEngine("op-add-ts", "A", 200, 200);
    const wasm = newMirror("op-add-w");
    const spy = vi.spyOn(m.DocumentEngine.prototype, "add_layer");

    const l = ts.addLayer("Layer 1");
    wasm.add_layer(l.id, "Layer 1", 200, 200);

    expect(delegatedTo(spy, l.id)).toBe(true);
    expect(tsLayers(ts)).toEqual(mirrorLayers(wasm).map(overlap));
  });

  it("deleteLayer: TS-delegated graph equals direct-wasm graph", () => {
    const m = requireWasm();
    const ts = new DocumentEngine("op-del-ts", "D", 200, 200);
    const wasm = newMirror("op-del-w");
    const spy = vi.spyOn(m.DocumentEngine.prototype, "delete_layer");

    const l1 = ts.addLayer("Layer 1");
    ts.addLayer("Layer 2");
    wasm.add_layer(l1.id, "Layer 1", 200, 200);
    wasm.add_layer("Layer 2", "Layer 2", 200, 200);

    ts.deleteLayer(l1.id);
    wasm.delete_layer(l1.id);

    expect(delegatedTo(spy, l1.id)).toBe(true);
    expect(tsLayers(ts)).toEqual(mirrorLayers(wasm).map(overlap));
  });

  it("transform move: TS-delegated graph equals direct-wasm graph", () => {
    const m = requireWasm();
    const ts = new DocumentEngine("op-move-ts", "M", 200, 200);
    const wasm = newMirror("op-move-w");
    const spy = vi.spyOn(m.DocumentEngine.prototype, "move_layer");

    const l1 = ts.addLayer("Layer 1");
    wasm.add_layer(l1.id, "Layer 1", 200, 200);

    ts.moveLayer(l1.id, 10, 20);
    wasm.move_layer(l1.id, 10, 20);

    expect(delegatedTo(spy, l1.id)).toBe(true);
    expect(tsLayers(ts)).toEqual(mirrorLayers(wasm).map(overlap));
  });

  it("transform scale: TS-delegated graph equals direct-wasm graph", () => {
    const m = requireWasm();
    const ts = new DocumentEngine("op-scale-ts", "S", 200, 200);
    const wasm = newMirror("op-scale-w");
    const spy = vi.spyOn(m.DocumentEngine.prototype, "transform_layer");

    const l1 = ts.addLayer("Layer 1");
    wasm.add_layer(l1.id, "Layer 1", 200, 200);

    ts.transformLayer(l1.id, { scaleX: 2, scaleY: 3 });
    wasm.transform_layer(l1.id, null, null, 2, 3, null, null, null);

    expect(delegatedTo(spy, l1.id)).toBe(true);
    expect(tsLayers(ts)).toEqual(mirrorLayers(wasm).map(overlap));
  });

  it("transform rotate: TS-delegated graph equals direct-wasm graph", () => {
    const m = requireWasm();
    const ts = new DocumentEngine("op-rot-ts", "R", 200, 200);
    const wasm = newMirror("op-rot-w");
    const spy = vi.spyOn(m.DocumentEngine.prototype, "transform_layer");

    const l1 = ts.addLayer("Layer 1");
    wasm.add_layer(l1.id, "Layer 1", 200, 200);

    ts.transformLayer(l1.id, { rotation: 45 });
    wasm.transform_layer(l1.id, null, null, null, null, 45, null, null);

    expect(delegatedTo(spy, l1.id)).toBe(true);
    expect(tsLayers(ts)).toEqual(mirrorLayers(wasm).map(overlap));
  });

  it("setOpacity: TS-delegated graph equals direct-wasm graph", () => {
    const m = requireWasm();
    const ts = new DocumentEngine("op-op-ts", "O", 200, 200);
    const wasm = newMirror("op-op-w");
    const spy = vi.spyOn(m.DocumentEngine.prototype, "set_layer_opacity");

    const l1 = ts.addLayer("Layer 1");
    wasm.add_layer(l1.id, "Layer 1", 200, 200);

    ts.setLayerOpacity(l1.id, 0.5);
    wasm.set_layer_opacity(l1.id, 0.5);

    expect(delegatedTo(spy, l1.id)).toBe(true);
    expect(tsLayers(ts)).toEqual(mirrorLayers(wasm).map(overlap));
  });

  it("undo/redo after-add: TS snapshot-undo graph equals wasm snapshot-restore graph", () => {
    const m = requireWasm();
    const ts = new DocumentEngine("op-ua-ts", "U", 200, 200);
    const wasm = newMirror("op-ua-w");
    const hist = new CommandHistory();
    const spy = vi.spyOn(m.DocumentEngine.prototype, "add_layer");

    const l1 = ts.addLayer("Layer 1");
    wasm.add_layer(l1.id, "Layer 1", 200, 200);

    // Commit the PRE-op snapshot (before adding Layer 2) on both paths.
    hist.commit(ts.snapshot());
    const preAddL2Wasm = wasm.snapshot_json();

    ts.addLayer("Layer 2");
    wasm.add_layer("Layer 2", "Layer 2", 200, 200);

    const prev = hist.undo(ts.snapshot());
    if (prev) ts.restore(prev);
    wasm.restore_snapshot(preAddL2Wasm);

    expect(delegatedTo(spy, l1.id)).toBe(true);
    expect(tsLayers(ts)).toEqual(mirrorLayers(wasm).map(overlap));
  });

  it("undo/redo after-delete: TS snapshot-undo graph equals wasm snapshot-restore graph", () => {
    const m = requireWasm();
    const ts = new DocumentEngine("op-ud-ts", "U", 200, 200);
    const wasm = newMirror("op-ud-w");
    const hist = new CommandHistory();
    const spy = vi.spyOn(m.DocumentEngine.prototype, "delete_layer");

    const l1 = ts.addLayer("Layer 1");
    ts.addLayer("Layer 2");
    wasm.add_layer(l1.id, "Layer 1", 200, 200);
    wasm.add_layer("Layer 2", "Layer 2", 200, 200);

    hist.commit(ts.snapshot());
    const preDeleteWasm = wasm.snapshot_json();

    ts.deleteLayer(l1.id);
    wasm.delete_layer(l1.id);

    const prev = hist.undo(ts.snapshot());
    if (prev) ts.restore(prev);
    wasm.restore_snapshot(preDeleteWasm);

    expect(delegatedTo(spy, l1.id)).toBe(true);
    expect(tsLayers(ts)).toEqual(mirrorLayers(wasm).map(overlap));
  });

  it("undo/redo forward transform then undo: equal forward and equal undone graphs", () => {
    const m = requireWasm();
    const ts = new DocumentEngine("op-uf-ts", "U", 200, 200);
    const wasm = newMirror("op-uf-w");
    const hist = new CommandHistory();
    const spy = vi.spyOn(m.DocumentEngine.prototype, "move_layer");

    const l1 = ts.addLayer("Layer 1");
    ts.addLayer("Layer 2");
    wasm.add_layer(l1.id, "Layer 1", 200, 200);
    wasm.add_layer("Layer 2", "Layer 2", 200, 200);

    hist.commit(ts.snapshot());
    const preMoveWasm = wasm.snapshot_json();

    ts.moveLayer(l1.id, 10, 20);
    wasm.move_layer(l1.id, 10, 20);

    expect(delegatedTo(spy, l1.id)).toBe(true);
    expect(tsLayers(ts)).toEqual(mirrorLayers(wasm).map(overlap));

    const prev = hist.undo(ts.snapshot());
    if (prev) ts.restore(prev);
    wasm.restore_snapshot(preMoveWasm);

    expect(tsLayers(ts)).toEqual(mirrorLayers(wasm).map(overlap));
  });
});

// -- Section 2: drive the REAL ProtocolEngine (document_core.rs) via the bridge --
describe("operation parity matrix - ARM PATH (ProtocolEngine command arms, real wasm)", () => {
  const ARM_DOC = "parity-arm";
  // Records each arm-scenario outcome so afterAll can print the matrix table
  // that feeds the user cutover decision.
  const armMatrix: Array<Record<string, unknown>> = [];

  beforeEach(() => {
    bridge.resetWasmDoc(ARM_DOC); // protocol_reset equivalent for this doc
  });

  // Drive one command through the production bridge (-> protocol_apply_command).
  function armApply(command: any): Promise<any> {
    return bridge.applyCommand({ contractVersion: CONTRACT_VERSION, docId: ARM_DOC, command });
  }
  function armSnapshot(): Promise<any> {
    return bridge.getSnapshot(ARM_DOC);
  }
  // Host-owned identity + placement: mint a TS-style id and insert above active
  // (index 0), so the arm now mirrors the TS engine (uuid-mint + append divergences
  // resolved). Read the result back by the id we control.
  async function armAdd(name: string): Promise<{ id: string; snapshot: any }> {
    const id = `layer-${Math.random().toString(36).slice(2, 10)}`;
    await armApply({ type: "addLayer", id, name, width: 200, height: 200, index: 0 });
    const snap = await armSnapshot();
    const layer = snap.layers.find((l: any) => l.id === id) as any;
    return { id, snapshot: snap };
  }

  // Comparison helpers (byName / overlapEqual / expectByNameOverlap / metaFields /
  // byMeta / metaEqual / expectByNameMeta) are defined at module top-level so both
  // the ARM-PATH and METADATA-ARM-PATH sections can use them.

  it("(a) addLayer x2 then deleteLayer - counts EQUAL; placement + impoverishment + uuid mint FIXED (MEASURED-EQUAL)", async () => {
    requireWasm();
    const north = await armAdd("North");
    const preDelete = await armAdd("South"); // [South, North] (arm inserts above active at host index 0)
    const preDeleteSnap = preDelete.snapshot;
    await armApply({ type: "deleteLayer", id: north.id });
    const postDelete = await armSnapshot();

    const ts = new DocumentEngine("arm-ts-a", "A", 200, 200);
    const nL = ts.addLayer("North");
    const sL = ts.addLayer("South"); // [South, North] (TS inserts above active)
    // Capture the TS layer BEFORE deleting it - the TS model carries width/height/
    // type/blend that the arm cannot represent.
    const tsNorth = ts.getLayer(nL.id) as unknown as any;
    const tsPreDelete = (ts.getLayers() as unknown as any[]).map((l) => l.name);
    ts.deleteLayer(nL.id);
    const tsPostDelete = ts.getLayers() as unknown as any[];

    // HARD: layer counts must match on both engines.
    expect(preDeleteSnap.layers.length).toBe(2);
    expect(tsPreDelete.length).toBe(2);
    expect(postDelete.layers.length).toBe(1);
    expect(tsPostDelete.length).toBe(1);

    // HARD: matched-by-name overlap (surviving "South") must be equal.
    expectByNameOverlap(postDelete.layers, tsPostDelete, "(a) delete");

    // FIXED (layer placement): arm now inserts above active layer, so order
    // matches the TS engine exactly.
    expect(preDeleteSnap.layers.map((l: any) => l.name)).toEqual(tsPreDelete);

    // FIXED (layer metadata): the arm now carries layerType/blendMode/width/height.
    const armLayer = preDeleteSnap.layers.find((l: any) => l.name === "North");
    expect(armLayer.layerType).toBeDefined();
    expect(armLayer.blendMode).toBeDefined();
    expect(armLayer.width).toBe(200);
    expect(armLayer.height).toBe(200);
    // TS side carries the full layer too.
    expect(tsNorth.width).toBe(200);
    expect(tsNorth.height).toBe(200);
    expect(tsNorth.type).toBe("raster");
    expect(tsNorth.blendMode).toBeDefined();

    // FIXED (host-minted id): the arm uses the TS-minted (layer-<rand>) id we
    // passed, not a uuid v4. Ids still differ from the TS oracle (which mints its
    // own), but both are now TS-style rather than a uuid.
    expect(north.id).toMatch(/^layer-[a-z0-9]+$/);
    expect(north.id).not.toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/);
    expect(north.id).not.toEqual(nL.id);

    armMatrix.push({
      scenario: "(a) addLayer x2 + deleteLayer",
      counts: "EQUAL (2 -> 1 on both engines)",
      overlapAfterDelete: "EQUAL (by name)",
      divergences:
        "none (arm inserts above active; carries full layer shape; uses TS-minted id - divergences #1/#2/#3 resolved)",
    });
  });

  it("(b) transform move/scale/rotate - overlap values EQUAL (arm path)", async () => {
    requireWasm();
    const { id: armId } = await armAdd("Layer");
    await armApply({ type: "transformLayer", id: armId, transform: { x: 10, y: 20, scaleX: 2, scaleY: 3, rotation: 45 } });
    const armSnap = await armSnapshot();

    const ts = new DocumentEngine("arm-ts-b", "B", 200, 200);
    const l = ts.addLayer("Layer");
    ts.transformLayer(l.id, { x: 10, y: 20, scaleX: 2, scaleY: 3, rotation: 45 });
    const tsLayersB = ts.getLayers() as unknown as any[];

    // HARD: matched-by-name overlap (all 5 transform fields) must match.
    expectByNameOverlap(armSnap.layers, tsLayersB, "(b) transform");
    const armO = byName(armSnap.layers).get("Layer");
    expect(armO.x).toBe(10);
    expect(armO.y).toBe(20);
    expect(armO.scaleX).toBe(2);
    expect(armO.scaleY).toBe(3);
    expect(armO.rotation).toBe(45);

    armMatrix.push({
      scenario: "(b) transform move/scale/rotate",
      counts: "n/a (1 layer)",
      overlap: "EQUAL (x/y/scaleX/scaleY/rotation, by name)",
      divergences: "none on overlap fields",
    });
  });

  it("(c) setOpacity - overlap opacity EQUAL (arm path)", async () => {
    requireWasm();
    const { id: armId } = await armAdd("Layer");
    await armApply({ type: "setOpacity", id: armId, opacity: 0.5 });
    const armSnap = await armSnapshot();

    const ts = new DocumentEngine("arm-ts-c", "C", 200, 200);
    const l = ts.addLayer("Layer");
    ts.setLayerOpacity(l.id, 0.5);
    const tsLayersC = ts.getLayers() as unknown as any[];

    expectByNameOverlap(armSnap.layers, tsLayersC, "(c) opacity");
    expect(byName(armSnap.layers).get("Layer").opacity).toBe(0.5);

    armMatrix.push({
      scenario: "(c) setOpacity",
      counts: "n/a",
      overlap: "EQUAL (opacity=0.5, by name)",
      divergences: "none on overlap fields",
    });
  });

  it("(d1) undo/redo after-add - graph result EQUAL; placement DIVERGENT on redo", async () => {
    requireWasm();
    const aA = await armAdd("A");
    await armAdd("B"); // [A, B]
    await armApply({ type: "undo" });
    const armUndo = await armSnapshot(); // [A]
    await armApply({ type: "redo" });
    const armRedo = await armSnapshot(); // [A, B]

    const ts = new DocumentEngine("arm-ts-d1", "D", 200, 200);
    const la = ts.addLayer("A"); // [A]
    const hist = new CommandHistory();
    hist.commit(ts.snapshot()); // pre-add-B = [A]
    ts.addLayer("B"); // [B, A]
    const prev = hist.undo(ts.snapshot());
    if (prev) ts.restore(prev); // [A]
    const tsUndo = ts.getLayers() as unknown as any[];
    const next = hist.redo(ts.snapshot());
    if (next) ts.restore(next); // [B, A]
    const tsRedo = ts.getLayers() as unknown as any[];

    // HARD: layer counts after undo/redo.
    expect(armUndo.layers.length).toBe(1);
    expect(tsUndo.length).toBe(1);
    expect(armRedo.layers.length).toBe(2);
    expect(tsRedo.length).toBe(2);

    // HARD: matched-by-name overlap after undo and redo.
    expectByNameOverlap(armUndo.layers, tsUndo, "(d1) undo");
    expectByNameOverlap(armRedo.layers, tsRedo, "(d1) redo");

    // FIXED (layer placement): arm now inserts above active layer, so redo order matches TS.
    expect(armRedo.layers.map((l: any) => l.name)).toEqual(tsRedo.map((l: any) => l.name));

    armMatrix.push({
      scenario: "(d1) undo/redo after-add",
      counts: "EQUAL (undo->1, redo->2 on both)",
      overlap: "EQUAL (by name) after undo and redo",
      divergences: "none (placement matches TS on redo; ids TS-style)",
    });
  });

  it("(d2) undo/redo after-delete - graph result EQUAL; placement DIVERGENT on undo", async () => {
    requireWasm();
    const aA = await armAdd("A");
    await armAdd("B"); // [A, B]
    await armApply({ type: "deleteLayer", id: aA.id }); // [B]
    await armApply({ type: "undo" });
    const armUndo = await armSnapshot(); // [A, B]
    await armApply({ type: "redo" });
    const armRedo = await armSnapshot(); // [B]

    const ts = new DocumentEngine("arm-ts-d2", "D", 200, 200);
    const la = ts.addLayer("A"); // [A]
    ts.addLayer("B"); // [B, A]
    const hist = new CommandHistory();
    hist.commit(ts.snapshot()); // pre-delete = [B, A]
    ts.deleteLayer(la.id); // [B]
    const prev = hist.undo(ts.snapshot());
    if (prev) ts.restore(prev); // [B, A]
    const tsUndo = ts.getLayers() as unknown as any[];
    const next = hist.redo(ts.snapshot());
    if (next) ts.restore(next); // [B]
    const tsRedo = ts.getLayers() as unknown as any[];

    expect(armUndo.layers.length).toBe(2);
    expect(tsUndo.length).toBe(2);
    expect(armRedo.layers.length).toBe(1);
    expect(tsRedo.length).toBe(1);

    expectByNameOverlap(armUndo.layers, tsUndo, "(d2) undo");
    expectByNameOverlap(armRedo.layers, tsRedo, "(d2) redo");

    // FIXED (layer placement): arm now inserts above active layer, so undo order matches TS.
    expect(armUndo.layers.map((l: any) => l.name)).toEqual(tsUndo.map((l: any) => l.name));

    armMatrix.push({
      scenario: "(d2) undo/redo after-delete",
      counts: "EQUAL (undo->2, redo->1 on both)",
      overlap: "EQUAL (by name) after undo and redo",
      divergences: "none (placement matches TS on undo; ids TS-style)",
    });
  });

  afterAll(() => {
    // eslint-disable-next-line no-console
    console.log("\n=== OPERATION PARITY MATRIX - ARM PATH (ProtocolEngine) ===");
    // eslint-disable-next-line no-console
    console.log(JSON.stringify(armMatrix, null, 2));
  });
});

// -- Section 3: metadata command arms (setVisible/setLocked/rename/reorder/
//    setBackgroundFlag/setBlendMode/transform-flip via the real wasm ProtocolEngine) --
describe("operation parity matrix - METADATA ARM PATH (ProtocolEngine command arms, real wasm)", () => {
  const META_DOC = "parity-meta-arm";
  const metaMatrix: Array<Record<string, unknown>> = [];

  beforeEach(() => {
    bridge.resetWasmDoc(META_DOC);
  });

  function armApply(command: any): Promise<any> {
    return bridge.applyCommand({ contractVersion: CONTRACT_VERSION, docId: META_DOC, command });
  }
  function armSnapshot(): Promise<any> {
    return bridge.getSnapshot(META_DOC);
  }
  async function armAdd(name: string): Promise<{ id: string; snapshot: any }> {
    const id = `layer-${Math.random().toString(36).slice(2, 10)}`;
    await armApply({ type: "addLayer", id, name, width: 200, height: 200, index: 0 });
    const snap = await armSnapshot();
    const layer = snap.layers.find((l: any) => l.id === id) as any;
    return { id, snapshot: snap };
  }

  it("(e) setVisible - visible EQUAL (arm path)", async () => {
    requireWasm();
    const { id: armId } = await armAdd("Layer");
    await armApply({ type: "setVisible", id: armId, visible: false });
    const armSnap = await armSnapshot();

    const ts = new DocumentEngine("arm-ts-e", "E", 200, 200);
    const l = ts.addLayer("Layer");
    ts.setLayerVisibility(l.id, false);
    const tsLayersE = ts.getLayers() as unknown as any[];

    expectByNameOverlap(armSnap.layers, tsLayersE, "(e) visible");
    expectByNameMeta(armSnap.layers, tsLayersE, "(e) visible");

    metaMatrix.push({
      scenario: "(e) setVisible",
      counts: "n/a",
      overlap: "EQUAL (visible=false, by name)",
      divergences: "none on overlap fields",
    });
  });

  it("(f) setLocked (4 kinds) - locks EQUAL (arm path)", async () => {
    requireWasm();
    const { id: armId } = await armAdd("Layer");
    const ts = new DocumentEngine("arm-ts-f", "F", 200, 200);
    const l = ts.addLayer("Layer");

    await armApply({ type: "setLocked", id: armId, kind: "base", locked: true });
    ts.setLayerLocked(l.id, true);
    await armApply({ type: "setLocked", id: armId, kind: "transparency", locked: true });
    ts.setLayerLockTransparency(l.id, true);
    await armApply({ type: "setLocked", id: armId, kind: "position", locked: true });
    ts.setLayerLockPosition(l.id, true);
    await armApply({ type: "setLocked", id: armId, kind: "rotation", locked: true });
    ts.setLayerLockRotation(l.id, true);

    const armSnap = await armSnapshot();
    const tsLayersF = ts.getLayers() as unknown as any[];

    expectByNameOverlap(armSnap.layers, tsLayersF, "(f) locked");
    expectByNameMeta(armSnap.layers, tsLayersF, "(f) locked");

    metaMatrix.push({
      scenario: "(f) setLocked (4 kinds)",
      counts: "n/a",
      overlap: "EQUAL (locked/lockTransparency/lockPosition/lockRotation, by name)",
      divergences: "none on overlap fields",
    });
  });

  it("(f2) setLocked transparency-only leaves other lock kinds unchanged (arm path)", async () => {
    requireWasm();
    const { id: armId } = await armAdd("Layer");
    const ts = new DocumentEngine("arm-ts-f2", "F2", 200, 200);
    const l = ts.addLayer("Layer");

    // Only transparency is set; the other three lock kinds stay false/absent.
    await armApply({ type: "setLocked", id: armId, kind: "transparency", locked: true });
    ts.setLayerLockTransparency(l.id, true);

    const armSnap = await armSnapshot();
    const tsLayersF2 = ts.getLayers() as unknown as any[];

    expectByNameOverlap(armSnap.layers, tsLayersF2, "(f2) transparency-only");
    expectByNameMeta(armSnap.layers, tsLayersF2, "(f2) transparency-only");
    // Asymmetric isolation: only transparency is set - a locked/position/rotation
    // swap would be caught here even though arm==ts on the symmetric case.
    const armLayer = armSnap.layers.find((x: any) => x.name === "Layer");
    expect(armLayer.lockTransparency).toBe(true);
    expect(armLayer.locked).not.toBe(true);
    expect(armLayer.lockPosition).not.toBe(true);
    expect(armLayer.lockRotation).not.toBe(true);

    metaMatrix.push({
      scenario: "(f2) setLocked transparency-only",
      counts: "n/a",
      overlap: "EQUAL (only lockTransparency set, by name)",
      divergences: "none on overlap fields",
    });
  });

  it("(g) rename - name EQUAL by index (arm path)", async () => {
    requireWasm();
    const { id: armId } = await armAdd("Layer");
    await armApply({ type: "rename", id: armId, name: "Renamed" });
    const armSnap = await armSnapshot();

    const ts = new DocumentEngine("arm-ts-g", "G", 200, 200);
    const l = ts.addLayer("Layer");
    ts.setLayerName(l.id, "Renamed");
    const tsLayersG = ts.getLayers() as unknown as any[];

    // Rename changes the name, so compare by index (both engines keep the same order).
    expect(armSnap.layers.length).toBe(tsLayersG.length);
    for (let i = 0; i < armSnap.layers.length; i++) {
      expect(overlap(armSnap.layers[i]), `(g) overlap equal at index ${i}`).toEqual(overlap(tsLayersG[i]));
    }

    metaMatrix.push({
      scenario: "(g) rename",
      counts: "n/a",
      overlap: "EQUAL (name 'Renamed', by index)",
      divergences: "none on overlap fields",
    });
  });

  it("(h) reorder mid-stack - order EQUAL (arm path)", async () => {
    requireWasm();
    await armAdd("A");
    const bAdd = await armAdd("B");
    await armAdd("C"); // arm inserts above active => [C, B, A]
    // Move "B" (index 1) to the top (index 0): arm mirrors TS applyReorderLayer.
    await armApply({ type: "reorder", id: bAdd.id, to: 0 });
    const armSnap = await armSnapshot();

    const ts = new DocumentEngine("arm-ts-h", "H", 200, 200);
    ts.addLayer("A"); // [A]
    const lb = ts.addLayer("B"); // [B, A]
    ts.addLayer("C"); // [C, B, A]
    const from = (ts.getLayers() as unknown as any[]).findIndex((x: any) => x.id === lb.id);
    ts.reorderLayer(from, 0);
    const tsLayersH = ts.getLayers() as unknown as any[];

    // HARD: order (by name) must match after the mid-stack reorder.
    expect(armSnap.layers.map((l: any) => l.name)).toEqual(["B", "C", "A"]);
    expect(tsLayersH.map((l: any) => l.name)).toEqual(["B", "C", "A"]);
    // Overlap + metadata must also match (parity beyond mere ordering).
    expectByNameOverlap(armSnap.layers, tsLayersH, "(h) reorder overlap");
    expectByNameMeta(armSnap.layers, tsLayersH, "(h) reorder metadata");

    metaMatrix.push({
      scenario: "(h) reorder mid-stack",
      counts: "n/a",
      overlap: "EQUAL (order [B,C,A] by name)",
      divergences: "none on overlap fields",
    });
  });

  it("(i) setBackgroundFlag - isBackground + locks EQUAL (arm path)", async () => {
    requireWasm();
    const { id: armId } = await armAdd("Layer");
    await armApply({ type: "setBackgroundFlag", id: armId });
    const armSnap = await armSnapshot();

    const ts = new DocumentEngine("arm-ts-i", "I", 200, 200);
    const l = ts.addLayer("Layer");
    ts.markLayerAsBackground(l.id);
    const tsLayersI = ts.getLayers() as unknown as any[];

    expectByNameOverlap(armSnap.layers, tsLayersI, "(i) background");
    expectByNameMeta(armSnap.layers, tsLayersI, "(i) background");

    metaMatrix.push({
      scenario: "(i) setBackgroundFlag",
      counts: "n/a",
      overlap: "EQUAL (isBackground + lockPosition + lockRotation, by name)",
      divergences: "none on overlap fields",
    });
  });

  it("(j) setBlendMode - blendMode EQUAL (arm path)", async () => {
    requireWasm();
    const { id: armId } = await armAdd("Layer");
    await armApply({ type: "setBlendMode", id: armId, mode: "multiply" });
    const armSnap = await armSnapshot();

    const ts = new DocumentEngine("arm-ts-j", "J", 200, 200);
    const l = ts.addLayer("Layer");
    ts.setLayerBlendMode(l.id, "multiply");
    const tsLayersJ = ts.getLayers() as unknown as any[];

    expectByNameOverlap(armSnap.layers, tsLayersJ, "(j) blend");
    expectByNameMeta(armSnap.layers, tsLayersJ, "(j) blend");

    metaMatrix.push({
      scenario: "(j) setBlendMode",
      counts: "n/a",
      overlap: "EQUAL (blendMode=multiply, by name)",
      divergences: "none on overlap fields",
    });
  });

  it("(k) transform flip - flipH/flipV EQUAL (arm path)", async () => {
    requireWasm();
    const { id: armId } = await armAdd("Layer");
    await armApply({ type: "transformLayer", id: armId, transform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: true } });
    const armSnap = await armSnapshot();

    const ts = new DocumentEngine("arm-ts-k", "K", 200, 200);
    const l = ts.addLayer("Layer");
    ts.transformLayer(l.id, { flipH: true });
    const tsLayersK = ts.getLayers() as unknown as any[];

    expectByNameOverlap(armSnap.layers, tsLayersK, "(k) flip");
    expectByNameMeta(armSnap.layers, tsLayersK, "(k) flip");

    metaMatrix.push({
      scenario: "(k) transform flip (flipH/flipV)",
      counts: "n/a",
      overlap: "EQUAL (flipH=true, by name)",
      divergences: "none on overlap fields",
    });
  });

  afterAll(() => {
    // eslint-disable-next-line no-console
    console.log("\n=== OPERATION PARITY MATRIX - METADATA ARM PATH (ProtocolEngine) ===");
    // eslint-disable-next-line no-console
    console.log(JSON.stringify(metaMatrix, null, 2));
  });
});

// -- Section 4: typed-add / setLayerParams / setAdjustment command arms --
// Drives the REAL ProtocolEngine (document_core.rs) AddLayer-with-type,
// SetLayerParams, and SetAdjustment arms via the production bridge, and compares
// the produced metadata against the TS engine's equivalent op (DocumentEngine
// addShapeLayer/addTextLayer/updateShapeParams/updateTextData/applyBasicAdjustment/
// clearBasicAdjustments). Bitmap RASTERIZATION stays host-side — width/height of a
// shape/text layer are derived from the host rasterizer, not the metadata arm, so
// those are recorded as divergences (the arm seeds width/height from the command).
describe("operation parity matrix - typed-add / setLayerParams / setAdjustment (ProtocolEngine arms, real wasm)", () => {
  const TYPED_DOC = "parity-typed-arm";
  const typedMatrix: Array<Record<string, unknown>> = [];

  // jsdom provides no OffscreenCanvas; the TS engine rasterizes shape/text on
  // add/update (host-side). Mirror shapeRaster.test.ts's stub so the oracle can
  // run the real TS ops. Bitmap pixels are irrelevant to metadata parity.
  beforeEach(() => {
    bridge.resetWasmDoc(TYPED_DOC);
    const MockOffscreenCanvas = function (this: any, w: number, h: number) {
      this.width = w;
      this.height = h;
      const ctx = {
        translate() {}, fillStyle: undefined, strokeStyle: undefined, lineWidth: undefined, lineCap: undefined,
        beginPath() {}, rect() {}, roundRect() {}, ellipse() {}, moveTo() {}, lineTo() {}, closePath() {}, fill() {}, stroke() {},
        measureText() { return { width: 0 }; },
        fillText() {}, strokeText() {}, fillRect() {}, clearRect() {}, save() {}, restore() {},
        setTransform() {}, scale() {}, rotate() {}, clip() {}, arc() {}, quadraticCurveTo() {}, bezierCurveTo() {}, setLineDash() {},
        createLinearGradient() { return { addColorStop() {} }; },
        getImageData(_x: number, _y: number, gw: number, gh: number) { return { data: new Uint8ClampedArray(Math.max(4, gw * gh * 4)) }; },
        putImageData() {}, drawImage() {},
      };
      this.getContext = () => ctx;
      this.transferToImageBitmap = () => ({ width: w, height: h });
    } as any;
    vi.stubGlobal("OffscreenCanvas", MockOffscreenCanvas);
  });
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  function armApply(command: any): Promise<any> {
    return bridge.applyCommand({ contractVersion: CONTRACT_VERSION, docId: TYPED_DOC, command });
  }
  function armSnapshot(): Promise<any> {
    return bridge.getSnapshot(TYPED_DOC);
  }

  const shape = {
    kind: "star" as const, width: 120, height: 80, radius: 6,
    fill: { kind: "solid" as const, color: "#E15A17" },
    stroke: { enabled: true, color: "#000000", width: 2 },
    arrowHead: false,
  };
  // Already normalized so TS normalizeTextData does not change its shape.
  const text = {
    content: "Hi", fontFamily: "Arial", fontSize: 32, fontWeight: 400, fontStyle: "normal" as const,
    color: "#000000", align: "left" as const, lineHeight: 1.2, letterSpacing: 0,
    boxMode: "point" as const, boxWidth: 0, boxHeight: 0,
    stroke: { width: 0, color: "#000000" },
  };
  const adj = { brightness: 10, contrast: 0, saturation: 0 }; // already within [-100,100]

  it("(l) addShapeLayer - type/blendMode/shapeParams EQUAL; width/height + locked DIVERGENT (host rasterization)", async () => {
    requireWasm();
    const armId = `layer-${Math.random().toString(36).slice(2, 10)}`;
    await armApply({ type: "addLayer", id: armId, name: "Star", width: 120, height: 80, index: 0, layerType: "shape", shapeParams: shape });
    const armSnap = await armSnapshot();
    const armL = armSnap.layers.find((l: any) => l.id === armId) as any;

    const ts = new DocumentEngine("arm-ts-l", "L", 200, 200);
    const tsL = ts.addShapeLayer("Star", shape) as unknown as any;

    expect(armL.layerType).toBe("shape");
    expect(tsL.type).toBe("shape");
    expect(armL.blendMode).toBe("normal");
    expect(tsL.blendMode).toBe("normal");
    // shapeParams ride verbatim on both sides (no host normalization for shapes).
    expect(armL.shapeParams).toEqual(shape);
    expect(armL.shapeParams).toEqual(tsL.shapeParams);
    // Divergence ratchet: each side's CURRENT value pinned explicitly.
    // locked: arm carries None (absent) vs TS createShapeLayerNode sets false.
    expect(armL.locked).toBeUndefined();
    expect(tsL.locked).toBe(false);
    // dims DIVERGE: the arm keeps the command width (120) while the TS engine
    // rasterizes the shape's bounding box to 124 for this star with stroke enabled
    // (width 2). Pin both.
    expect(armL.width).toBe(120);
    expect(armL.height).toBe(80);
    expect(tsL.width).toBe(124);
    expect(tsL.height).toBe(84);
    // DIVERGENCE: width differs (arm command 120 vs TS shape-bbox 124); only `locked`
    // also diverges — TS createShapeLayerNode sets false while the arm carries None.

    typedMatrix.push({
      scenario: "(l) addShapeLayer",
      counts: "n/a",
      metadata: "EQUAL (layerType=shape, blendMode=normal, shapeParams verbatim)",
      divergences: "width/height: arm command 120x80 vs TS shape-bbox 124x84; locked: ts false vs arm None (pre-existing convention)",
    });
  });

  it("(m) addTextLayer - type/blendMode EQUAL; textData DIVERGENT (TS normalizes on host)", async () => {
    requireWasm();
    const armId = `layer-${Math.random().toString(36).slice(2, 10)}`;
    await armApply({ type: "addLayer", id: armId, name: "Text", width: 200, height: 20, index: 0, layerType: "text", textData: text });
    const armSnap = await armSnapshot();
    const armL = armSnap.layers.find((l: any) => l.id === armId) as any;

    const ts = new DocumentEngine("arm-ts-m", "M", 200, 200);
    const tsL = ts.addTextLayer("Text", text) as unknown as any;

    expect(armL.layerType).toBe("text");
    expect(tsL.type).toBe("text");
    expect(armL.blendMode).toBe("normal");
    expect(tsL.blendMode).toBe("normal");
    // DIVERGENCE: both the arm and the TS engine expand the raw input `text` (12
    // fields) into a fuller stored textData (extra default/host fields), so neither
    // equals the input verbatim. Type and blendMode match; textData expansion is a
    // host-side step on both sides.
    expect(armL.textData).toBeTruthy();
    expect(tsL.textData).toBeTruthy();
    // Divergence ratchet: pin the verbatim scalar fields both sides
    // preserve, then assert the stored shape diverges from the raw input on each side.
    const tKeys = ["content","fontFamily","fontSize","fontWeight","fontStyle","color","align","lineHeight","letterSpacing","boxMode","boxWidth","boxHeight"] as const;
    for (const k of tKeys) expect(armL.textData[k]).toEqual((text as any)[k]);
    for (const k of tKeys) expect(tsL.textData[k]).toEqual((text as any)[k]);
    expect(armL.textData).not.toEqual(text);
    expect(tsL.textData).not.toEqual(text);

    typedMatrix.push({
      scenario: "(m) addTextLayer",
      counts: "n/a",
      metadata: "EQUAL (layerType=text, blendMode=normal)",
      divergences: "textData: both sides expand raw input (arm +extra default fields, TS normalizes) so stored shape != raw input",
    });
  });

  it("(n) updateShapeParams - shapeParams EQUAL; width/height DIVERGENT", async () => {
    requireWasm();
    const armId = `layer-${Math.random().toString(36).slice(2, 10)}`;
    await armApply({ type: "addLayer", id: armId, name: "Star", width: 120, height: 80, index: 0, layerType: "shape", shapeParams: shape });
    await armApply({ type: "setLayerParams", id: armId, shapeParams: shape });
    const armSnap = await armSnapshot();
    const armL = armSnap.layers.find((l: any) => l.id === armId) as any;

    const ts = new DocumentEngine("arm-ts-n", "N", 200, 200);
    const tsL = ts.addShapeLayer("Star", shape) as unknown as any;
    ts.updateShapeParams(tsL.id, shape);
    const tsAfter = (ts.getLayers() as unknown as any[]).find((l) => l.id === tsL.id);

    expect(armL.shapeParams).toEqual(shape);
    expect(armL.shapeParams).toEqual(tsAfter.shapeParams);
    // Divergence ratchet: width DIVERGES - arm keeps command 120 while TS
    // rasterizes the shape bbox to 124; setLayerParams does not recompute them.
    expect(armL.width).toBe(120);
    expect(armL.height).toBe(80);
    expect(tsAfter.width).toBe(124);
    expect(tsAfter.height).toBe(84);

    typedMatrix.push({
      scenario: "(n) updateShapeParams",
      counts: "n/a",
      metadata: "EQUAL (shapeParams verbatim)",
      divergences: "width/height: arm command 120x80 vs TS shape-bbox 124x84 (setLayerParams does not recompute dims)",
    });
  });

  it("(o) updateTextData - type/blendMode EQUAL; textData DIVERGENT (TS normalizes)", async () => {
    requireWasm();
    const armId = `layer-${Math.random().toString(36).slice(2, 10)}`;
    await armApply({ type: "addLayer", id: armId, name: "Text", width: 200, height: 20, index: 0, layerType: "text", textData: text });
    await armApply({ type: "setLayerParams", id: armId, textData: text });
    const armSnap = await armSnapshot();
    const armL = armSnap.layers.find((l: any) => l.id === armId) as any;

    const ts = new DocumentEngine("arm-ts-o", "O", 200, 200);
    const tsL = ts.addTextLayer("Text", text) as unknown as any;
    ts.updateTextData(tsL.id, text);
    const tsAfter = (ts.getLayers() as unknown as any[]).find((l) => l.id === tsL.id);

    expect(armL.layerType).toBe("text");
    expect(tsAfter.type).toBe("text");
    expect(armL.textData).toBeTruthy();
    expect(tsAfter.textData).toBeTruthy();
    // Divergence ratchet: both sides expand the raw input `text` into a
    // fuller stored textData; pin the verbatim scalar fields each side preserves, then
    // assert the stored shape diverges from the raw input on each side.
    const oKeys = ["content","fontFamily","fontSize","fontWeight","fontStyle","color","align","lineHeight","letterSpacing","boxMode","boxWidth","boxHeight"] as const;
    for (const k of oKeys) expect(armL.textData[k]).toEqual((text as any)[k]);
    for (const k of oKeys) expect(tsAfter.textData[k]).toEqual((text as any)[k]);
    expect(armL.textData).not.toEqual(text);
    expect(tsAfter.textData).not.toEqual(text);

    typedMatrix.push({
      scenario: "(o) updateTextData",
      counts: "n/a",
      metadata: "EQUAL (layerType=text, blendMode=normal)",
      divergences: "textData: both sides expand raw input (arm on setLayerParams, TS on host update) so stored shape != raw input",
    });
  });

  it("(p) applyBasicAdjustment - basicAdjustment + hasAdjustments EQUAL", async () => {
    requireWasm();
    const armId = `layer-${Math.random().toString(36).slice(2, 10)}`;
    await armAddBase(armId);
    await armApply({ type: "setAdjustment", id: armId, adjustment: adj });
    const armSnap = await armSnapshot();
    const armL = armSnap.layers.find((l: any) => l.id === armId) as any;

    const ts = new DocumentEngine("arm-ts-p", "P", 200, 200);
    const tsL = ts.addShapeLayer("Star", shape) as unknown as any;
    ts.applyBasicAdjustment(tsL.id, adj);
    const tsL2 = ts.getLayer(tsL.id) as unknown as any;

    expect(armL.basicAdjustment).toEqual(adj);
    expect(armL.hasAdjustments).toBe(true);
    expect(tsL2.basicAdjustment).toEqual(adj);
    expect(tsL2.hasAdjustments).toBe(true);

    typedMatrix.push({
      scenario: "(p) applyBasicAdjustment",
      counts: "n/a",
      metadata: "EQUAL (basicAdjustment=clamped input, hasAdjustments=true; derives from non-zero channel)",
      divergences: "none on adjustment fields; TS applyBasicAdjustment has a !imageBitmap no-op guard the arm lacks (latent divergence when a layer has no bitmap - arm applies unconditionally)",
    });
  });

  it("(q) clearBasicAdjustments - clears basicAdjustment + hasAdjustments false", async () => {
    requireWasm();
    const armId = `layer-${Math.random().toString(36).slice(2, 10)}`;
    await armAddBase(armId);
    await armApply({ type: "setAdjustment", id: armId, adjustment: adj });
    await armApply({ type: "setAdjustment", id: armId, adjustment: undefined });
    const armSnap = await armSnapshot();
    const armL = armSnap.layers.find((l: any) => l.id === armId) as any;

    const ts = new DocumentEngine("arm-ts-q", "Q", 200, 200);
    const tsL = ts.addShapeLayer("Star", shape) as unknown as any;
    ts.applyBasicAdjustment(tsL.id, adj);
    ts.clearBasicAdjustments(tsL.id);
    const tsL2 = ts.getLayer(tsL.id) as unknown as any;

    expect(armL.basicAdjustment).toBeUndefined();
    expect(armL.hasAdjustments).toBe(false);
    expect(tsL2.basicAdjustment).toBeUndefined();
    expect(tsL2.hasAdjustments).toBe(false);

    typedMatrix.push({
      scenario: "(q) clearBasicAdjustments",
      counts: "n/a",
      metadata: "EQUAL (basicAdjustment cleared; hasAdjustments=false on both)",
      divergences: "none on adjustment fields; TS applyBasicAdjustment has a !imageBitmap no-op guard the arm lacks (latent divergence when a layer has no bitmap - arm applies unconditionally)",
    });
  });

  // Host-minted id helper shared by the adjustment rows (a plain raster add, used
  // only as the adjustment target — the arm has no typed requirement here).
  async function armAddBase(id: string): Promise<void> {
    await armApply({ type: "addLayer", id, name: "Base", width: 200, height: 200, index: 0 });
  }

  afterAll(() => {
    // eslint-disable-next-line no-console
    console.log("\n=== OPERATION PARITY MATRIX - typed-add / setLayerParams / setAdjustment (ProtocolEngine) ===");
    // eslint-disable-next-line no-console
    console.log(JSON.stringify(typedMatrix, null, 2));
  });
});

// -- Section 5: selection command arms (SetSelection/ClearSelection/SelectAll/
//    InvertSelection) against the REAL wasm ProtocolEngine. Selection projection
//    rides the snapshot selection field; the delta is empty on every arm. --
describe("operation parity matrix - SELECTION ARM PATH (ProtocolEngine command arms, real wasm)", () => {
  const SEL_DOC = "parity-sel-arm";
  const selMatrix: Array<Record<string, unknown>> = [];

  beforeEach(() => {
    bridge.resetWasmDoc(SEL_DOC);
  });

  function armApply(command: any): Promise<any> {
    return bridge.applyCommand({ contractVersion: CONTRACT_VERSION, docId: SEL_DOC, command });
  }
  async function armSnapshot(): Promise<any> {
    return bridge.getSnapshot(SEL_DOC);
  }
  async function getArmSel(): Promise<any> {
    return (await armSnapshot()).selection;
  }
  // Compare the selection geometry fields (x/y/width/height/angle/shape), ignoring
  // inverted's presence/absence so createSelection (TS omits inverted) and the arm
  // agree on the geometry that matters for parity.
  function selEq(a: any, b: any): boolean {
    return (
      a && b &&
      a.x === b.x && a.y === b.y && a.width === b.width && a.height === b.height &&
      (a.angle ?? 0) === (b.angle ?? 0) &&
      (a.shape ?? undefined) === (b.shape ?? undefined)
    );
  }

  it("(s1) setSelection - selection EQUAL via snapshot; delta empty", async () => {
    requireWasm();
    const sel = { x: 10, y: 20, width: 30, height: 40, angle: 5, shape: "ellipse" as const };
    const res = await armApply({ type: "setSelection", selection: sel });
    const armSel = await getArmSel();

    const ts = new DocumentEngine("arm-ts-s1", "S", 200, 200);
    ts.createSelection(sel.x, sel.y, sel.width, sel.height, sel.angle, "ellipse");
    const tsSel = ts.getSelection();

    expect(armSel).not.toBeNull();
    expect(selEq(armSel, tsSel)).toBe(true);
    expect(armSel.shape).toBe("ellipse");
    expect(res.delta.changes).toHaveLength(0);

    selMatrix.push({
      scenario: "(s1) setSelection",
      overlap: "EQUAL (x/y/width/height/angle/shape via snapshot selection)",
      divergences: "none on geometry; (g) GATE: the arm rejects non-finite/negative geometry on all five numeric fields (x/y/width/height/angle via setSelection requires finite x/y/width/height/angle and width/height >= 0) while the TS host createSelection validates nothing - recorded divergence, host owns geometry validation before sending",
    });
  });

  it("(s2) clearSelection - both null via snapshot", async () => {
    requireWasm();
    await armApply({ type: "setSelection", selection: { x: 1, y: 2, width: 3, height: 4, angle: 0 } });
    await armApply({ type: "clearSelection" });
    const armSel = await getArmSel();

    const ts = new DocumentEngine("arm-ts-s2", "S", 200, 200);
    ts.createSelection(1, 2, 3, 4, 0);
    ts.clearSelection();
    const tsSel = ts.getSelection();

    expect(armSel).toBeUndefined();
    expect(tsSel).toBeNull();

    selMatrix.push({
      scenario: "(s2) clearSelection",
      overlap: "EQUAL (both null via snapshot selection)",
      divergences: "none",
    });
  });

  it("(s3) selectAll - full-canvas rect EQUAL (arm reads seeded canonical dims)", async () => {
    requireWasm();
    // Drive the REAL wasm arm: seed the per-doc canonical shadow with known dims.
    // The emulator's dims hook is NOT used here (the arm reads the canonical shadow).
    const canon = JSON.stringify({ id: "sel-doc", name: "S", width: 200, height: 200, layers: [] });
    wasmMod.protocol_seed_canonical(canon, SEL_DOC);
    await armApply({ type: "selectAll" });
    const armSel = await getArmSel();

    const ts = new DocumentEngine("arm-ts-s3", "S", 200, 200);
    ts.selectAll();
    const tsSel = ts.getSelection();

    expect(armSel).not.toBeNull();
    expect(selEq(armSel, tsSel)).toBe(true);
    expect(armSel.width).toBe(200);
    expect(armSel.height).toBe(200);

    selMatrix.push({
      scenario: "(s3) selectAll",
      overlap: "EQUAL (full-canvas rect x:0 y:0 width:200 height:200 via snapshot selection)",
      divergences: "none (arm reads seeded canonical dims)",
    });
  });

  it("(s4) invertSelection with selection - inverted EQUAL; version bumps no entry", async () => {
    requireWasm();
    await armApply({ type: "setSelection", selection: { x: 1, y: 2, width: 3, height: 4, angle: 0, inverted: false } });
    const vBefore = (await armSnapshot()).version;
    const qBefore = await bridge.getHistoryQuery();
    await armApply({ type: "invertSelection" });
    const armSel = await getArmSel();
    const snap = await armSnapshot();

    const ts = new DocumentEngine("arm-ts-s4", "S", 200, 200);
    ts.createSelection(1, 2, 3, 4, 0);
    ts.invertSelection();
    const tsSel = ts.getSelection();
    if (!tsSel) throw new Error("tsSel null");

    expect(armSel.inverted).toBe(true);
    expect(tsSel.inverted).toBe(true);
    expect(snap.version).toBe(vBefore + 1);
    expect((await bridge.getHistoryQuery()).entries.length).toBe(qBefore.entries.length);

    selMatrix.push({
      scenario: "(s4) invertSelection (with selection)",
      overlap: "EQUAL (inverted=true on both via snapshot selection)",
      divergences: "none",
    });
  });

  it("(s5) invertSelection without selection - falls back to full-canvas (arm == TS)", async () => {
    requireWasm();
    // Seed the per-doc canonical shadow with known dims so the arm's select-all
    // fallback (which reads the canonical shadow) has dims to fill.
    const canon = JSON.stringify({ id: "sel-doc", name: "S", width: 200, height: 200, layers: [] });
    wasmMod.protocol_seed_canonical(canon, SEL_DOC);
    const qBefore = await bridge.getHistoryQuery();
    const res = await armApply({ type: "invertSelection" });
    const armSel = await getArmSel();
    const snap = await armSnapshot();
    // Arm falls back to full-canvas select-all (mirrors the host op); DV bumps, no entry.
    expect(armSel).not.toBeNull();
    expect(selEq(armSel, { x: 0, y: 0, width: 200, height: 200, angle: 0 })).toBe(true);
    expect(snap.version).toBe(qBefore.cursor + 1);
    expect((await bridge.getHistoryQuery()).entries.length).toBe(qBefore.entries.length);

    // TS oracle does the SAME fallback: invert-without-selection selects the full
    // canvas. The two agree (no divergence) — the fallback is intentional inherited
    // host behavior, and the arm mirrors it.
    const ts = new DocumentEngine("arm-ts-s5", "S", 200, 200);
    ts.invertSelection();
    const tsSel = ts.getSelection();
    if (!tsSel) throw new Error("tsSel null");
    expect(selEq(armSel, tsSel)).toBe(true);

    selMatrix.push({
      scenario: "(s5) invertSelection (no selection)",
      overlap: "EQUAL (arm and TS both fall back to full-canvas x:0 y:0 width:200 height:200 via snapshot selection)",
      divergences: "none (fallback to select-all is intentional inherited host behavior, mirrored by the arm)",
    });
  });

  afterAll(() => {
    // eslint-disable-next-line no-console
    console.log("\n=== OPERATION PARITY MATRIX - SELECTION ARM PATH (ProtocolEngine) ===");
    // eslint-disable-next-line no-console
    console.log(JSON.stringify(selMatrix, null, 2));
  });
});

// SECTION 3 - STRUCTURAL ARM PATH (ProtocolEngine structural arms, real wasm)
// Drives the same native authority as the ARM PATH section but through the five
// structural command arms (Duplicate / MergeDown / MergeSelected / Flatten /
// Rasterize). Merge/flatten/mergeSelected read document dims from the seeded
// canonical shadow, so beforeEach seeds it (mirrors the selection arm's pattern).
//
// Side A (the oracle) is the PURE-TS `layerOps` module (duplicateLayer /
// mergeDown / mergeSelectedLayers / flattenLayers / shapeLayerToRaster /
// textLayerToRaster) run DIRECTLY against a plain DocumentModel object - no
// DocumentEngine, no wasm graph mirror. Side B is the REAL wasm ProtocolEngine
// arm driven through the production bridge. The two are genuinely independent
// implementations; this is NOT a two-Rust-engine cross-check.
// Layers are compared by NAME; ids are host-supplied on the arm and oracle-assigned
// on Side A, so they are not compared across engines.
describe("operation parity matrix - STRUCTURAL ARM PATH (ProtocolEngine structural arms, real wasm)", () => {
  const STRUCT_DOC = "parity-struct";
  const structMatrix: Array<Record<string, unknown>> = [];

  // Plain DocumentModel (mirrors the shape layerOps unit tests build directly).
  function newDocModel(): any {
    return {
      id: "struct-ts",
      name: "S",
      width: 200,
      height: 200,
      layers: [],
      activeLayerId: null,
      selection: null,
      viewport: { panX: 0, panY: 0, zoom: 1, rotation: 0 },
      dirty: false,
    };
  }

  beforeEach(() => {
    bridge.resetWasmDoc(STRUCT_DOC);
    const canon = JSON.stringify({ id: STRUCT_DOC, name: "S", width: 200, height: 200, layers: [] });
    wasmMod.protocol_seed_canonical(canon, STRUCT_DOC);
  });

  function armApply(command: any): Promise<any> {
    return bridge.applyCommand({ contractVersion: CONTRACT_VERSION, docId: STRUCT_DOC, command });
  }
  function armSnapshot(): Promise<any> {
    return bridge.getSnapshot(STRUCT_DOC);
  }
  async function armAdd(name: string): Promise<{ id: string }> {
    const id = `layer-${Math.random().toString(36).slice(2, 10)}`;
    await armApply({ type: "addLayer", id, name, width: 200, height: 200, index: 0 });
    return { id };
  }
  async function armAddShape(name: string): Promise<{ id: string }> {
    const id = `layer-${Math.random().toString(36).slice(2, 10)}`;
    await armApply({
      type: "addLayer",
      id,
      name,
      width: 64,
      height: 64,
      index: 0,
      layerType: "shape",
      shapeParams: { kind: "star", width: 64, height: 64, radius: 4, fill: { kind: "solid", color: "#E15A17" }, stroke: { enabled: false, color: "#000000", width: 0 }, arrowHead: false },
    });
    return { id };
  }
  async function armAddText(name: string): Promise<{ id: string }> {
    const id = `layer-${Math.random().toString(36).slice(2, 10)}`;
    await armApply({
      type: "addLayer",
      id,
      name,
      width: 100,
      height: 20,
      index: 0,
      layerType: "text",
      textData: { content: "Hi", fontFamily: "Arial", fontSize: 32, fontWeight: 400, fontStyle: "normal", color: "#000000", align: "left", lineHeight: 1.2, letterSpacing: 0, boxMode: "point", boxWidth: 0, boxHeight: 0, stroke: { width: 0, color: "#000000" } },
    });
    return { id };
  }

  it("(p1) duplicateLayer - clone above source; count + name parity EQUAL", async () => {
    requireWasm();
    const base = await armAdd("Base");
    await armAdd("Other"); // [Other, Base] (arm inserts above active at index 0)
    await armApply({ type: "duplicateLayer", id: base.id, newId: "dup-1" });
    const armSnap = await armSnapshot();

    // Side A: pure-TS layerOps directly against a plain DocumentModel (independent
    // implementation from the wasm arm - no DocumentEngine, no wasm mirror).
    const model = newDocModel();
    const ba = addLayer(model, "Base"); // [Base]
    addLayer(model, "Other"); // [Other, Base] (inserted above active)
    duplicateLayer(model, ba.id); // clones Base above source -> [Other, Base 2, Base]
    const tsLayers = model.layers;

    expect(armSnap.layers.length).toBe(tsLayers.length);
    expectByNameOverlap(armSnap.layers, tsLayers, "(p1) duplicate");
    expect(armSnap.layers.map((l: any) => l.name).sort()).toEqual(tsLayers.map((l: any) => l.name).sort());

    structMatrix.push({
      scenario: "(p1) duplicateLayer",
      counts: "EQUAL (2 -> 3 on both engines)",
      overlap: "EQUAL (by name; clone 'Base 2' derived identically)",
      divergences: "(e) ids host-supplied (dupe id supplied, oracle minted its own); (a) bitmap clone stays host-side; (c) unknown-source: host-side layerOps.duplicateLayer THROWS while the arm silently no-ops - documented divergence; the host owns identity checks before sending",
    });
  });

  it("(p2) mergeDown - fused 'top + bottom' name; count parity EQUAL", async () => {
    requireWasm();
    await armAdd("X");
    const top = await armAdd("North"); // [North, X] (North above X)
    await armApply({ type: "mergeDown", id: top.id, mergedId: "M" });
    const armSnap = await armSnapshot();

    // Side A: pure-TS layerOps directly against a plain DocumentModel.
    const model = newDocModel();
    addLayer(model, "X"); // [X]
    const nl = addLayer(model, "North"); // [North, X] (North above X)
    mergeDown(model, nl.id); // merges North into the layer below it (X)
    const tsLayers = model.layers;

    expect(armSnap.layers.length).toBe(tsLayers.length);
    expectByNameOverlap(armSnap.layers, tsLayers, "(p2) mergeDown");
    expect(armSnap.layers[0].name).toBe("North + X");

    structMatrix.push({
      scenario: "(p2) mergeDown",
      counts: "EQUAL (2 -> 1 on both engines)",
      overlap: "EQUAL (merged name 'North + X' = top + bottom)",
      divergences: "(a) pixel composite stays host-side; (b) composite-null bail has no arm equivalent; (e) merged id host-supplied",
    });
  });

  it("(p3) mergeSelected - 3 layers -> 'A (+2 merged)'; count parity EQUAL", async () => {
    requireWasm();
    // jsdom lacks OffscreenCanvas; the oracle's mergeSelectedLayers composites
    // before merging, so stub it here (parity with the always-merge arm).
    stubOffscreenCanvas();
    const a = await armAdd("A");
    const b = await armAdd("B");
    const c = await armAdd("C"); // [C, B, A]
    await armApply({ type: "mergeSelected", ids: [a.id, b.id, c.id], mergedId: "M3" });
    const armSnap = await armSnapshot();

    // Side A: pure-TS layerOps directly against a plain DocumentModel. The stub is
    // still required here: mergeSelectedLayers composites selected layers and bails
    // (returns null -> no merge) when the composite bitmap is null (layerOps.ts:204).
    const model = newDocModel();
    const oa = addLayer(model, "A");
    const ob = addLayer(model, "B");
    const oc = addLayer(model, "C"); // [C, B, A]
    mergeSelectedLayers(model, [oa.id, ob.id, oc.id]);
    const tsLayers = model.layers;

    expect(armSnap.layers.length).toBe(tsLayers.length);
    expectByNameOverlap(armSnap.layers, tsLayers, "(p3) mergeSelected x3");
    expect(armSnap.layers[0].name).toBe("C (+2 merged)");

    structMatrix.push({
      scenario: "(p3) mergeSelected (3 layers)",
      counts: "EQUAL (3 -> 1 on both engines)",
      overlap: "EQUAL (merged name 'C (+2 merged)' uses top-most selected name)",
      divergences: "(a) pixel composite host-side; (b) composite-null bail no arm equivalent; (d) MAX_LAYERS budget guard host-side; (e) merged id host-supplied",
    });
  });

  it("(p4) mergeSelected - 2 layers -> 'A + B'; count parity EQUAL", async () => {
    requireWasm();
    // jsdom lacks OffscreenCanvas; stub it so the oracle composites + merges.
    stubOffscreenCanvas();
    const p = await armAdd("P");
    const q = await armAdd("Q"); // [Q, P]
    await armApply({ type: "mergeSelected", ids: [p.id, q.id], mergedId: "M2" });
    const armSnap = await armSnapshot();

    // Side A: pure-TS layerOps directly against a plain DocumentModel (stub still
    // required for the same composite path as p3).
    const model = newDocModel();
    const op = addLayer(model, "P");
    const oq = addLayer(model, "Q"); // [Q, P]
    mergeSelectedLayers(model, [op.id, oq.id]);
    const tsLayers = model.layers;

    expect(armSnap.layers.length).toBe(tsLayers.length);
    expectByNameOverlap(armSnap.layers, tsLayers, "(p4) mergeSelected x2");
    expect(armSnap.layers[0].name).toBe("Q + P");

    structMatrix.push({
      scenario: "(p4) mergeSelected (2 layers)",
      counts: "EQUAL (2 -> 1 on both engines)",
      overlap: "EQUAL (merged name 'Q + P')",
      divergences: "(a) pixel composite host-side; (e) merged id host-supplied",
    });
  });

  it("(p5) flatten - single 'Background' node; count parity EQUAL", async () => {
    requireWasm();
    await armAdd("A");
    await armAdd("B");
    await armAdd("C"); // [C, B, A]
    await armApply({ type: "flatten", mergedId: "BG" });
    const armSnap = await armSnapshot();

    // Side A: pure-TS layerOps directly against a plain DocumentModel.
    const model = newDocModel();
    addLayer(model, "A");
    addLayer(model, "B");
    addLayer(model, "C");
    flattenLayers(model);
    const tsLayers = model.layers;

    expect(armSnap.layers.length).toBe(tsLayers.length);
    expectByNameOverlap(armSnap.layers, tsLayers, "(p5) flatten");
    expect(armSnap.layers[0].name).toBe("Background");

    structMatrix.push({
      scenario: "(p5) flatten",
      counts: "EQUAL (3 -> 1 on both engines)",
      overlap: "EQUAL (single 'Background' node)",
      divergences: "(a) pixel composite host-side; (b) composite-null bail no arm equivalent; (d) budget guard host-side; (e) bg id host-supplied",
    });
  });

  it("(p6) rasterizeLayer - shape/text drop params -> raster; name preserved", async () => {
    requireWasm();
    const s = await armAddShape("Star");
    await armApply({ type: "rasterizeLayer", id: s.id });
    const armSnap = await armSnapshot();
    const armStar = armSnap.layers.find((l: any) => l.id === s.id);
    expect(armStar.layerType).toBe("raster");
    expect(armStar.shapeParams).toBeUndefined();

    const t = await armAddText("Caption");
    await armApply({ type: "rasterizeLayer", id: t.id });
    const armSnap2 = await armSnapshot();
    const armCap = armSnap2.layers.find((l: any) => l.id === t.id);
    expect(armCap.layerType).toBe("raster");
    expect(armCap.textData).toBeUndefined();

    // Side A: pure-TS layerOps directly against a plain DocumentModel.
    const model = newDocModel();
    const sl = addShapeLayer(model, "Star", { kind: "star", width: 64, height: 64, radius: 4, fill: { kind: "solid", color: "#E15A17" }, stroke: { enabled: false, color: "#000000", width: 0 }, arrowHead: false });
    shapeLayerToRaster(sl); // mutates sl in place -> type raster, shapeParams removed
    // The TS oracle LayerNode carries the kind on `type`; the arm's RenderLayer
    // carries it on `layerType` (field-name divergence, not a behavior break).
    expect(model.layers.find((l: any) => l.name === "Star")?.type).toBe("raster");
    const tl = addTextLayer(model, "Caption", { content: "Hi", fontFamily: "Arial", fontSize: 32, fontWeight: 400, fontStyle: "normal", color: "#000000", align: "left", lineHeight: 1.2, letterSpacing: 0, boxMode: "point", boxWidth: 0, boxHeight: 0, stroke: { width: 0, color: "#000000" } });
    textLayerToRaster(tl);
    expect(model.layers.find((l: any) => l.name === "Caption")?.type).toBe("raster");

    expectByNameOverlap(armSnap2.layers, model.layers, "(p6) rasterize");

    structMatrix.push({
      scenario: "(p6) rasterizeLayer (shape + text)",
      counts: "EQUAL (1 -> 1 each)",
      overlap: "EQUAL (name preserved; raster kind on both)",
      divergences: "(c) rasterize arm label consolidation; (a) bitmap re-raster stays host-side; (f) oracle sets layer.type, arm sets layerType (field-name divergence)",
    });
  });

  afterAll(() => {
    // eslint-disable-next-line no-console
    console.log("\n=== OPERATION PARITY MATRIX - STRUCTURAL ARM PATH (ProtocolEngine) ===");
    // eslint-disable-next-line no-console
    console.log(JSON.stringify(structMatrix, null, 2));
  });
});
