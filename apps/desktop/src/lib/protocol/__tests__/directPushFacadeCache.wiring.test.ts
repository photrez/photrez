// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Direct pushModelToRust() call sites and the facade projection snapshot
// (photrez.facade=1).
//
// The projection snapshot is the layer vector the next routed op rebuilds the
// model from (applyFacadeSnapshot replaces model.layers with it). Two legacy
// mutators push to the Rust mirror DIRECTLY, without notifyChange:
// applyBasicAdjustment and clearBasicAdjustments (the non-bake adjustment path).
// Both mutate layer fields the projection snapshot carries
// (basicAdjustment/hasAdjustments), so each must republish the snapshot from the
// model; otherwise the snapshot keeps the PRE-mutation projection and the next
// routed op reverts the edit. A third direct push, setLayerImageBitmap, changes
// only the bitmap and the layer pixel size, neither of which the snapshot
// carries (see facadeProjection.ts), so it needs no refresh and has none.
// None of them fires the on-change callback either. (The dims-guard test below
// still calls setLayerImageBitmap, but only to move the MODEL size, which that
// method does set.)
//
// These tests drive the REAL wasm mirror + the REAL facade funnel; no engine
// mock. The commit shim is installed the way production installs it once at
// EditorShell boot.

import { describe, it, expect, vi, beforeEach, afterEach, beforeAll } from "vitest";
import { DocumentEngine } from "@/engine/document";
import { getWasmExportModule } from "@/components/editor/wasmExport";
import {
  getFacade,
  installFacadeCommitShim,
  recordExternalTransitionFor,
  seedFacadeFromEngine,
  __resetFacadeRegistryForTests,
} from "@/lib/protocol/facadeRegistry";

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
  localStorage.removeItem("photrez.facadeAuthority");
});

afterEach(() => {
  localStorage.removeItem("photrez.facade");
  localStorage.removeItem("photrez.facadeAuthority");
  liveEngine = null;
  __resetFacadeRegistryForTests();
  vi.restoreAllMocks();
});

function fakeBitmap(width = 100, height = 100): ImageBitmap {
  return { width, height, close: vi.fn() } as unknown as ImageBitmap;
}

// Seed a doc with one facade-routed ("Owned") layer so it is present in the
// facade snapshot the routed projection rebuilds from.
async function setupDoc(id: string) {
  const engine = new DocumentEngine(id, id, 800, 600);
  const facade = getFacade(id);
  await seedFacadeFromEngine(engine as never, facade);
  const snap = await facade.addLayer("Owned", 100, 100, 0);
  engine.applyFacadeSnapshot(snap as never);
  const ownedId = facade.snapshot.layers.find((l) => l.name === "Owned")!.id;
  return { engine, facade, ownedId };
}

describe("direct pushModelToRust() paths refresh the facade projection snapshot (photrez.facade=1)", () => {
  it("applyBasicAdjustment with no commit republishes the new adjustment", async () => {
    const { engine, facade, ownedId } = await setupDoc("docDirectAdjust");
    // No live engine: the commit shim must not be the reason the snapshot updates.
    liveEngine = null;
    engine.setLayerImageBitmap(ownedId, fakeBitmap());
    expect(facade.snapshot.layers.find((l) => l.id === ownedId)!.hasAdjustments).toBeFalsy();

    engine.applyBasicAdjustment(ownedId, { brightness: 10, contrast: 0, saturation: 0 });

    const projected = facade.snapshot.layers.find((l) => l.id === ownedId)!;
    expect(projected.hasAdjustments).toBe(true);
    expect(projected.basicAdjustment?.brightness).toBe(10);
  });

  it("clearBasicAdjustments with no commit clears the projected adjustment", async () => {
    const { engine, facade, ownedId } = await setupDoc("docDirectClear");
    liveEngine = null;
    engine.setLayerImageBitmap(ownedId, fakeBitmap());
    engine.applyBasicAdjustment(ownedId, { brightness: 12, contrast: 0, saturation: 0 });
    // Load the snapshot through an extra legacy record so the PRE-clear snapshot
    // is re-projected independently of the clear call under test.
    await recordExternalTransitionFor(
      "docDirectClear",
      { label: "Apply Adjustment", affectedLayerIds: [ownedId], snapshot: {} },
      engine as never,
    );
    expect(facade.snapshot.layers.find((l) => l.id === ownedId)!.hasAdjustments).toBe(true);

    engine.clearBasicAdjustments(ownedId);

    expect(engine.getLayer(ownedId)!.hasAdjustments).toBe(false);
    const projected = facade.snapshot.layers.find((l) => l.id === ownedId)!;
    expect(projected.hasAdjustments).toBe(false);
    expect(projected.basicAdjustment).toBeUndefined();
  });

  it("a routed metadata projection cannot revert a model size the pixel path moved", async () => {
    const { engine, facade, ownedId } = await setupDoc("docDimsGuard");
    liveEngine = null;
    engine.setLayerImageBitmap(ownedId, fakeBitmap());
    engine.resizeCanvas(1600, 1200);
    expect(engine.getWidth()).toBe(1600);
    expect(facade.snapshot.width).toBe(1600);

    // Metadata projection: carries the facade's size but is not dims-authoritative.
    engine.applyFacadeSnapshot(
      {
        version: facade.snapshot.version,
        layers: facade.snapshot.layers,
        width: facade.snapshot.width,
        height: facade.snapshot.height,
      } as never,
    );
    expect(engine.getWidth()).toBe(1600);
    expect(engine.getHeight()).toBe(1200);

    // A dims-authoritative projection (canvas command) still writes the size.
    engine.applyFacadeSnapshot(
      { version: facade.snapshot.version, layers: facade.snapshot.layers, width: 800, height: 600 } as never,
      { dimsAuthoritative: true },
    );
    expect(engine.getWidth()).toBe(800);
    expect(engine.getHeight()).toBe(600);
  });
});

describe("facade flag OFF leaves the snapshot untouched on the direct-push paths", () => {
  beforeEach(() => {
    localStorage.removeItem("photrez.facade");
  });

  it("adjustment apply/clear + bitmap replace do not change the snapshot", async () => {
    const { engine, facade, ownedId } = await setupDoc("docDirectOff");
    engine.setLayerImageBitmap(ownedId, fakeBitmap());
    const before = JSON.stringify(facade.snapshot);

    engine.applyBasicAdjustment(ownedId, { brightness: 10, contrast: 0, saturation: 0 });
    engine.clearBasicAdjustments(ownedId);
    engine.setLayerImageBitmap(ownedId, fakeBitmap(120, 120));

    expect(JSON.stringify(facade.snapshot)).toBe(before);
  });
});
