// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Cross-document drop onto a facade-enabled document, followed by a routed op
// ON THE DROPPED LAYER ITSELF (photrez.facade=1).
//
// The cross-doc branch of addLayerFromCrossDoc creates layers engine-direct
// and commits TS history. The commit shim mirrors every history.commit into
// the engine (recordExternalTransitionFor) and each engine mutation refreshes
// the facade projection snapshot through the post-mutation choke point
// (DocumentEngine notifyChange), while the push keeps the wasm mirror in step.
// A later routed projection marks every snapshotted layer owned, so a routed
// op that names the dropped layer must apply instead of silently no-op-ing.
//
// These tests drive the REAL wasm mirror + the REAL facade funnel; the commit
// shim is the production entry point every legacy op calls (EditorShell
// installs it once at boot). No hand-written engine mock.
//
// Live reachability: useCanvasLayerDrag.ts calls addLayerFromCrossDoc on a
// cross-document canvas drop, and useCanvasDrop.ts calls it on an OS/canvas
// drop onto another document. Both are live production drag paths.

import { describe, it, expect, vi, beforeEach, afterEach, beforeAll } from "vitest";
import { DocumentEngine, isFacadeOwnedLayer } from "@/engine/document";
import { CommandHistory } from "@/engine/history";
import { DEFAULT_TEXT_DATA, type TextData } from "@/engine/textTypes";
import { getWasmExportModule } from "@/components/editor/wasmExport";
import { addLayerFromCrossDoc } from "@/components/editor/crossDocLayerOps";
import {
  commitFacadeAdjustment,
  commitFacadeBlendMode,
  commitFacadeLock,
  commitFacadeOpacity,
  commitFacadeParams,
  commitFacadeRename,
  commitFacadeVisibility,
  getFacade,
  installFacadeCommitShim,
  seedFacadeFromEngine,
  __resetFacadeRegistryForTests,
} from "@/lib/protocol/facadeRegistry";
import { getSnapshot } from "@/lib/protocol/bridge";

// The shim reads getEngine() at commit time; hand it the per-test engine.
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
  (globalThis as unknown as Record<string, () => void>).__clearFacadeOwnedForTests?.();
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

function dropFromOtherDoc(sourceId: string, targetId: string, targetEngine: DocumentEngine) {
  const sourceEngine = new DocumentEngine(sourceId, sourceId, 800, 600);
  const sourceLayer = sourceEngine.addLayer("Src", 120, 90);
  const targetHistory = new CommandHistory();
  const ws = {
    getEngine: (id: string) => (id === sourceId ? sourceEngine : targetEngine),
    getHistory: (id: string) => (id === targetId ? targetHistory : null),
    getActiveDocumentId: () => targetId,
    isFull: () => false,
    addDocument: () => {},
  };
  const { newLayerId } = addLayerFromCrossDoc(
    { version: 1, sourceDocId: sourceId, layerId: sourceLayer.id, sourceName: "Src", isAltPressed: false },
    { type: "canvas" },
    { x: 0, y: 0 },
    ws as never,
  );
  expect(newLayerId).toBeTruthy();
  return newLayerId!;
}

describe("cross-doc drop then a routed op on the dropped layer (photrez.facade=1)", () => {
  it("a routed op on the dropped layer fails loud, never silent (native set lacks it)", async () => {
    const targetId = "docDropFollowTgt";
    const { engine: targetEngine, facade, ownedId } = await setupDoc(targetId);
    liveEngine = targetEngine;

    const droppedId = dropFromOtherDoc("docDropFollowSrc", targetId, targetEngine);

    // The mirror settles: the dropped layer reaches the TS-side snapshot.
    await vi.waitFor(() => {
      expect(facade.snapshot.layers.map((l) => l.id)).toContain(droppedId);
    });

    // One routed op on an unrelated owned layer. Its projection restates the
    // whole layer vector, which marks every snapshotted layer owned.
    const first = await commitFacadeOpacity(targetEngine as never, [ownedId], 0.9);
    expect(first.status).toBe("applied");
    expect(targetEngine.getLayer(ownedId)?.opacity).toBeCloseTo(0.9);
    expect(isFacadeOwnedLayer(droppedId)).toBe(true);

    // But the command engine never received the dropped layer: the engine
    // learns layers only through facade commands (or a native re-push), and
    // neither the history mirror nor the TS snapshot refresh teaches it.
    const proto = await getSnapshot(targetId);
    expect((proto.layers ?? []).map((l) => l.id)).not.toContain(droppedId);

    // So a routed op naming the dropped layer must fail loud instead of
    // reporting applied while the value never lands. Pre-guard code resolved
    // { status: "applied" } here with the model left at opacity 1.
    await expect(
      commitFacadeOpacity(targetEngine as never, [droppedId], 0.25),
    ).rejects.toThrow(/does not hold this layer/);
    expect(targetEngine.getLayer(droppedId)?.opacity).toBeCloseTo(1);
  });

  it("before any marking projection the dropped layer routes legacy (fall-through, not silent)", async () => {
    const targetId = "docDropLegacyTgt";
    const { engine: targetEngine, facade } = await setupDoc(targetId);
    liveEngine = targetEngine;

    const droppedId = dropFromOtherDoc("docDropLegacySrc", targetId, targetEngine);

    await vi.waitFor(() => {
      expect(facade.snapshot.layers.map((l) => l.id)).toContain(droppedId);
    });

    // Not yet owned, so the funnel declines and the caller keeps its untouched
    // legacy path. This pins the defeat for the test above: without the marking
    // projection the same call cannot report applied.
    expect(isFacadeOwnedLayer(droppedId)).toBe(false);
    const r = await commitFacadeOpacity(targetEngine as never, [droppedId], 0.5);
    expect(r.status).toBe("legacy");
  });
});

describe("facade flag OFF is byte-identical (photrez.facade unset)", () => {
  beforeEach(() => {
    localStorage.removeItem("photrez.facade");
  });

  it("a cross-doc drop leaves the facade projection snapshot untouched", async () => {
    const targetId = "docDropOffTgt";
    const engine = new DocumentEngine(targetId, targetId, 800, 600);
    const facade = getFacade(targetId);
    engine.addLayer("Base");
    await seedFacadeFromEngine(engine as never, facade);
    const before = JSON.stringify(facade.snapshot);
    liveEngine = engine;

    dropFromOtherDoc("docDropOffSrc", targetId, engine);

    expect(JSON.stringify(facade.snapshot)).toBe(before);
  });
});

// Each sibling funnel shares the opacity funnel's silent-success shape: after
// the marking projection the dropped layer is facade-owned but the command
// engine never received it, so without a settled-value check the funnel would
// report applied while the model is unchanged. One case per funnel below.
describe("cross-doc drop then each sibling funnel fails loud on the dropped layer (photrez.facade=1)", () => {
  async function setupMarkedDrop(targetId: string, sourceId: string) {
    const { engine: targetEngine, facade, ownedId } = await setupDoc(targetId);
    liveEngine = targetEngine;
    const droppedId = dropFromOtherDoc(sourceId, targetId, targetEngine);
    await vi.waitFor(() => {
      expect(facade.snapshot.layers.map((l) => l.id)).toContain(droppedId);
    });
    const first = await commitFacadeOpacity(targetEngine as never, [ownedId], 0.9);
    expect(first.status).toBe("applied");
    expect(isFacadeOwnedLayer(droppedId)).toBe(true);
    const proto = await getSnapshot(targetId);
    expect((proto.layers ?? []).map((l) => l.id)).not.toContain(droppedId);
    return { targetEngine, droppedId };
  }

  function boxed(content: string): TextData {
    return { ...DEFAULT_TEXT_DATA, content, boxMode: "area", boxWidth: 10, boxHeight: 10 };
  }

  it("visibility: throws and the model keeps visible true", async () => {
    const { targetEngine, droppedId } = await setupMarkedDrop("docDropFollowVis", "docDropFollowVisSrc");
    await expect(commitFacadeVisibility(targetEngine as never, [droppedId], false)).rejects.toThrow(
      /does not hold this layer/,
    );
    expect(targetEngine.getLayer(droppedId)?.visible).toBe(true);
  });

  it("rename: throws and the model keeps its name", async () => {
    const { targetEngine, droppedId } = await setupMarkedDrop("docDropFollowRen", "docDropFollowRenSrc");
    await expect(commitFacadeRename(targetEngine as never, [droppedId], "Renamed by guard test")).rejects.toThrow(
      /does not hold this layer/,
    );
    expect(targetEngine.getLayer(droppedId)?.name).not.toBe("Renamed by guard test");
  });

  it("lock: throws and the model keeps unlocked", async () => {
    const { targetEngine, droppedId } = await setupMarkedDrop("docDropFollowLock", "docDropFollowLockSrc");
    await expect(commitFacadeLock(targetEngine as never, [droppedId], "base", true)).rejects.toThrow(
      /does not hold this layer/,
    );
    expect(targetEngine.getLayer(droppedId)?.locked).toBe(false);
  });

  it("blend mode: throws and the model keeps normal", async () => {
    const { targetEngine, droppedId } = await setupMarkedDrop("docDropFollowBlend", "docDropFollowBlendSrc");
    await expect(commitFacadeBlendMode(targetEngine as never, [droppedId], "multiply")).rejects.toThrow(
      /does not hold this layer/,
    );
    expect(targetEngine.getLayer(droppedId)?.blendMode).toBe("normal");
  });

  it("adjustment: throws and the model keeps no adjustment", async () => {
    const { targetEngine, droppedId } = await setupMarkedDrop("docDropFollowAdj", "docDropFollowAdjSrc");
    await expect(
      commitFacadeAdjustment(targetEngine as never, [droppedId], { brightness: 20, contrast: 0, saturation: 0 }),
    ).rejects.toThrow(/does not hold this layer/);
    expect(targetEngine.getLayer(droppedId)?.basicAdjustment).toBeUndefined();
  });

  it("params: throws and the model keeps no text data", async () => {
    const { targetEngine, droppedId } = await setupMarkedDrop("docDropFollowParams", "docDropFollowParamsSrc");
    await expect(commitFacadeParams(targetEngine as never, [droppedId], { textData: boxed("x") })).rejects.toThrow(
      /does not hold this layer/,
    );
    expect(targetEngine.getLayer(droppedId)?.textData).toBeUndefined();
  });
});

// The command engine limits each adjustment channel to [-100, 100], so an
// out-of-range send on a layer the engine holds is a working op that lands
// limited. The guard must compare against the limited value, not the raw send.
describe("adjustment guard honors the arm channel limits (photrez.facade=1)", () => {
  it("brightness 150 on an owned layer applies and settles at 100", async () => {
    const { engine: targetEngine, ownedId } = await setupDoc("docAdjClampProbe");
    liveEngine = targetEngine;

    const r = await commitFacadeAdjustment(targetEngine as never, [ownedId], {
      brightness: 150,
      contrast: 0,
      saturation: 0,
    });
    expect(r.status).toBe("applied");
    expect(targetEngine.getLayer(ownedId)?.basicAdjustment).toEqual({
      brightness: 100,
      contrast: 0,
      saturation: 0,
    });
  });
});

// The command engine limits opacity to [0, 1] before storing, so an
// out-of-range send on a layer the engine holds is a working op that lands
// limited. The guard must compare against the limited value, not the raw send.
describe("opacity guard honors the arm limits (photrez.facade=1)", () => {
  it("opacity 1.5 on an owned layer applies and settles at 1", async () => {
    const { engine: targetEngine, ownedId } = await setupDoc("docOpacityClampProbe");
    liveEngine = targetEngine;

    const r = await commitFacadeOpacity(targetEngine as never, [ownedId], 1.5);
    expect(r.status).toBe("applied");
    expect(targetEngine.getLayer(ownedId)?.opacity).toBeCloseTo(1);
  });
});

// A sent value that already equals the settled value must not mask an id the
// engine never received. The dropped layer rests at opacity 1, so sending 1
// (and 1.5, which the arm limits to 1) coincides and the old value-only check
// resolved applied. The membership check must still throw.
describe("coincident value still fails loud on an engine-missing id (photrez.facade=1)", () => {
  it("opacity 1 on the marked-but-engine-missing dropped id throws", async () => {
    const targetId = "docDropCoincidentTgt";
    const { engine: targetEngine, facade, ownedId } = await setupDoc(targetId);
    liveEngine = targetEngine;
    const droppedId = dropFromOtherDoc("docDropCoincidentSrc", targetId, targetEngine);
    await vi.waitFor(() => {
      expect(facade.snapshot.layers.map((l) => l.id)).toContain(droppedId);
    });
    const first = await commitFacadeOpacity(targetEngine as never, [ownedId], 0.9);
    expect(first.status).toBe("applied");
    expect(isFacadeOwnedLayer(droppedId)).toBe(true);
    const proto = await getSnapshot(targetId);
    expect((proto.layers ?? []).map((l) => l.id)).not.toContain(droppedId);
    expect(targetEngine.getLayer(droppedId)?.opacity).toBeCloseTo(1);

    await expect(commitFacadeOpacity(targetEngine as never, [droppedId], 1)).rejects.toThrow(
      /does not hold this layer/,
    );
    await expect(commitFacadeOpacity(targetEngine as never, [droppedId], 1.5)).rejects.toThrow(
      /does not hold this layer/,
    );
    expect(targetEngine.getLayer(droppedId)?.opacity).toBeCloseTo(1);
  });
});
