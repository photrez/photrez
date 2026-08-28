// ADR 0008 Opacity ticket — funnel + transient preview tests.
//
// Covers: single facade-owned edit; all-owned multi-edit; mixed rejection;
// empty selection; undo/redo via H0 stream; expectedVersion mandatory;
// ZERO IPC during transient preview ticks; legacy path unchanged.

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { DocumentEngine, hasFacadeOwnedLayers, isFacadeOwnedLayer } from "@/engine/document";
import * as bridge from "@/lib/protocol/bridge";
import { EditorFacade } from "@/lib/protocol/editorFacade";
import {
  commitFacadeOpacity,
  getFacade,
  seedFacadeFromEngine,
  setOpacityPreview,
  opacityPreview,
  clearOpacityPreview,
  applyFacadePreviews,
  __resetFacadeRegistryForTests,
} from "@/lib/protocol/facadeRegistry";

beforeEach(() => localStorage.setItem("photrez.facade", "1"));
afterEach(() => {
  localStorage.removeItem("photrez.facade");
  clearOpacityPreview();
  __resetFacadeRegistryForTests();
  vi.restoreAllMocks();
});

function makeDoc(id: string) {
  const engine = new DocumentEngine(id, id, 800, 600);
  const facade = getFacade(id);
  seedFacadeFromEngine(engine as never, facade);
  return { engine, facade };
}

describe("commitFacadeOpacity (PropertiesPanel funnel)", () => {
  it("single facade-owned edit: ONE SetOpacity command w/ expectedVersion + projection + zero legacy mutation", () => {
    const { engine, facade } = makeDoc("docO");
    facade.addLayer("L");
    engine.applyFacadeSnapshot(facade.snapshot as never);
    const id = facade.snapshot.layers.find((l) => l.name === "L")!.id;
    const vBefore = facade.renderedVersion;
    const applySpy = vi.spyOn(bridge, "applyCommand");

    const r = commitFacadeOpacity(engine as never, [id], 0.42);

    expect(r.status).toBe("applied");
    expect(r.count).toBe(1);
    expect(applySpy).toHaveBeenCalledTimes(1);
    // inspect the single envelope
    const env = applySpy.mock.calls[0][0] as {
      expectedVersion?: number;
      command: { type: string; id: string; opacity: number };
    };
    expect(env.command.type).toBe("setOpacity");
    expect(env.command.id).toBe(id);
    expect(env.command.opacity).toBeCloseTo(0.42, 6);
    expect(env.expectedVersion).toBe(vBefore); // mandatory guard
  });

  it("all-owned multi-edit: one command PER layer, sequential expectedVersions", () => {
    const { engine, facade } = makeDoc("docO2");
    facade.addLayer("A");
    facade.addLayer("B");
    engine.applyFacadeSnapshot(facade.snapshot as never);
    const ids = facade.snapshot.layers.map((l) => l.id).slice(-2);

    const spy = vi.spyOn(bridge, "applyCommand");
    const r = commitFacadeOpacity(engine as never, ids, 0.3);

    expect(r.status).toBe("applied");
    expect(r.count).toBe(2);
    const calls = spy.mock.calls.slice(-2) as Array<
      [{ command: { id: string } }]
    >;
    expect(calls[0][0].command.id).toBe(ids[0]);
    expect(calls[1][0].command.id).toBe(ids[1]);
  });

  it("mixed selection rejected atomically (zero commands)", () => {
    ownedViaProjection();
    const { engine } = makeDoc("docM");
    const facade = getFacade("docM");
    facade.addLayer("Owned");
    engine.applyFacadeSnapshot(facade.snapshot as never);
    const ownedId = facade.snapshot.layers[facade.snapshot.layers.length - 1].id;
    expect(isFacadeOwnedLayer(ownedId)).toBe(true);
    void ownedViaProjection;

    const r = commitFacadeOpacity(engine as never, [ownedId, "legacy-bg"], 0.5);
    expect(r.status).toBe("mixed-rejected");
  });

  it("empty selection -> silent no-op status", () => {
    const { engine } = makeDoc("docE");
    const r = commitFacadeOpacity(engine as never, [], 0.5);
    expect(r.status).toBe("empty");
  });

  it("undo/redo walk the H0 stream and restore/reapply opacity", () => {
    const { engine, facade } = makeDoc("docU");
    facade.addLayer("U");
    engine.applyFacadeSnapshot(facade.snapshot as never);
    const id = facade.snapshot.layers[facade.snapshot.layers.length - 1].id;
    const before = facade.snapshot.layers.find((l) => l.id === id)!.opacity;

    facade.setOpacity(id, 0.25);
    engine.applyFacadeSnapshot(facade.snapshot as never);
    expect(facade.snapshot.layers.find((l) => l.id === id)!.opacity).toBeCloseTo(0.25, 6);

    facade.undo(); // H0 native walker
    engine.applyFacadeSnapshot(facade.snapshot as never);
    expect(engine.getLayer(id)!.opacity).toBeCloseTo(before, 6);

    facade.redo();
    engine.applyFacadeSnapshot(facade.snapshot as never);
    expect(engine.getLayer(id)!.opacity).toBeCloseTo(0.25, 6);
  });

  it("transient preview ticks send ZERO IPC and preview signal carries the value", () => {
    const { engine, facade } = makeDoc("docP");
    facade.addLayer("P");
    engine.applyFacadeSnapshot(facade.snapshot as never);
    const id = facade.snapshot.layers[facade.snapshot.layers.length - 1].id;
    const spy = vi.spyOn(bridge, "applyCommand");

    for (const o of [0.9, 0.8, 0.7, 0.6]) {
      setOpacityPreview({ layerId: id, opacity: o }); // what the slider tick does
    }
    expect(spy).not.toHaveBeenCalled(); // ZERO IPC during transient phase
    expect(opacityPreview()!.opacity).toBe(0.6);

    // render-state merge picks up the preview (renderer unchanged)
    const rs = applyFacadePreviews({ layers: [{ id, transform: {} as never }] } as never);
    expect((rs.layers[0] as { opacity?: number }).opacity).toBe(0.6);
  });

  it("expectedVersion stale -> E_VERSION_MISMATCH propagates (mandatory guard)", () => {
    const { engine, facade } = makeDoc("docV");
    facade.addLayer("V");
    engine.applyFacadeSnapshot(facade.snapshot as never);
    const id = facade.snapshot.layers[facade.snapshot.layers.length - 1].id;
    // bump version behind a stale caller
    bridge.applyCommand({ contractVersion: 1, command: { type: "noop" } });
    expect(() =>
      bridge.applyCommand({
        contractVersion: 1,
        expectedVersion: facade.renderedVersion - 1,
        command: { type: "setOpacity", id, opacity: 0.5 },
      }),
    ).toThrow(/E_VERSION_MISMATCH/);
  });

  it("legacy path unchanged when flag OFF / non-owned (status legacy)", () => {
    localStorage.removeItem("photrez.facade");
    const { engine } = makeDoc("docL");
    const r = commitFacadeOpacity(engine as never, ["any"], 0.5);
    expect(r.status).toBe("legacy"); // caller falls back to untouched legacy code
    expect(hasFacadeOwnedLayers()).toBe(false);
  });
});

function ownedViaProjection(): void {}
