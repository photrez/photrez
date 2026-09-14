// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Facade projection snapshot vs. legacy commit-BEFORE-mutation (photrez.facade=1).
//
// Several legacy paths commit BEFORE the mutation they are about to make (Stamp
// Visible: history.commit then engine.addLayer; cross-document move/copy:
// history.commit then targetEngine.addLayer), so a layer vector captured at
// commit time is missing the layer the mutation adds. The projection snapshot the
// next routed op rebuilds the model from (applyFacadeSnapshot replaces
// model.layers with the snapshot vector) would therefore drop the legacy layer.
// The commit shim's own re-projection reads the model AFTER the record round-trip
// (post-mutation), so it does not carry the pre-mutation set; the refresh that
// keeps the snapshot in step is the post-mutation choke point
// (DocumentEngine.notifyChange), which runs after the mutation and is keyed by the
// mutating engine, so the projection keeps every layer the model holds.
//
// These tests drive the REAL wasm mirror + the REAL facade funnel; the commit
// shim is the production entry point every legacy op calls (EditorShell installs
// it once at boot). No hand-written engine mock.

import { describe, it, expect, vi, beforeEach, afterEach, beforeAll } from "vitest";
import { DocumentEngine } from "@/engine/document";
import { CommandHistory } from "@/engine/history";
import { getWasmExportModule } from "@/components/editor/wasmExport";
import { addLayerFromCrossDoc } from "@/components/editor/crossDocLayerOps";
import {
  commitFacadeOpacity,
  getFacade,
  installFacadeCommitShim,
  recordExternalTransitionFor,
  seedFacadeFromEngine,
  __resetFacadeRegistryForTests,
} from "@/lib/protocol/facadeRegistry";

// The shim reads getEngine()/getId() at commit time; hand it the per-test engine.
let liveEngine: DocumentEngine | null = null;

beforeAll(async () => {
  await getWasmExportModule();
  // Install the production shim once (shimInstalled is module-sticky; production
  // installs it once at EditorShell boot).
  installFacadeCommitShim({
    getEngine: () => liveEngine as never,
    getDocId: () => liveEngine?.getId() ?? "default",
  });
});

beforeEach(() => {
  localStorage.setItem("photrez.facade", "1");
  localStorage.removeItem("photrez.facadeAuthority");
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

describe("facade projection snapshot across legacy commit-before-mutation (photrez.facade=1)", () => {
  it("Stamp Visible shape: the legacy add survives a later routed projection", async () => {
    const { engine, facade, ownedId } = await setupDoc("docStampLoss");
    liveEngine = engine;
    const history = new CommandHistory();

    // Stamp Visible: the undo point is committed BEFORE the new layer is added.
    history.commit(engine.snapshot(), "Stamp Visible");
    const stamped = engine.addLayer("Stamp Visible", 800, 600);

    // The projection snapshot the next routed op rebuilds the model from must
    // learn the layer the legacy add just produced.
    await vi.waitFor(() => {
      expect(facade.snapshot.layers.map((l) => l.id)).toContain(stamped.id);
    });

    // A routed op on an unrelated facade-owned layer must not drop it.
    const r = await commitFacadeOpacity(engine as never, [ownedId], 0.9);
    expect(r.status).toBe("applied");
    const ids = engine.getLayers().map((l) => l.id);
    expect(ids).toContain(stamped.id);
    expect(ids).toContain(ownedId);
  });

  it("cross-document copy shape: the moved-in layer survives a later routed projection", async () => {
    const sourceId = "docCrossSrc";
    const targetId = "docCrossTgt";
    const sourceEngine = new DocumentEngine(sourceId, sourceId, 800, 600);
    const sourceLayer = sourceEngine.addLayer("Src", 120, 90);

    const { engine: targetEngine, facade, ownedId } = await setupDoc(targetId);
    liveEngine = targetEngine;

    const targetHistory = new CommandHistory();
    const ws = {
      getEngine: (id: string) => (id === sourceId ? sourceEngine : targetEngine),
      getHistory: (id: string) => (id === targetId ? targetHistory : null),
      getActiveDocumentId: () => targetId,
      isFull: () => false,
      addDocument: () => {},
    };
    const beforeIds = new Set(targetEngine.getLayers().map((l) => l.id));
    // Production cross-document copy: commits the target's undo point BEFORE the
    // add (crossDocLayerOps ~:174 commit then ~:185 add).
    const { newLayerId } = addLayerFromCrossDoc(
      { version: 1, sourceDocId: sourceId, layerId: sourceLayer.id, sourceName: "Src", isAltPressed: false },
      { type: "canvas" },
      { x: 0, y: 0 },
      ws as never,
    );
    expect(newLayerId).toBeTruthy();
    const movedId = newLayerId!;
    expect(beforeIds.has(movedId)).toBe(false);

    await vi.waitFor(() => {
      expect(facade.snapshot.layers.map((l) => l.id)).toContain(movedId);
    });

    const r = await commitFacadeOpacity(targetEngine as never, [ownedId], 0.9);
    expect(r.status).toBe("applied");
    const ids = targetEngine.getLayers().map((l) => l.id);
    expect(ids).toContain(movedId);
    expect(ids).toContain(ownedId);
  });

  it("undo of a legacy add does not let the removed id re-enter through the facade channel", async () => {
    const docId = "docUndoLegacyAdd";
    const engine = new DocumentEngine(docId, docId, 800, 600);
    const facade = getFacade(docId);
    const base = engine.addLayer("Base");
    await seedFacadeFromEngine(engine as never, facade);

    const preAdd = engine.snapshot();
    const added = engine.addLayer("Added");
    // addLayer's own notifyChange already projected the new layer into the
    // snapshot. This legacy commit AFTER the add re-projects the same vector; it
    // is the shape a second legacy op leaves behind, not the source of the update.
    await recordExternalTransitionFor(
      docId,
      { label: "Add Layer", affectedLayerIds: [added.id], snapshot: {} },
      engine as never,
    );
    expect(facade.snapshot.layers.map((l) => l.id)).toContain(added.id);

    // Undo the add: restore the pre-add model, which removes the layer.
    engine.restore(preAdd);
    expect(engine.getLayers().map((l) => l.id)).not.toContain(added.id);
    expect(engine.getLayers().map((l) => l.id)).toContain(base.id);

    // A later routed op must not resurrect the removed id from the snapshot.
    const pSnap = await facade.addLayer("P", 100, 100, 0);
    engine.applyFacadeSnapshot(pSnap as never);
    expect(engine.getLayers().map((l) => l.id)).not.toContain(added.id);
    expect(engine.getLayers().map((l) => l.id)).toContain(base.id);
  });
});

describe("facade flag OFF is byte-identical (photrez.facade unset)", () => {
  beforeEach(() => {
    localStorage.removeItem("photrez.facade");
  });

  it("a commit + legacy add + restore leaves the facade projection snapshot untouched", async () => {
    const engine = new DocumentEngine("docOffChoke", "docOffChoke", 800, 600);
    const facade = getFacade("docOffChoke");
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
