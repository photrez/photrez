// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Facade typed-add desync regression (photrez.facade=1).
//
// A legacy typed-layer add (addTextLayer/addShapeLayer) runs through the
// always-on Rust graph mirror and projects the result back with
// syncLayersFromRust. When the facade is the authority, that mirror never
// received the facade-routed layers, so the projection rebuilds the model from
// a SHORT mirror and drops them; a mirrored external transition then re-pushes
// the short model and the next routed op silently no-ops on the missing layer.
// Separately, the facade projection snapshot must learn the externally-added
// layer or the next routed projection rebuilds the model from a snapshot
// without it and drops it again.
//
// These tests drive the REAL wasm mirror (wasmTestShim) and the REAL facade
// funnel; no hand-written engine mock. Native-side consistency is covered by
// the live probe (a headless test cannot observe the native registry engine).

import { describe, it, expect, vi, beforeEach, afterEach, beforeAll } from "vitest";
import { DocumentEngine, isFacadeOwnedLayer } from "@/engine/document";
import { DEFAULT_TEXT_DATA } from "@/engine/textTypes";
import { getWasmExportModule } from "@/components/editor/wasmExport";
import {
  commitFacadeOpacity,
  getFacade,
  recordExternalTransitionFor,
  seedFacadeFromEngine,
  __resetFacadeRegistryForTests,
} from "@/lib/protocol/facadeRegistry";

let wasmModule: { protocol_reset: (docId: string) => void } | null = null;

// jsdom has no OffscreenCanvas; stub the minimal seam the text rasterizer uses
// so the typed add produces a bitmap and never falls through to a null 2d
// context.
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

beforeAll(async () => {
  const m = await getWasmExportModule();
  wasmModule = m;
});

beforeEach(() => {
  localStorage.setItem("photrez.facade", "1");
  // Pinned wasm authority exercises the facade flag path without the native
  // registry; native consistency is covered by the live probe.
  localStorage.setItem("photrez.facadeAuthority", "wasm");
  stubOffscreenCanvas();
});

afterEach(() => {
  localStorage.removeItem("photrez.facade");
  localStorage.removeItem("photrez.facadeAuthority");
  __resetFacadeRegistryForTests();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

async function setupDoc(id: string) {
  const engine = new DocumentEngine(id, id, 800, 600);
  const facade = getFacade(id);
  await seedFacadeFromEngine(engine as never, facade);
  const snap = await facade.addLayer("Owned", 100, 100, 0);
  engine.applyFacadeSnapshot(snap as never);
  const ownedId = facade.snapshot.layers.find((l) => l.name === "Owned")!.id;
  return { engine, facade, ownedId };
}

describe("facade typed-add desync (photrez.facade=1)", () => {
  it("keeps a facade-routed layer when a legacy typed add follows", async () => {
    const { engine, ownedId } = await setupDoc("docTypedAdd");
    expect(engine.getLayers().map((l) => l.id)).toContain(ownedId);

    const text = engine.addTextLayer("Hello", { ...DEFAULT_TEXT_DATA, content: "Hello" });

    const ids = engine.getLayers().map((l) => l.id);
    expect(ids).toContain(ownedId);
    expect(ids).toContain(text.id);
  });

  it("refreshes the facade projection snapshot after an external transition so the next routed op keeps the external layer", async () => {
    const { engine, facade, ownedId } = await setupDoc("docTypedExt");
    const text = engine.addTextLayer("Hello", { ...DEFAULT_TEXT_DATA, content: "Hello" });

    const rec = await recordExternalTransitionFor(
      "docTypedExt",
      { label: "Add Text", affectedLayerIds: [text.id], snapshot: {} },
      engine as never,
    );
    expect(rec.ok).toBe(true);
    // The projection snapshot the next routed op rebuilds the model from must
    // have learned the externally-added layer.
    expect(facade.snapshot.layers.map((l) => l.id)).toContain(text.id);

    const r = await commitFacadeOpacity(engine as never, [ownedId], 0.9);
    expect(r.status).toBe("applied");
    const ids = engine.getLayers().map((l) => l.id);
    expect(ids).toContain(ownedId);
    expect(ids).toContain(text.id);
  });
});

// The choke point lives INSIDE syncLayersFromRust, so it covers every legacy op
// that rebuilds the model from the mirror - not just the two typed adds. These
// tests drive a DIFFERENT legacy op (addLayer/reorderLayer) and the facade delete
// path to pin the class and the no-resurrection boundary.

// Replace the per-engine wasm mirror with a stub so the rebuild can be driven
// with an empty or throwing layer list. Only the methods the exercised op calls
// are needed.
function stubMirror(engine: DocumentEngine, overrides: Record<string, unknown>): void {
  (engine as unknown as { rustEngine: unknown }).rustEngine = {
    reorder_layer: () => true,
    get_active_layer_id: () => null,
    ...overrides,
  };
}

describe("facade choke point in syncLayersFromRust (photrez.facade=1)", () => {
  it("preserves a facade-routed layer across an unrelated legacy op (mirror rebuild)", async () => {
    const { engine, ownedId } = await setupDoc("docChokePoint");
    // addLayer is NOT facade-routed: it goes through the Rust mirror and rebuilds
    // the model via syncLayersFromRust from a mirror that lacks the routed layer.
    engine.addLayer("Legacy");
    expect(engine.getLayers().map((l) => l.id)).toContain(ownedId);
  });

  it("does NOT resurrect a layer deleted through the facade", async () => {
    const { engine, facade, ownedId } = await setupDoc("docNoResurrect");
    const delSnap = await facade.deleteLayer(ownedId);
    engine.applyFacadeSnapshot(delSnap as never);
    expect(engine.getLayers().map((l) => l.id)).not.toContain(ownedId);
    expect(isFacadeOwnedLayer(ownedId)).toBe(false);

    // A legacy op rebuilds the model from the mirror; the deleted id must stay
    // gone (it is neither in the model nor facade-owned any more).
    engine.addLayer("Legacy");
    expect(engine.getLayers().map((l) => l.id)).not.toContain(ownedId);
  });

  it("empty mirror: a facade-owned layer survives the rebuild", async () => {
    const { engine, ownedId } = await setupDoc("docEmptyMirror");
    stubMirror(engine, { get_layers_json: () => "[]" });
    engine.reorderLayer(0, 0);
    expect(engine.getLayers().map((l) => l.id)).toContain(ownedId);
  });

  it("throwing mirror read: the model is left untouched (no crash)", async () => {
    const { engine, ownedId } = await setupDoc("docThrowMirror");
    stubMirror(engine, {
      get_layers_json: () => {
        throw new Error("mirror down");
      },
    });
    expect(() => engine.reorderLayer(0, 0)).not.toThrow();
    expect(engine.getLayers().map((l) => l.id)).toContain(ownedId);
  });

  it("throwing engine.getLayers(): recordExternalTransitionFor still records ok", async () => {
    const { engine } = await setupDoc("docThrowGetLayers");
    const throwing = {
      getId: () => "docThrowGetLayers",
      getLayers: () => {
        throw new Error("engine gone");
      },
    };
    const res = await recordExternalTransitionFor(
      "docThrowGetLayers",
      { label: "Legacy", affectedLayerIds: [], snapshot: {} },
      throwing as never,
    );
    expect(res.ok).toBe(true);
  });
});

// Regression: the rebuild must not APPEND a mirror-absent preserved layer. The
// mirror only restates the ids it received, so with a mixed doc (a routed layer
// beside legacy layers) an append dumps every routed layer to the end of the
// panel + stacking order on any legacy op that rebuilds from a short mirror.

function mirrorLayerJson(id: string, name: string): unknown {
  return {
    id,
    name,
    type: "raster",
    visible: true,
    opacity: 1,
    locked: false,
    width: 100,
    height: 100,
    transform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
  };
}

describe("facade rebuild order + active retention (photrez.facade=1)", () => {
  it("a legacy reorder keeps a routed layer in its slot (never teleports it to the end)", async () => {
    const { engine, facade, ownedId } = await setupDoc("docOrderKeep");
    const bSnap = await facade.addLayer("LegacyB", 100, 100, 0);
    engine.applyFacadeSnapshot(bSnap as never);
    const cSnap = await facade.addLayer("LegacyC", 100, 100, 0);
    engine.applyFacadeSnapshot(cSnap as never);
    const bId = facade.snapshot.layers.find((l) => l.name === "LegacyB")!.id;
    const cId = facade.snapshot.layers.find((l) => l.name === "LegacyC")!.id;
    // Routed layer on top, two legacy layers below.
    expect(engine.getLayers().map((l) => l.id)).toEqual([ownedId, bId, cId]);

    // The per-engine mirror never received the routed layer; it restates only the
    // two legacy layers (the window right after a facade op, before the next
    // legacy push refreshes the mirror).
    stubMirror(engine, {
      reorder_layer: () => true,
      get_active_layer_id: () => null,
      get_layers_json: () => JSON.stringify([mirrorLayerJson(bId, "LegacyB"), mirrorLayerJson(cId, "LegacyC")]),
    });

    engine.reorderLayer(0, 1);

    const ids = engine.getLayers().map((l) => l.id);
    expect(ids).toContain(ownedId);
    // Still on top - the append implementation would have moved it to the end.
    expect(ids[0]).toBe(ownedId);
  });

  it("a legacy op keeps the routed active layer selected (mirror-absent active)", async () => {
    const { engine, ownedId } = await setupDoc("docActiveKeep");
    expect(engine.getModel().activeLayerId).toBe(ownedId);

    // Short mirror with no active id: adopting it would clear the selection.
    stubMirror(engine, {
      reorder_layer: () => true,
      get_active_layer_id: () => null,
      get_layers_json: () => "[]",
    });

    engine.reorderLayer(0, 0);

    expect(engine.getModel().activeLayerId).toBe(ownedId);
  });
});

// Blind spot the no-resurrect test above leaves open: it never PRIMS the mirror
// with the facade-owned id. In the real app a legacy op runs notifyChange ->
// pushModelToRust, which copies the WHOLE model (facade-owned layers included)
// into the per-engine wasm mirror. A facade delete then removes the id from the
// model but leaves it in that stale mirror, so the next rebuild sees a
// mirror-present + prev-absent id - the shape of a "new" mirror id. The mirror
// must be primed with a real legacy op before the facade delete for the
// resurrection to be reachable at all.

describe("facade-deleted id primed into the per-engine mirror (photrez.facade=1)", () => {
  it("does NOT resurrect a facade-deleted id that an earlier legacy push left in the mirror", async () => {
    const docId = "docPrimedResurrect";
    const engine = new DocumentEngine(docId, docId, 800, 600);
    const facade = getFacade(docId);

    // 1. legacy base layer, then the one-time facade seed from it.
    const base = engine.addLayer("base");
    await seedFacadeFromEngine(engine as never, facade);

    // 2. facade add: projects into the model but never into the per-engine mirror.
    const pSnap = await facade.addLayer("P", 100, 100, 0);
    engine.applyFacadeSnapshot(pSnap as never);
    const pId = facade.snapshot.layers.find((l) => l.name === "P")!.id;
    expect(engine.getLayers().map((l) => l.id)).toContain(pId);

    // 3. legacy add: notifyChange pushes the WHOLE model (P included) into the
    //    mirror, so the mirror now holds the facade-owned id. The commit shim
    //    refresh teaches the facade snapshot about L1, as it does in production.
    const l1 = engine.addLayer("L1");
    await recordExternalTransitionFor(
      docId,
      { label: "Add L1", affectedLayerIds: [l1.id], snapshot: {} },
      engine as never,
    );

    // 4. facade delete: the model loses P, the stale mirror keeps it.
    const delSnap = await facade.deleteLayer(pId);
    engine.applyFacadeSnapshot(delSnap as never);
    expect(engine.getLayers().map((l) => l.id)).not.toContain(pId);
    expect(isFacadeOwnedLayer(pId)).toBe(false);

    // 5. another legacy add rebuilds the model from the stale mirror.
    const l2 = engine.addLayer("L2");

    // A legacy add inserts at the top (index 0), so the survivors keep their
    // relative order and the newest add is first; the deleted id is GONE.
    expect(engine.getLayers().map((l) => l.id)).toEqual([l2.id, l1.id, base.id]);
    expect(engine.getLayers().map((l) => l.id)).not.toContain(pId);
    expect(engine.getModel().activeLayerId).not.toBe(pId);
  });

  it("does not adopt the stale mirror active id of a facade-deleted layer", async () => {
    const docId = "docPrimedActive";
    const engine = new DocumentEngine(docId, docId, 800, 600);
    const facade = getFacade(docId);
    const base = engine.addLayer("base");
    await seedFacadeFromEngine(engine as never, facade);
    const pSnap = await facade.addLayer("P", 100, 100, 0);
    engine.applyFacadeSnapshot(pSnap as never);
    const pId = facade.snapshot.layers.find((l) => l.name === "P")!.id;
    const l1 = engine.addLayer("L1");
    await recordExternalTransitionFor(
      docId,
      { label: "Add L1", affectedLayerIds: [l1.id], snapshot: {} },
      engine as never,
    );

    // A primed mirror that still holds the routed id AND reports it active - the
    // shape a legacy push leaves behind just before the facade delete.
    stubMirror(engine, {
      reorder_layer: () => true,
      get_active_layer_id: () => pId,
      get_layers_json: () =>
        JSON.stringify([
          mirrorLayerJson(base.id, "base"),
          mirrorLayerJson(l1.id, "L1"),
          mirrorLayerJson(pId, "P"),
        ]),
    });

    const delSnap = await facade.deleteLayer(pId);
    engine.applyFacadeSnapshot(delSnap as never);
    expect(engine.getLayers().map((l) => l.id)).not.toContain(pId);

    engine.reorderLayer(0, 0);

    expect(engine.getLayers().map((l) => l.id)).not.toContain(pId);
    expect(engine.getModel().activeLayerId).not.toBe(pId);
  });

  it("warns instead of silently dropping model content when the mirror read is degenerate", async () => {
    const docId = "docDegenerateMirror";
    const engine = new DocumentEngine(docId, docId, 800, 600);
    const facade = getFacade(docId);
    await seedFacadeFromEngine(engine as never, facade);
    // A facade projection marks EVERY projected layer owned, so the layer that
    // can still be dropped by a degenerate mirror is one added through the legacy
    // path AFTER the last projection - it is neither in the mirror nor owned.
    const ownedSnap = await facade.addLayer("Owned", 100, 100, 0);
    engine.applyFacadeSnapshot(ownedSnap as never);
    const legacy = engine.addLayer("Legacy");
    expect(isFacadeOwnedLayer(legacy.id)).toBe(false);

    const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
    stubMirror(engine, { reorder_layer: () => true, get_active_layer_id: () => null, get_layers_json: () => "[]" });
    engine.reorderLayer(0, 0);

    const line = warn.mock.calls.map((c) => String(c[0])).find((m) => m.includes("[facade-sync]"));
    expect(line).toBeDefined();
    expect(line).toContain(legacy.id);
    // A legacy layer with no mirror entry and no facade ownership cannot be
    // preserved; the contract is that the loss is ANNOUNCED, not prevented.
    expect(engine.getLayers().map((l) => l.id)).not.toContain(legacy.id);
  });
});

describe("photrez.facade=0 opt-out is byte-identical", () => {
  beforeEach(() => {
    localStorage.setItem("photrez.facade", "0");
  });

  it("the typed-add path leaves the facade snapshot unchanged", async () => {
    const engine = new DocumentEngine("docOffAdd", "docOffAdd", 800, 600);
    const facade = getFacade("docOffAdd");
    engine.addLayer("Base");
    await seedFacadeFromEngine(engine as never, facade);
    const before = JSON.stringify(facade.snapshot);

    engine.addTextLayer("Hello", { ...DEFAULT_TEXT_DATA, content: "Hello" });
    expect(JSON.stringify(facade.snapshot)).toBe(before);
  });

  it("recordExternalTransitionFor leaves the facade snapshot unchanged", async () => {
    const engine = new DocumentEngine("docOffRec", "docOffRec", 800, 600);
    const facade = getFacade("docOffRec");
    engine.addLayer("Base");
    await seedFacadeFromEngine(engine as never, facade);
    const before = JSON.stringify(facade.snapshot);

    const res = await recordExternalTransitionFor(
      "docOffRec",
      { label: "Legacy", affectedLayerIds: [], snapshot: {} },
      engine as never,
    );
    expect(res.ok).toBe(true);
    expect(JSON.stringify(facade.snapshot)).toBe(before);
  });
});
