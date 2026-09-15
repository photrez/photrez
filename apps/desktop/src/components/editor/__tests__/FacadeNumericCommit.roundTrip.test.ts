// Round trip for the committed numeric transform funnel against the real Rust arm:
// funnel -> TransformLayer command -> RenderDelta -> snapshot projection -> engine
// model. The option-bar routing tests replace this funnel with a double, so this file
// is what proves a committed edit (including a flip) survives the whole round trip.
//
// Shape note: the engine is constructed directly rather than through
// WorkspaceManager.createBlankDocument. The blank-document path has already mutated the
// native document before a facade exists, so the first facade command of a freshly
// seeded document is rejected with E_VERSION_MISMATCH ("expected version 0 got 1")
// instead of committing - measured while writing these tests. Direct construction is
// the same shape `mixedHistory.test.ts` uses for this funnel.
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { DocumentEngine } from "@/engine/document";
import {
  __resetFacadeRegistryForTests,
  facadeCommitNumericTransform,
  getFacade,
  seedFacadeFromEngine,
} from "@/lib/protocol/facadeRegistry";
import { getWasmExportModule } from "../wasmExport";

let wasm: { protocol_reset: (docId: string) => void } | null = null;

beforeAll(async () => {
  wasm = await getWasmExportModule();
});

beforeEach(() => {
  localStorage.setItem("photrez.facade", "1");
});

afterEach(() => {
  localStorage.removeItem("photrez.facade");
  __resetFacadeRegistryForTests();
  (globalThis as unknown as Record<string, () => void>).__clearFacadeOwnedForTests?.();
  vi.restoreAllMocks();
});

async function ownedLayer(docId: string): Promise<{ engine: DocumentEngine; id: string }> {
  const engine = new DocumentEngine(docId, "Round Trip", 800, 600);
  const facade = getFacade(docId);
  await seedFacadeFromEngine(engine as never, facade);
  engine.applyFacadeSnapshot(await facade.addLayer("Owned"));
  const layer = engine.getLayers().find((l) => l.name === "Owned");
  if (!layer) throw new Error("setup: facade-created layer did not project into the engine");
  return { engine, id: layer.id };
}

describe("committed numeric transform round trip (real arm, no doubles)", () => {
  it("a position commit lands in the engine model through the projection", async () => {
    const { engine, id } = await ownedLayer("commit-position");
    const before = { ...engine.getLayer(id)!.transform };

    const ok = await facadeCommitNumericTransform(engine, id, { x: 150 });

    expect(ok).toBe(true);
    expect(engine.getLayer(id)!.transform.x).toBe(150);
    expect(engine.getLayer(id)!.transform.y).toBe(before.y);
    wasm?.protocol_reset("commit-position");
  });

  it("a flip sticks, survives the next numeric commit, and flips back", async () => {
    const { engine, id } = await ownedLayer("commit-flip");
    const original = { ...engine.getLayer(id)!.transform };

    expect(await facadeCommitNumericTransform(engine, id, { flipH: true })).toBe(true);
    expect(engine.getLayer(id)!.transform.flipH).toBe(true);

    // Without flipH in the projection, this next commit reads the stale model and
    // re-sends flipH:false, silently undoing the flip.
    expect(await facadeCommitNumericTransform(engine, id, { x: 40 })).toBe(true);
    expect(engine.getLayer(id)!.transform.x).toBe(40);
    expect(engine.getLayer(id)!.transform.flipH).toBe(true);

    // Flipping again negates the projected flag, not the one read at click time.
    expect(await facadeCommitNumericTransform(engine, id, { flipH: false })).toBe(true);
    expect(engine.getLayer(id)!.transform.flipH).toBe(false);
    expect({ ...engine.getLayer(id)!.transform }).toEqual({ ...original, x: 40 });
    wasm?.protocol_reset("commit-flip");
  });

  it("a locked layer is refused by the funnel before any command", async () => {
    const { engine, id } = await ownedLayer("commit-locked");
    const facade = getFacade("commit-locked");
    const snapshot = facade.snapshot as unknown as {
      version: number;
      layers: Array<Record<string, unknown>>;
    };
    engine.applyFacadeSnapshot({
      ...snapshot,
      version: snapshot.version + 1,
      layers: snapshot.layers.map((l) => (l.id === id ? { ...l, locked: true } : l)),
    } as never);
    const commitSpy = vi.spyOn(facade, "commitTransform");

    const ok = await facadeCommitNumericTransform(engine, id, { x: 150 });

    expect(ok).toBe(false);
    expect(commitSpy).not.toHaveBeenCalled();
    expect(engine.getLayer(id)!.transform.x).toBe(0);
    wasm?.protocol_reset("commit-locked");
  });
});
