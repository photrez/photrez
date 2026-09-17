// Overlapping-commit contract for the committed numeric transform funnel.
//
// The facade keeps ONE transient transform slot per document and only advances its
// rendered version after a command resolves, so two commits started in the same tick
// (two quick field submits, a keyboard flip issued while an earlier commit is still in
// flight, one Align batch over several layers) cannot both be correct without
// ordering. A commit that starts while a pointer gesture owns the slot is worse: it
// overwrites the gesture's entry, and the gesture's own commitTransform then finds an
// empty slot.
//
// These tests drive the funnel for real: a real DocumentEngine, the real snapshot
// projection and the real protocol arm (no facade double), so every assertion below
// observes the committed model rather than a stubbed return value.
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { DocumentEngine } from "@/engine/document";
import type { Transform2D } from "@/engine/types";
import {
  __resetFacadeRegistryForTests,
  facadeCommitNumericTransform,
  getFacade,
  seedFacadeFromEngine,
} from "../facadeRegistry";
import { routeNumericTransform } from "@/components/editor/layers/transformRouting";
import { showToast } from "@/components/editor/Toast";
import { getWasmExportModule } from "@/components/editor/wasmExport";

vi.mock("@/components/editor/Toast", () => ({ showToast: vi.fn() }));

let wasm: { protocol_reset: (docId: string) => void } | null = null;
const usedDocs: string[] = [];

beforeAll(async () => {
  wasm = await getWasmExportModule();
});

beforeEach(() => {
  localStorage.setItem("photrez.facade", "1");
  localStorage.setItem("photrez.facadeAuthority", "wasm");
});

afterEach(() => {
  // The native documents outlive the facade registry, so every id this file used
  // goes back to its empty state. Without this, a later test that reuses an id would
  // observe a version it never wrote.
  for (const id of usedDocs) wasm?.protocol_reset(id);
  usedDocs.length = 0;
  localStorage.removeItem("photrez.facade");
  localStorage.removeItem("photrez.facadeAuthority");
  __resetFacadeRegistryForTests();
  (globalThis as unknown as Record<string, () => void>).__clearFacadeOwnedForTests?.();
  vi.restoreAllMocks();
});

async function docWithLayers(docId: string, count: number) {
  usedDocs.push(docId);
  const engine = new DocumentEngine(docId, "Race", 800, 600);
  const facade = getFacade(docId);
  await seedFacadeFromEngine(engine as never, facade);
  const ids: string[] = [];
  for (let i = 0; i < count; i++) {
    const name = `Owned ${i}`;
    engine.applyFacadeSnapshot(await facade.addLayer(name));
    const layer = engine.getLayers().find((l) => l.name === name);
    if (!layer) throw new Error("setup: facade-created layer did not project into the engine");
    ids.push(layer.id);
  }
  return { engine, facade, ids };
}

function transformOf(engine: DocumentEngine, layerId: string): Transform2D {
  return engine.getLayer(layerId)!.transform;
}

type FacadeSnapshotView = { version: number; layers: Array<Record<string, unknown>> };

function facadeView(facade: { snapshot: unknown }): FacadeSnapshotView {
  return facade.snapshot as FacadeSnapshotView;
}

// Land a model change in the gap between an issued edit and the hop that will run it:
// the layer goes away (or locks) while an earlier commit is still in flight, which is
// what a Delete keypress or a lock toggle does in the app. The facade's own view moves
// with the engine model, so a projection re-read cannot resurrect the layer.
function projectLayers(
  engine: DocumentEngine,
  facade: { snapshot: unknown },
  map: (layers: Array<Record<string, unknown>>) => Array<Record<string, unknown>>,
): void {
  const view = facadeView(facade);
  view.layers = map(view.layers);
  engine.applyFacadeSnapshot({ ...view, version: view.version + 1 } as never);
}

describe("overlapping numeric transform commits", () => {
  it("two commits on different layers, started without awaiting, both land on their own layer", async () => {
    const docId = "race-two-layers";
    const { engine, facade, ids } = await docWithLayers(docId, 2);
    const [a, b] = ids;
    const versionBefore = facade.renderedVersion;

    const first = facadeCommitNumericTransform(engine, a, { x: 100 });
    const second = facadeCommitNumericTransform(engine, b, { y: 200 });

    await expect(Promise.all([first, second])).resolves.toEqual([true, true]);
    expect(transformOf(engine, a).x).toBe(100);
    expect(transformOf(engine, a).y).toBe(0);
    expect(transformOf(engine, b).y).toBe(200);
    expect(transformOf(engine, b).x).toBe(0);
    // Exactly one committed command each.
    expect(facade.renderedVersion).toBe(versionBefore + 2);
  });

  it("two commits on one layer, started without awaiting: both land, final value wins", async () => {
    const docId = "race-same-layer";
    const { engine, facade, ids } = await docWithLayers(docId, 1);
    const versionBefore = facade.renderedVersion;

    const first = facadeCommitNumericTransform(engine, ids[0], { x: 150 });
    const second = facadeCommitNumericTransform(engine, ids[0], { x: 220 });

    await expect(Promise.all([first, second])).resolves.toEqual([true, true]);
    expect(transformOf(engine, ids[0]).x).toBe(220);
    expect(facade.renderedVersion).toBe(versionBefore + 2);
  });

  it("a queued duplicate of the value the earlier commit already projected resolves false", async () => {
    const docId = "race-noop";
    const { engine, ids } = await docWithLayers(docId, 1);

    const first = facadeCommitNumericTransform(engine, ids[0], { x: 150 });
    const second = facadeCommitNumericTransform(engine, ids[0], { x: 150 });

    await expect(first).resolves.toBe(true);
    await expect(second).resolves.toBe(false);
    expect(transformOf(engine, ids[0]).x).toBe(150);
  });

  it("two queued flips both apply: the second negates what the first projected", async () => {
    const docId = "race-flip-twice-queued";
    const { engine, ids } = await docWithLayers(docId, 1);
    const original = { ...transformOf(engine, ids[0]) };

    // Both clicks happen inside one commit window. Deciding the flag at click time
    // would send flipH:true twice and silently drop the second as unchanged, so the
    // patch is resolved against the transform each hop actually reads.
    const first = facadeCommitNumericTransform(engine, ids[0], (cur) => ({ flipH: !cur.flipH }));
    const second = facadeCommitNumericTransform(engine, ids[0], (cur) => ({ flipH: !cur.flipH }));

    await expect(Promise.all([first, second])).resolves.toEqual([true, true]);
    expect(transformOf(engine, ids[0]).flipH).toBe(original.flipH);
  });

  it("a commit started during a live pointer gesture is refused and the gesture survives", async () => {
    const docId = "race-gesture";
    const { engine, facade, ids } = await docWithLayers(docId, 2);
    const [dragId, otherId] = ids;
    // This is exactly what the handle drag does at pointerdown/pointermove.
    facade.beginTransform(dragId, { ...transformOf(engine, dragId) });
    facade.updateTransform({ ...transformOf(engine, dragId), x: 500 });

    const numeric = await facadeCommitNumericTransform(engine, otherId, { x: 100 })
      .then((ok) => ({ ok }))
      .catch((e: Error) => ({ err: e.message }));

    // pointerup: the gesture must still find its own entry in the slot.
    const snap = await facade.commitTransform();
    expect(snap).not.toBeNull();
    engine.applyFacadeSnapshot(snap as never);
    expect(transformOf(engine, dragId).x).toBe(500);
    expect(transformOf(engine, otherId).x).toBe(0);
    expect(numeric).toEqual({ err: expect.stringContaining(otherId) });
  });

  it("missing and locked layers keep resolving false without a command", async () => {
    const docId = "race-guards";
    const { engine, facade, ids } = await docWithLayers(docId, 2);
    const versionBefore = facade.renderedVersion;
    const commitSpy = vi.spyOn(facade, "commitTransform");

    await expect(facadeCommitNumericTransform(engine, "layer-missing", { x: 10 })).resolves.toBe(false);
    projectLayers(engine, facade, (layers) =>
      layers.map((l) => (l.id === ids[1] ? { ...l, locked: true } : l)),
    );
    await expect(facadeCommitNumericTransform(engine, ids[1], { x: 10 })).resolves.toBe(false);
    expect(commitSpy).not.toHaveBeenCalled();
    expect(facade.renderedVersion).toBe(versionBefore);
  });

  it("a delete that lands while an edit is queued fails loudly instead of vanishing", async () => {
    const docId = "race-delete-queued";
    const { engine, facade, ids } = await docWithLayers(docId, 2);

    const first = facadeCommitNumericTransform(engine, ids[0], { x: 10 });
    const second = facadeCommitNumericTransform(engine, ids[1], { x: 20 });
    // Same tick, before any hop can settle: the layer disappears while `second` waits.
    projectLayers(engine, facade, (layers) => layers.filter((l) => l.id !== ids[1]));

    await expect(first).resolves.toBe(true);
    await expect(second).rejects.toThrow(/no longer exists/);
  });

  it("a lock toggle that lands while an edit is queued fails loudly instead of vanishing", async () => {
    const docId = "race-lock-queued";
    const { engine, facade, ids } = await docWithLayers(docId, 2);

    const first = facadeCommitNumericTransform(engine, ids[0], { x: 10 });
    const second = facadeCommitNumericTransform(engine, ids[1], { x: 20 });
    projectLayers(engine, facade, (layers) =>
      layers.map((l) => (l.id === ids[1] ? { ...l, locked: true } : l)),
    );

    await expect(first).resolves.toBe(true);
    await expect(second).rejects.toThrow(/is locked/);
  });

  it("the queued failure reaches the caller as a visible error, not a silent no-op", async () => {
    const docId = "race-delete-visible";
    const { engine, facade, ids } = await docWithLayers(docId, 2);
    const requestRender = vi.fn();

    const first = facadeCommitNumericTransform(engine, ids[0], { x: 10 });
    const second = routeNumericTransform(engine, ids[1], { x: 20 }, { requestRender });
    projectLayers(engine, facade, (layers) => layers.filter((l) => l.id !== ids[1]));

    await expect(first).resolves.toBe(true);
    await expect(second).resolves.toBe("error");
    expect(showToast).toHaveBeenCalledWith(
      expect.stringContaining("no longer exists"),
      "error",
    );
    expect(requestRender).not.toHaveBeenCalled();
  });

  it("photrez.facade=0 opt-out: the funnel dispatches nothing even when a caller forgets the gate", async () => {
    const docId = "race-flag-off";
    const { engine, facade, ids } = await docWithLayers(docId, 1);
    const versionBefore = facade.renderedVersion;
    const commitSpy = vi.spyOn(facade, "commitTransform");
    localStorage.setItem("photrez.facade", "0");

    await expect(facadeCommitNumericTransform(engine, ids[0], { x: 150 })).resolves.toBe(false);

    expect(commitSpy).not.toHaveBeenCalled();
    expect(facade.renderedVersion).toBe(versionBefore);
    expect(transformOf(engine, ids[0]).x).toBe(0);
  });

  it("a hop queued behind a tail that never settles warns instead of parking silently", async () => {
    const docId = "race-stalled-tail";
    const { engine, ids } = await docWithLayers(docId, 2);
    const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});
    let fakeNow = 1_000;
    vi.spyOn(Date, "now").mockImplementation(() => fakeNow);

    const first = facadeCommitNumericTransform(engine, ids[0], { x: 10 });
    // Five minutes later the first hop still has not settled, and another edit is
    // issued for the same document.
    fakeNow = 1_000 + 300_000;
    const second = facadeCommitNumericTransform(engine, ids[1], { x: 20 });

    expect(warnSpy).toHaveBeenCalledWith(expect.stringContaining(docId));
    await Promise.all([first, second]);
  });
});
