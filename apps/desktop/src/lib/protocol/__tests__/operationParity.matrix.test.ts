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
//  oracle; layers are compared by NAME (the arm mints uuid v4 ids, the TS
//  engine mints its own - ids are never compared across engines).
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
  // Arm mints a uuid v4 id; read it back from the snapshot by name.
  async function armAdd(name: string): Promise<{ id: string; snapshot: any }> {
    await armApply({ type: "addLayer", name });
    const snap = await armSnapshot();
    const layer = snap.layers.find((l: any) => l.name === name) as any;
    return { id: layer.id, snapshot: snap };
  }

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

  it("(a) addLayer x2 then deleteLayer - counts EQUAL; placement + addLayer impoverishment + uuid mint DIVERGENT", async () => {
    requireWasm();
    const north = await armAdd("North");
    const preDelete = await armAdd("South"); // [North, South] (arm appends at end)
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

    // DIVERGENCE 1 - placement: arm appends at END, TS inserts above active.
    expect(preDeleteSnap.layers.map((l: any) => l.name)).not.toEqual(tsPreDelete);

    // DIVERGENCE 2 - addLayer impoverishment: arm carries name only; the
    // resulting RenderLayer has no type/blend/width/height the TS layer carries
    // (command.rs AddLayer{name}; model.rs RenderLayer has no such fields).
    const armLayer = preDeleteSnap.layers.find((l: any) => l.name === "North");
    expect(armLayer.type).toBeUndefined();
    expect(armLayer.blendMode).toBeUndefined();
    expect(armLayer.width).toBeUndefined();
    expect(armLayer.height).toBeUndefined();
    // TS side carries the full layer; the arm has no field for these.
    expect(tsNorth.width).toBe(200);
    expect(tsNorth.height).toBe(200);
    expect(tsNorth.type).toBe("raster");
    expect(tsNorth.blendMode).toBeDefined();

    // DIVERGENCE 3 - uuid minting: arm id is uuid v4; TS id is layer-<rand>.
    expect(north.id).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/);
    expect(north.id).not.toEqual(nL.id);

    armMatrix.push({
      scenario: "(a) addLayer x2 + deleteLayer",
      counts: "EQUAL (2 -> 1 on both engines)",
      overlapAfterDelete: "EQUAL (by name)",
      divergences:
        "placement (arm appends at END vs TS above-active); addLayer impoverishment (arm name-only; no type/blend/width/height); uuid mint differs from TS id",
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

    // DIVERGENCE - placement on redo: arm [A,B] vs TS [B,A].
    expect(armRedo.layers.map((l: any) => l.name)).not.toEqual(tsRedo.map((l: any) => l.name));

    armMatrix.push({
      scenario: "(d1) undo/redo after-add",
      counts: "EQUAL (undo->1, redo->2 on both)",
      overlap: "EQUAL (by name) after undo and redo",
      divergences: "placement DIVERGENT on redo (arm [A,B] vs TS [B,A]); uuid ids differ",
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

    // DIVERGENCE - placement on undo: arm [A,B] vs TS [B,A].
    expect(armUndo.layers.map((l: any) => l.name)).not.toEqual(tsUndo.map((l: any) => l.name));

    armMatrix.push({
      scenario: "(d2) undo/redo after-delete",
      counts: "EQUAL (undo->2, redo->1 on both)",
      overlap: "EQUAL (by name) after undo and redo",
      divergences: "placement DIVERGENT on undo (arm [A,B] vs TS [B,A]); uuid ids differ",
    });
  });

  afterAll(() => {
    // eslint-disable-next-line no-console
    console.log("\n=== OPERATION PARITY MATRIX - ARM PATH (ProtocolEngine) ===");
    // eslint-disable-next-line no-console
    console.log(JSON.stringify(armMatrix, null, 2));
  });
});
