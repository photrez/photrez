// Layer width/height through the facade projection snapshot.
//
// The snapshot is the layer vector the next routed op rebuilds the model from
// (applyFacadeSnapshot replaces model.layers). Layer dimensions were not carried
// on that vector, so the REBUILD branch (a layer id the model does not have) fell
// back to the DOCUMENT size: a natively created 100x100 layer landed in an 800x600
// model as 800x600. The fields are carried now and the rebuild branch uses them.
//
// Ownership: layer dims belong to the MODEL. The pixel path produces them
// host-side (updateShapeParams / updateTextData / setLayerImageBitmap in
// engine/document.ts), and no facade command reports a new size back: every arm
// clones the layer the engine already stores, so a restatement can only repeat the
// size learned at add time. (The canonical re-push path - seed_canonical at
// document open, after a native add, and on heal - up-projects the model's dims
// into the engine; that is a push, never a restatement.) Every branch that has a
// model node to protect therefore ignores width/height - a restatement of the
// stored layer would otherwise clobber the real size on every metadata op.
// Projected dims are honored only in the rebuild branch, the one case with no
// model value to keep.
//
// The chain that must round-trip is
// toFacadeProjectionLayer -> refreshFacadeSnapshotFromModelLayers ->
// applyFacadeSnapshot; it is the one two earlier investigations fell into because
// the cache dropped the dimensions.

import { describe, it, expect, vi, beforeEach, afterEach, beforeAll } from "vitest";
import { DocumentEngine } from "@/engine/document";
import { toFacadeProjectionLayer } from "@/lib/protocol/facadeProjection";
import {
  commitFacadeRename,
  getFacade,
  seedFacadeFromEngine,
  __resetFacadeRegistryForTests,
} from "@/lib/protocol/facadeRegistry";
import { getWasmExportModule } from "@/components/editor/wasmExport";
import { applyCommand, getSnapshot } from "@/lib/protocol/bridge";
import { CONTRACT_VERSION } from "@/lib/protocol/types";

function layerDesc(id: string, extra: Record<string, unknown> = {}) {
  return {
    id,
    name: "L",
    visible: true,
    opacity: 1,
    x: 0,
    y: 0,
    scaleX: 1,
    scaleY: 1,
    rotation: 0,
    resourceId: 0,
    ...extra,
  };
}

function snap(version: number, layers: Record<string, unknown>[]): never {
  return { version, layers } as never;
}

describe("facade layer dims projection (pure)", () => {
  it("existing branch: dims are model-owned, a restated size never overwrites them", () => {
    const engine = new DocumentEngine("dims-doc", "D", 800, 600);
    const l = engine.addLayer("L", 100, 100);
    engine.applyFacadeSnapshot(snap(1, [layerDesc(l.id, { width: 300, height: 250 })]));
    expect(engine.getLayer(l.id)!.width).toBe(100);
    expect(engine.getLayer(l.id)!.height).toBe(100);
  });

  it("existing branch: absent width/height preserve the model value (anti-clobber)", () => {
    const engine = new DocumentEngine("dims-keep", "D", 800, 600);
    const l = engine.addLayer("L", 200, 150);
    expect(engine.getLayer(l.id)!.width).toBe(200);
    // The wire omits both keys when the engine holds None: must not reset to the
    // document size.
    engine.applyFacadeSnapshot(snap(1, [layerDesc(l.id)]));
    expect(engine.getLayer(l.id)!.width).toBe(200);
    expect(engine.getLayer(l.id)!.height).toBe(150);
  });

  it("retained branch: the retained model node's dims win over a restatement", () => {
    const engine = new DocumentEngine("dims-retain", "D", 800, 600);
    const l = engine.addLayer("L", 100, 100);
    // Drop from the model (deleted / undone add) -> the node is retained.
    engine.applyFacadeSnapshot(snap(1, []));
    expect(engine.getLayer(l.id)).toBeUndefined();
    // Reappears with the fields absent: the retained node's own dims survive.
    engine.applyFacadeSnapshot(snap(2, [layerDesc(l.id)]));
    expect(engine.getLayer(l.id)!.width).toBe(100);
    expect(engine.getLayer(l.id)!.height).toBe(100);
    // Drop again, then reappear with the fields restated. The retention IS a model
    // node, and no arm ever moves a stored layer's size, so the restated pair can
    // only be older than the retained one and must not win.
    engine.applyFacadeSnapshot(snap(3, []));
    engine.applyFacadeSnapshot(snap(4, [layerDesc(l.id, { width: 300, height: 250 })]));
    expect(engine.getLayer(l.id)!.width).toBe(100);
    expect(engine.getLayer(l.id)!.height).toBe(100);
  });

  it("rebuild branch: present carries the layer's own size (not the document size)", () => {
    const engine = new DocumentEngine("dims-rebuild", "D", 800, 600);
    engine.applyFacadeSnapshot(snap(1, [layerDesc("fresh-native", { width: 100, height: 100 })]));
    expect(engine.getLayer("fresh-native")!.width).toBe(100);
    expect(engine.getLayer("fresh-native")!.height).toBe(100);
  });

  it("rebuild branch: absent falls back to the document size (no per-layer value exists)", () => {
    const engine = new DocumentEngine("dims-rebuild-absent", "D", 800, 600);
    engine.applyFacadeSnapshot(snap(1, [layerDesc("fresh-missing")]));
    expect(engine.getLayer("fresh-missing")!.width).toBe(800);
    expect(engine.getLayer("fresh-missing")!.height).toBe(600);
  });

  it("invalid input: a null width/height is never written (the existing branch keeps the model dims)", () => {
    const engine = new DocumentEngine("dims-null", "D", 800, 600);
    const l = engine.addLayer("L", 120, 90);
    engine.applyFacadeSnapshot(snap(1, [layerDesc(l.id, { width: null, height: null })]));
    expect(engine.getLayer(l.id)!.width).toBe(120);
    expect(engine.getLayer(l.id)!.height).toBe(90);
  });

  it("toFacadeProjectionLayer carries width/height out of the model", () => {
    const out = toFacadeProjectionLayer({
      id: "l",
      name: "L",
      visible: true,
      opacity: 1,
      width: 300,
      height: 250,
      transform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0 },
    });
    expect(out.width).toBe(300);
    expect(out.height).toBe(250);
    // Absent stays absent so the projection keeps the model value (see the
    // anti-clobber case above).
    const absent = toFacadeProjectionLayer({
      id: "l",
      name: "L",
      visible: true,
      opacity: 1,
      transform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0 },
    });
    expect(absent.width).toBeUndefined();
    expect(absent.height).toBeUndefined();
  });
});

// ─── Real chain: real wasm mirror + real facade funnel (photrez.facade=1) ───

let wasmModule: { protocol_reset: (docId: string) => void } | null = null;

beforeAll(async () => {
  const m = await getWasmExportModule();
  wasmModule = m;
});

beforeEach(() => {
  localStorage.setItem("photrez.facade", "1");
});

afterEach(() => {
  localStorage.removeItem("photrez.facade");
  __resetFacadeRegistryForTests();
  wasmModule?.protocol_reset("default");
  vi.restoreAllMocks();
});

function fakeBitmap(width: number, height: number): ImageBitmap {
  return { width, height, close: vi.fn() } as unknown as ImageBitmap;
}

async function setupDoc(id: string) {
  const engine = new DocumentEngine(id, id, 800, 600);
  const facade = getFacade(id);
  await seedFacadeFromEngine(engine as never, facade);
  return { engine, facade };
}

describe("layer dims survive the real facade chain", () => {
  it("a native add of a non-document-sized layer lands with its own dims", async () => {
    const { facade } = await setupDoc("docDimsNativeAdd");
    const added = await facade.addLayer("Native", 100, 100, 0);
    const wire = added.layers.find((l) => l.name === "Native")!;
    // Item-1 evidence from the real arm: the upsert carries the values.
    expect(wire.width).toBe(100);
    expect(wire.height).toBe(100);

    // A model that does not have the layer takes the REBUILD branch. Before the
    // projection carried the fields this landed at the document size (800x600).
    const sink = new DocumentEngine("docDimsNativeAddSink", "S", 800, 600);
    sink.applyFacadeSnapshot(added);
    const landed = sink.getLayers().find((l) => l.name === "Native")!;
    expect(landed.width).toBe(100);
    expect(landed.height).toBe(100);
  });

  it("cache round trip: toFacadeProjectionLayer -> refresh -> applyFacadeSnapshot keeps the dims", async () => {
    const { engine, facade } = await setupDoc("docDimsCache");
    // A legacy add goes through notifyChange, which republishes the model into
    // the facade snapshot through toFacadeProjectionLayer.
    const l = engine.addLayer("Legacy", 200, 200);
    const cached = facade.snapshot.layers.find((x) => x.id === l.id)!;
    expect(cached.width).toBe(200);
    expect(cached.height).toBe(200);

    const sink = new DocumentEngine("docDimsCacheSink", "S", 800, 600);
    sink.applyFacadeSnapshot(facade.snapshot as never);
    expect(sink.getLayer(l.id)!.width).toBe(200);
    expect(sink.getLayer(l.id)!.height).toBe(200);
  });

  it("a bitmap replace that moves the pixel dims still agrees with the model after a routed op", async () => {
    const { engine, facade } = await setupDoc("docDimsBitmap");
    const first = await facade.addLayer("Owned", 100, 100, 0);
    engine.applyFacadeSnapshot(first as never);
    await facade.addLayer("Other", 100, 100, 0);
    engine.applyFacadeSnapshot(facade.snapshot as never);
    const ownedId = facade.snapshot.layers.find((l) => l.name === "Owned")!.id;
    const otherId = facade.snapshot.layers.find((l) => l.name === "Other")!.id;

    // The pixel path owns the layer's own bitmap dims: replacing the bitmap with
    // a differently sized one moves them.
    engine.setLayerImageBitmap(ownedId, fakeBitmap(120, 120));
    expect(engine.getLayer(ownedId)!.width).toBe(120);

    // A routed metadata op on the OTHER layer does not restate this layer, so the
    // only value the projection can write comes from the snapshot cache. The pixel
    // value must survive it (setLayerImageBitmap republishes the model into the
    // snapshot, since dims are projection-carried now).
    const r = await commitFacadeRename(engine as never, [otherId], "Renamed");
    expect(r.status).toBe("applied");
    expect(engine.getLayer(ownedId)!.width).toBe(120);
    expect(engine.getLayer(ownedId)!.height).toBe(120);
  });

  it("a same-layer metadata op does not overwrite model-owned dims with the engine's stored size", async () => {
    const { engine, facade } = await setupDoc("docDimsOwnership");
    const added = await facade.addLayer("Owned", 100, 100, 0);
    engine.applyFacadeSnapshot(added as never);
    const ownedId = added.layers.find((l) => l.name === "Owned")!.id;

    // The pixel path produced the model's real dims and moved them away from the
    // 100x100 the native engine stored at create time.
    engine.setLayerImageBitmap(ownedId, fakeBitmap(120, 90));
    expect(engine.getLayer(ownedId)!.width).toBe(120);
    expect(engine.getLayer(ownedId)!.height).toBe(90);

    // A routed op on the SAME layer makes the arm restate its stored layer,
    // which still carries the create-time size. The model owns the dims, so the
    // restatement must not write them back.
    const r = await commitFacadeRename(engine as never, [ownedId], "Renamed");
    expect(r.status).toBe("applied");
    expect(engine.getLayer(ownedId)!.name).toBe("Renamed");
    expect(engine.getLayer(ownedId)!.width).toBe(120);
    expect(engine.getLayer(ownedId)!.height).toBe(90);
  });

  it("rebuild branch still takes the projected size (the model has nothing to keep)", async () => {
    const { engine, facade } = await setupDoc("docDimsOwnershipRebuild");
    const added = await facade.addLayer("Native", 140, 60, 0);
    // The sink model never held this layer, so the rebuild branch must land on the
    // projected size rather than the document size.
    const sink = new DocumentEngine("docDimsOwnershipRebuildSink", "S", 800, 600);
    sink.applyFacadeSnapshot(added as never);
    const landed = sink.getLayers().find((l) => l.name === "Native")!;
    expect(landed.width).toBe(140);
    expect(landed.height).toBe(60);
    expect(engine.getLayer(added.layers[0].id)).toBeUndefined();
  });

  it("a routed duplicate of a re-rastered layer keeps the model's dims, not the engine's", async () => {
    const { engine } = await setupDoc("docDimsDuplicateRetention");
    const docId = engine.getId();
    const added = await applyCommand({
      contractVersion: CONTRACT_VERSION,
      expectedVersion: 0,
      docId,
      command: {
        type: "addLayer",
        id: "layer-dims-src",
        name: "Owned",
        width: 100,
        height: 100,
        index: 0,
      } as never,
    });
    const snapshot = await getSnapshot(docId);
    engine.applyFacadeSnapshot(snapshot as never);
    expect(added.delta.version).toBe(snapshot.version);
    const srcId = "layer-dims-src";

    // The pixel path re-rastered the layer AFTER the engine learned it: the model
    // is 120x90 while the engine's stored copy still carries the create-time 100x100.
    engine.setLayerImageBitmap(srcId, fakeBitmap(120, 90));
    expect(engine.getLayer(srcId)!.width).toBe(120);

    // Routed duplicate. routeDuplicate pre-seeds the projection retention with the
    // clone node (model dims, the same duplicateLayerNode carry) and then dispatches
    // the Duplicate arm, which clones the layer the ENGINE holds. The clone id is
    // new to the model, so the arrival lands on the RETAINED branch.
    const cloneId = "layer-dims-clone";
    engine.seedRetainedNodeForProjection({ ...engine.getLayer(srcId)!, id: cloneId });
    await applyCommand({
      contractVersion: CONTRACT_VERSION,
      expectedVersion: snapshot.version,
      docId,
      command: { type: "duplicateLayer", id: srcId, newId: cloneId } as never,
    });
    const afterDuplicate = await getSnapshot(docId);
    engine.applyFacadeSnapshot(afterDuplicate as never);

    const clone = engine.getLayer(cloneId);
    expect(clone).toBeDefined();
    expect(clone!.width).toBe(120);
    expect(clone!.height).toBe(90);
    // The source keeps its model dims through the same projection.
    expect(engine.getLayer(srcId)!.width).toBe(120);
  });
});
