// SPDX-License-Identifier: AGPL-3.0-or-later
//
// The facade projection snapshot choke point is DocumentEngine.notifyChange()
// (photrez.facade=1). notifyChange fires after every legacy mutation and is keyed
// by the mutating engine, so it covers two shapes the commit shim cannot:
//
//   - a cross-document operation whose target is NOT the active document (the
//     shim resolves the active engine, so only the target's own notifyChange
//     refreshes the target facade), and
//   - an async mutation that lands its real fields after the await (adjustment
//     bake), so the snapshot must publish the POST-bake values.
//
// These tests drive the REAL wasm mirror + the REAL facade funnel. The commit
// shim is installed exactly as production does (once at EditorShell boot); the
// per-test liveEngine stands in for the active document.

import { describe, it, expect, vi, beforeEach, afterEach, beforeAll } from "vitest";
import { DocumentEngine } from "@/engine/document";
import { CommandHistory } from "@/engine/history";
import { getWasmExportModule } from "@/components/editor/wasmExport";
import { addLayerFromCrossDoc } from "@/components/editor/crossDocLayerOps";
import {
  commitFacadeOpacity,
  getFacade,
  installFacadeCommitShim,
  seedFacadeFromEngine,
  __resetFacadeRegistryForTests,
} from "@/lib/protocol/facadeRegistry";
import type { RenderBackend } from "@/renderer/types";

// The shim reads getEngine()/getId() at commit time; hand it the per-test engine.
let liveEngine: DocumentEngine | null = null;

beforeAll(async () => {
  await getWasmExportModule();
  installFacadeCommitShim({
    getEngine: () => liveEngine as never,
    getDocId: () => liveEngine?.getId() ?? "default",
  });
});

beforeEach(() => {
  localStorage.setItem("photrez.facade", "1");
  localStorage.setItem("photrez.facadeAuthority", "wasm");
});

afterEach(() => {
  localStorage.removeItem("photrez.facade");
  localStorage.removeItem("photrez.facadeAuthority");
  liveEngine = null;
  __resetFacadeRegistryForTests();
  vi.restoreAllMocks();
});

// Seed a doc with one facade-routed ("Owned") layer so a later routed op has a
// facade-owned target to route through.
async function setupDoc(id: string) {
  const engine = new DocumentEngine(id, id, 800, 600);
  const facade = getFacade(id);
  await seedFacadeFromEngine(engine as never, facade);
  const snap = await facade.addLayer("Owned", 100, 100, 0);
  engine.applyFacadeSnapshot(snap as never);
  const ownedId = facade.snapshot.layers.find((l) => l.name === "Owned")!.id;
  return { engine, facade, ownedId };
}

describe("notifyChange is the facade projection choke point (photrez.facade=1)", () => {
  it("a cross-document copy refreshes the TARGET facade while ANOTHER doc is active", async () => {
    const sourceId = "docTabSrc";
    const targetId = "docTabTgt";
    const sourceEngine = new DocumentEngine(sourceId, sourceId, 800, 600);
    const sourceLayer = sourceEngine.addLayer("Src", 120, 90);

    const { engine: targetEngine, facade, ownedId } = await setupDoc(targetId);
    // The commit shim resolves the ACTIVE engine (EditorShell), which is the
    // source tab here. Only the target's own notifyChange can refresh its facade.
    liveEngine = sourceEngine;

    const targetHistory = new CommandHistory();
    const ws = {
      getEngine: (id: string) => (id === sourceId ? sourceEngine : targetEngine),
      getHistory: (id: string) => (id === targetId ? targetHistory : null),
      getActiveDocumentId: () => sourceId,
      isFull: () => false,
      addDocument: () => {},
    };
    const { newLayerId } = addLayerFromCrossDoc(
      { version: 1, sourceDocId: sourceId, layerId: sourceLayer.id, sourceName: "Src", isAltPressed: false },
      { type: "tab", docId: targetId },
      { x: 0, y: 0 },
      ws as never,
    );
    const movedId = newLayerId!;
    expect(movedId).toBeTruthy();

    await vi.waitFor(() => {
      expect(facade.snapshot.layers.map((l) => l.id)).toContain(movedId);
    });

    // A later routed projection on the target keeps the moved-in layer.
    const r = await commitFacadeOpacity(targetEngine as never, [ownedId], 0.9);
    expect(r.status).toBe("applied");
    expect(targetEngine.getLayers().map((l) => l.id)).toContain(movedId);
    expect(targetEngine.getLayers().map((l) => l.id)).toContain(ownedId);
  });

  it("an adjustment bake publishes POST-bake values, not the pre-bake snapshot", async () => {
    const docId = "docBakePost";
    const { engine, facade, ownedId } = await setupDoc(docId);
    liveEngine = engine;

    const bitmap = { width: 100, height: 100, close: vi.fn() } as unknown as ImageBitmap;
    engine.setLayerImageBitmap(ownedId, bitmap);
    engine.applyBasicAdjustment(ownedId, { brightness: 10, contrast: 0, saturation: 0 });
    expect(engine.getLayer(ownedId)!.hasAdjustments).toBe(true);

    const history = new CommandHistory();
    history.commit(engine.snapshot(), "Apply Adjustment");
    // Let the commit shim settle so the snapshot carries the PRE-bake value. The
    // bake below must overwrite it with the post-bake value.
    await vi.waitFor(() => {
      expect(facade.snapshot.layers.find((l) => l.id === ownedId)!.hasAdjustments).toBe(true);
    });

    const renderer = { bakeLayerToBitmap: () => bitmap } as unknown as RenderBackend;
    await engine.commitBasicAdjustment(ownedId, renderer);

    expect(engine.getLayer(ownedId)!.hasAdjustments).toBe(false);
    expect(facade.snapshot.layers.find((l) => l.id === ownedId)!.hasAdjustments).toBe(false);
  });

  it("notifyChange alone updates the snapshot (no commit, no shim)", async () => {
    const docId = "docNotifyOnly";
    const engine = new DocumentEngine(docId, docId, 800, 600);
    const facade = getFacade(docId);
    engine.addLayer("Base");
    await seedFacadeFromEngine(engine as never, facade);
    const before = facade.snapshot.layers.length;
    // No liveEngine: the commit shim must not be the reason the snapshot updates.
    liveEngine = null;

    const added = engine.addLayer("NotifyOnly");

    expect(facade.snapshot.layers.map((l) => l.id)).toContain(added.id);
    expect(facade.snapshot.layers.length).toBe(before + 1);
  });

  it("restore() carries the restored model's canvas size into the snapshot", async () => {
    const docId = "docRestoreDims";
    const engine = new DocumentEngine(docId, docId, 800, 600);
    const facade = getFacade(docId);
    engine.addLayer("Base");
    await seedFacadeFromEngine(engine as never, facade);
    liveEngine = null;
    const preResize = engine.snapshot();

    engine.resizeCanvas(1024, 768);
    expect(facade.snapshot.width).toBe(1024);
    expect(facade.snapshot.height).toBe(768);

    engine.restore(preResize);

    expect(engine.getWidth()).toBe(800);
    expect(engine.getHeight()).toBe(600);
    expect(facade.snapshot.width).toBe(800);
    expect(facade.snapshot.height).toBe(600);
  });
});

describe("photrez.facade=0 opt-out leaves the snapshot untouched through notifyChange + commit + restore", () => {
  beforeEach(() => {
    localStorage.setItem("photrez.facade", "0");
  });

  it("a commit + legacy add + restore does not change the projection snapshot", async () => {
    const docId = "docOffAll";
    const engine = new DocumentEngine(docId, docId, 800, 600);
    const facade = getFacade(docId);
    engine.addLayer("Base");
    await seedFacadeFromEngine(engine as never, facade);
    const before = JSON.stringify(facade.snapshot);
    liveEngine = engine;

    const history = new CommandHistory();
    const preAdd = engine.snapshot();
    history.commit(preAdd, "Flag Off Commit");
    engine.addLayer("Legacy");
    engine.resizeCanvas(1024, 768);
    engine.restore(preAdd);

    expect(JSON.stringify(facade.snapshot)).toBe(before);
  });
});
