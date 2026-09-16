// Layer width/height through the facade projection snapshot.
//
// The snapshot is the layer vector the next routed op rebuilds the model from
// (applyFacadeSnapshot replaces model.layers). Layer dimensions were not carried
// on that vector, so the REBUILD branch (a layer id the model does not have) fell
// back to the DOCUMENT size: a natively created 100x100 layer landed in an 800x600
// model as 800x600. Production order (the legacy model creates the layer first,
// then the projection lands on the EXISTING branch) hid it, so the failure is
// only reachable when the native engine originates an add - or when a test
// projects a native add onto a model that does not have the layer.
//
// Convention (same as flipH/flipV, textData, shapeParams): present = write the
// model value, absent = keep it. The wire carries width/height as
// Option<f64> with skip_serializing_if = "Option::is_none"
// (crates/core/src/model.rs:51-54); the AddLayer arm sets Some(width)/Some(height)
// from the command (crates/core/src/document_core_apply.rs:192-193), so the keys
// are absent exactly when no real value exists.
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
  it("existing branch: present width/height reach the model", () => {
    const engine = new DocumentEngine("dims-doc", "D", 800, 600);
    const l = engine.addLayer("L", 100, 100);
    engine.applyFacadeSnapshot(snap(1, [layerDesc(l.id, { width: 300, height: 250 })]));
    expect(engine.getLayer(l.id)!.width).toBe(300);
    expect(engine.getLayer(l.id)!.height).toBe(250);
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

  it("retained branch: absent keeps the retained dims, present overrides", () => {
    const engine = new DocumentEngine("dims-retain", "D", 800, 600);
    const l = engine.addLayer("L", 100, 100);
    // Drop from the model (deleted / undone add) -> the node is retained.
    engine.applyFacadeSnapshot(snap(1, []));
    expect(engine.getLayer(l.id)).toBeUndefined();
    // Reappears with the fields absent: the retained node's own dims survive.
    engine.applyFacadeSnapshot(snap(2, [layerDesc(l.id)]));
    expect(engine.getLayer(l.id)!.width).toBe(100);
    expect(engine.getLayer(l.id)!.height).toBe(100);
    // Drop again, then reappear with the fields restated: the restatement wins.
    engine.applyFacadeSnapshot(snap(3, []));
    engine.applyFacadeSnapshot(snap(4, [layerDesc(l.id, { width: 300, height: 250 })]));
    expect(engine.getLayer(l.id)!.width).toBe(300);
    expect(engine.getLayer(l.id)!.height).toBe(250);
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

  it("invalid input: a null width/height is treated as absent, never written", () => {
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
});
