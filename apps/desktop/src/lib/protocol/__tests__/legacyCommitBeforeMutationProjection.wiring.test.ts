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
  getHistoryProjection,
  installFacadeCommitShim,
  recordExternalTransitionFor,
  seedFacadeFromEngine,
  __resetFacadeRegistryForTests,
} from "@/lib/protocol/facadeRegistry";

// The shim reads getEngine()/getId() at commit time; hand it the per-test engine.
let liveEngine: DocumentEngine | null = null;
// Counts how often the shim decided to mirror: providers.getEngine() is reached
// only AFTER the alreadyRecordedInRust skip, so this is the shim's own decision,
// observable without the Tauri native protocol the cursor read needs.
let engineProbeCount = 0;

beforeAll(async () => {
  await getWasmExportModule();
  // Install the production shim once (shimInstalled is module-sticky; production
  // installs it once at EditorShell boot).
  installFacadeCommitShim({
    getEngine: () => {
      engineProbeCount++;
      return liveEngine as never;
    },
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

describe("photrez.facade=0 opt-out is byte-identical", () => {
  beforeEach(() => {
    localStorage.setItem("photrez.facade", "0");
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

// The commit shim mirrors legacy commits into the unified cursor as External
// entries. A pixel-path commit (brush/fill/adjustment bake) calls
// CommandHistory.commit with alreadyRecordedInRust=true because Rust already owns
// that stroke's Pixel entry (rust_pixels_write_region). Mirroring it AGAIN as
// External gives one stroke two cursor entries, and the next undo steps the
// External mirror instead of the Rust pixels. Counted off the real cursor, not a
// spy: the assertion must be able to see a mirror the shim really recorded.
describe("commit shim skips the External mirror for an already-Rust-recorded commit", () => {
  const externalCount = async (docId: string) => {
    const proj = await getHistoryProjection(docId);
    return proj.entries.filter((e) => String(e.origin).startsWith("external")).length;
  };

  it("alreadyRecordedInRust=true mirrors nothing; false and absent still mirror", async () => {
    const docId = "docPixelCommitMirror";
    const { engine } = await setupDoc(docId);
    liveEngine = engine;
    const history = new CommandHistory();
    const imperative = {
      layerId: engine.getLayers()[0].id,
      before: [],
      after: [],
    };

    history.commit(engine.snapshot(), "Brush Stroke", imperative as never, true);
    // Sleep once, then assert: the mirror is a fire-and-forget record, so poll
    // until the cursor is dirty would hide the failure. 50ms is long enough for an
    // unconditional mirror to land (the "false" commit below proves that).
    await new Promise((r) => setTimeout(r, 50));
    expect(await externalCount(docId)).toBe(0);

    // The mirror must still happen for commits Rust does not already own, or the
    // fix would be "return early always" rather than an honour-the-flag skip.
    history.commit(engine.snapshot(), "Move Layer");
    await vi.waitFor(async () => {
      expect(await externalCount(docId)).toBe(1);
    });

    // The imperative/3rd-argument path keeps mirroring: the tile memento it
    // carries is the only undo payload for that shape.
    history.commit(engine.snapshot(), "Add Shape", imperative as never);
    await vi.waitFor(async () => {
      expect(await externalCount(docId)).toBe(2);
    });
  });

  it("the 4th argument alone decides: a truthy imperative payload changes nothing", async () => {
    const docId = "docPixelCommitArg";
    const { engine } = await setupDoc(docId);
    liveEngine = engine;
    const history = new CommandHistory();

    // Truthy imperative payload, explicit false: mirrors, as before.
    history.commit(engine.snapshot(), "Add Shape", { layerId: "x", before: [], after: [] } as never, false);
    await vi.waitFor(async () => {
      expect(await externalCount(docId)).toBe(1);
    });

    // Explicit true: the pixel path's own shape, no mirror.
    history.commit(engine.snapshot(), "Brush Stroke", { layerId: "x", before: [], after: [] } as never, true);
    await new Promise((r) => setTimeout(r, 50));
    expect(await externalCount(docId)).toBe(1);
  });

  // The defect was measured with the authority flag absent, which is the shipped
  // default (isNativeAuthority() is true when localStorage has no entry). The file
  // beforeEach pins it to "wasm", so drop it here to cover the real default; the
  // shim branch does not read authority, so the outcome must be identical.
  // The defect was measured with photrez.facadeAuthority absent, which is the
  // shipped default (isNativeAuthority() is true with no entry, see
  // bridge.nativeAuthority.test.ts). The cursor READ path under native authority
  // goes through the Tauri native protocol, which a jsdom test has no runtime for,
  // so the cursor itself is not observable here - the shim's own decision is.
  // The shim reaches providers.getEngine() only after the alreadyRecordedInRust
  // skip, so the probe below reads the production branch directly and does not
  // depend on the authority flag.
  it("skips the mirror branch for a pixel commit under the default native authority", async () => {
    localStorage.removeItem("photrez.facadeAuthority");
    const docId = "docPixelCommitNative";
    const engine = new DocumentEngine(docId, docId, 800, 600);
    engine.addLayer("Base");
    liveEngine = engine;
    const history = new CommandHistory();

    engineProbeCount = 0;
    history.commit(engine.snapshot(), "Brush Stroke", { layerId: "x", before: [], after: [] } as never, true);
    await new Promise((r) => setTimeout(r, 50));
    expect(engineProbeCount).toBe(0);

    // The same authority setting must still mirror a commit Rust does not own,
    // or the zero above would pass because the branch is dead, not skipped.
    history.commit(engine.snapshot(), "Move Layer");
    await new Promise((r) => setTimeout(r, 50));
    expect(engineProbeCount).toBe(1);
  });
});
