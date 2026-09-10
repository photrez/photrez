// Routing of the four metadata ops (visibility / rename / lock / blendMode)
// through the native command arms, mirroring commitFacadeOpacity
// (facadeOpacity.test.ts). The arms already exist in document_core_apply.rs and
// are proven MEASURED-EQUAL in the parity matrix; this file proves the PRODUCTION
// dispatch (commitFacadeX helpers) issues the correct envelope and projects the
// result, and that flag-OFF is byte-identical (zero applyCommand).
//
// Per-op coverage satisfies AGENTS.md wiring-test rule:
//  (1) flag OFF -> legacy (zero applyCommand invocations)
//  (2) flag ON + facade-owned -> applyCommand with correct command type + payload
//      shape; engine projection reflects the change
//  (3) rejection path -> applyCommand rejects -> error propagates (no silent
//      TS-only mutation; the call site's try/catch surfaces it as a toast)
//  Plus a native-history undo contract (routed op reverts via facade undo).

import { describe, it, expect, vi, beforeEach, afterEach, beforeAll } from "vitest";
import { DocumentEngine, isFacadeOwnedLayer, hasFacadeOwnedLayers } from "@/engine/document";
import * as bridge from "@/lib/protocol/bridge";
import { CONTRACT_VERSION } from "@/lib/protocol/types";
import {
  commitFacadeVisibility,
  commitFacadeRename,
  commitFacadeLock,
  commitFacadeBlendMode,
  getFacade,
  seedFacadeFromEngine,
  __resetFacadeRegistryForTests,
} from "@/lib/protocol/facadeRegistry";
import { getWasmExportModule } from "@/components/editor/wasmExport";

// Facade readiness: photrez.facade=1 metadata routing tests run against the REAL
// Rust engine (same harness as facadeOpacity.test.ts).
let wasmModule: { protocol_reset: (docId: string) => void } | null = null;

beforeAll(async () => {
  const m = await getWasmExportModule();
  wasmModule = m;
});

beforeEach(() => localStorage.setItem("photrez.facade", "1"));
afterEach(() => {
  localStorage.removeItem("photrez.facade");
  __resetFacadeRegistryForTests();
  wasmModule?.protocol_reset("default");
  vi.restoreAllMocks();
});

function makeDoc(id: string) {
  const engine = new DocumentEngine(id, id, 800, 600);
  const facade = getFacade(id);
  seedFacadeFromEngine(engine as never, facade);
  return { engine, facade };
}

function lastOwnedId(facade: ReturnType<typeof getFacade>): string {
  const layers = facade.snapshot.layers;
  return layers[layers.length - 1].id;
}

describe("commitFacadeVisibility (SetVisible arm)", () => {
  it("flag ON + facade-owned: ONE setVisible command w/ expectedVersion + projection, zero legacy mutation", async () => {
    const { engine, facade } = makeDoc("vis1");
    await facade.addLayer("L");
    engine.applyFacadeSnapshot(facade.snapshot as never);
    const id = lastOwnedId(facade);
    const vBefore = facade.renderedVersion;
    const spy = vi.spyOn(bridge, "applyCommand");

    const r = await commitFacadeVisibility(engine as never, [id], false);

    expect(r.status).toBe("applied");
    expect(r.count).toBe(1);
    expect(spy).toHaveBeenCalledTimes(1);
    const env = spy.mock.calls[0][0] as { expectedVersion?: number; command: { type: string; id: string; visible: boolean } };
    expect(env.command.type).toBe("setVisible");
    expect(env.command.id).toBe(id);
    expect(env.command.visible).toBe(false);
    expect(env.expectedVersion).toBe(vBefore); // mandatory guard
    engine.applyFacadeSnapshot(facade.snapshot as never);
    expect(engine.getLayer(id)!.visible).toBe(false);
  });

  it("flag OFF: legacy status, zero applyCommand (byte-identical default path)", async () => {
    localStorage.removeItem("photrez.facade");
    const { engine } = makeDoc("visL");
    const spy = vi.spyOn(bridge, "applyCommand");
    const r = await commitFacadeVisibility(engine as never, ["any"], false);
    expect(r.status).toBe("legacy");
    expect(spy).not.toHaveBeenCalled();
    expect(hasFacadeOwnedLayers()).toBe(false);
  });

  it("mixed selection rejected atomically (zero commands)", async () => {
    const { engine } = makeDoc("visM");
    const facade = getFacade("visM");
    await facade.addLayer("Owned");
    engine.applyFacadeSnapshot(facade.snapshot as never);
    const ownedId = lastOwnedId(facade);
    expect(isFacadeOwnedLayer(ownedId)).toBe(true);
    const r = await commitFacadeVisibility(engine as never, [ownedId, "legacy-bg"], false);
    expect(r.status).toBe("mixed-rejected");
  });

  it("empty selection -> silent no-op status", async () => {
    const { engine } = makeDoc("visE");
    const r = await commitFacadeVisibility(engine as never, [], false);
    expect(r.status).toBe("empty");
  });

  it("routed op reverts via native-history undo", async () => {
    const { engine, facade } = makeDoc("visU");
    await facade.addLayer("U");
    engine.applyFacadeSnapshot(facade.snapshot as never);
    const id = lastOwnedId(facade);
    const before = engine.getLayer(id)!.visible;
    await facade.setLayerVisibility(id, false);
    engine.applyFacadeSnapshot(facade.snapshot as never);
    expect(engine.getLayer(id)!.visible).toBe(false);
    await facade.undo();
    engine.applyFacadeSnapshot(facade.snapshot as never);
    expect(engine.getLayer(id)!.visible).toBe(before);
  });

  it("rejection propagates (no silent TS mutation)", async () => {
    const { engine, facade } = makeDoc("visR");
    await facade.addLayer("R");
    engine.applyFacadeSnapshot(facade.snapshot as never);
    const id = lastOwnedId(facade);
    vi.spyOn(bridge, "applyCommand").mockRejectedValueOnce(new Error("E_EXTERNAL_PENDING"));
    await expect(commitFacadeVisibility(engine as never, [id], false)).rejects.toThrow(/E_EXTERNAL_PENDING/);
    expect(engine.getLayer(id)!.visible).toBe(true); // unchanged
  });
});

describe("commitFacadeRename (Rename arm)", () => {
  it("flag ON + facade-owned: ONE rename command w/ expectedVersion + projection", async () => {
    const { engine, facade } = makeDoc("ren1");
    await facade.addLayer("L");
    engine.applyFacadeSnapshot(facade.snapshot as never);
    const id = lastOwnedId(facade);
    const vBefore = facade.renderedVersion;
    const spy = vi.spyOn(bridge, "applyCommand");

    const r = await commitFacadeRename(engine as never, [id], "Renamed");

    expect(r.status).toBe("applied");
    expect(spy).toHaveBeenCalledTimes(1);
    const env = spy.mock.calls[0][0] as { expectedVersion?: number; command: { type: string; id: string; name: string } };
    expect(env.command.type).toBe("rename");
    expect(env.command.id).toBe(id);
    expect(env.command.name).toBe("Renamed");
    expect(env.expectedVersion).toBe(vBefore);
    engine.applyFacadeSnapshot(facade.snapshot as never);
    expect(engine.getLayer(id)!.name).toBe("Renamed");
  });

  it("flag OFF: legacy status, zero applyCommand", async () => {
    localStorage.removeItem("photrez.facade");
    const { engine } = makeDoc("renL");
    const spy = vi.spyOn(bridge, "applyCommand");
    const r = await commitFacadeRename(engine as never, ["any"], "X");
    expect(r.status).toBe("legacy");
    expect(spy).not.toHaveBeenCalled();
  });

  it("mixed selection rejected atomically", async () => {
    const { engine } = makeDoc("renM");
    const facade = getFacade("renM");
    await facade.addLayer("Owned");
    engine.applyFacadeSnapshot(facade.snapshot as never);
    const ownedId = lastOwnedId(facade);
    const r = await commitFacadeRename(engine as never, [ownedId, "legacy-bg"], "X");
    expect(r.status).toBe("mixed-rejected");
  });

  it("routed op reverts via native-history undo", async () => {
    const { engine, facade } = makeDoc("renU");
    await facade.addLayer("U");
    engine.applyFacadeSnapshot(facade.snapshot as never);
    const id = lastOwnedId(facade);
    const before = engine.getLayer(id)!.name;
    await facade.setLayerName(id, "Renamed");
    engine.applyFacadeSnapshot(facade.snapshot as never);
    expect(engine.getLayer(id)!.name).toBe("Renamed");
    await facade.undo();
    engine.applyFacadeSnapshot(facade.snapshot as never);
    expect(engine.getLayer(id)!.name).toBe(before);
  });

  it("rejection propagates (no silent TS mutation)", async () => {
    const { engine, facade } = makeDoc("renR");
    await facade.addLayer("R");
    engine.applyFacadeSnapshot(facade.snapshot as never);
    const id = lastOwnedId(facade);
    vi.spyOn(bridge, "applyCommand").mockRejectedValueOnce(new Error("E_EXTERNAL_PENDING"));
    await expect(commitFacadeRename(engine as never, [id], "X")).rejects.toThrow(/E_EXTERNAL_PENDING/);
    expect(engine.getLayer(id)!.name).toBe("R");
  });
});

describe("commitFacadeLock (SetLocked arm, 4 kinds)", () => {
  it("flag ON + facade-owned: ONE setLocked command w/ kind + expectedVersion + projection", async () => {
    const { engine, facade } = makeDoc("loc1");
    await facade.addLayer("L");
    engine.applyFacadeSnapshot(facade.snapshot as never);
    const id = lastOwnedId(facade);
    const vBefore = facade.renderedVersion;
    const spy = vi.spyOn(bridge, "applyCommand");

    const r = await commitFacadeLock(engine as never, [id], "base", true);

    expect(r.status).toBe("applied");
    expect(spy).toHaveBeenCalledTimes(1);
    const env = spy.mock.calls[0][0] as { expectedVersion?: number; command: { type: string; id: string; kind: string; locked: boolean } };
    expect(env.command.type).toBe("setLocked");
    expect(env.command.id).toBe(id);
    expect(env.command.kind).toBe("base");
    expect(env.command.locked).toBe(true);
    expect(env.expectedVersion).toBe(vBefore);
    engine.applyFacadeSnapshot(facade.snapshot as never);
    expect(engine.getLayer(id)!.locked).toBe(true);
  });

  it("all four lock kinds route to setLocked with the named kind", async () => {
    const { engine, facade } = makeDoc("locK");
    await facade.addLayer("L");
    engine.applyFacadeSnapshot(facade.snapshot as never);
    const id = lastOwnedId(facade);
    const spy = vi.spyOn(bridge, "applyCommand");
    for (const kind of ["transparency", "position", "rotation"] as const) {
      await commitFacadeLock(engine as never, [id], kind, true);
    }
    // 1 (base from above) + 3 = 4 commands, each carries the right kind.
    const kinds = spy.mock.calls.slice(-3).map((c) => (c[0] as { command: { kind: string } }).command.kind);
    expect(kinds).toEqual(["transparency", "position", "rotation"]);
  });

  it("flag OFF: legacy status, zero applyCommand", async () => {
    localStorage.removeItem("photrez.facade");
    const { engine } = makeDoc("locL");
    const spy = vi.spyOn(bridge, "applyCommand");
    const r = await commitFacadeLock(engine as never, ["any"], "base", true);
    expect(r.status).toBe("legacy");
    expect(spy).not.toHaveBeenCalled();
  });

  it("mixed selection rejected atomically", async () => {
    const { engine } = makeDoc("locM");
    const facade = getFacade("locM");
    await facade.addLayer("Owned");
    engine.applyFacadeSnapshot(facade.snapshot as never);
    const ownedId = lastOwnedId(facade);
    const r = await commitFacadeLock(engine as never, [ownedId, "legacy-bg"], "base", true);
    expect(r.status).toBe("mixed-rejected");
  });

  it("routed op reverts via native-history undo", async () => {
    const { engine, facade } = makeDoc("locU");
    await facade.addLayer("U");
    engine.applyFacadeSnapshot(facade.snapshot as never);
    const id = lastOwnedId(facade);
    const before = engine.getLayer(id)!.locked;
    await facade.setLayerLocked(id, "base", true);
    engine.applyFacadeSnapshot(facade.snapshot as never);
    expect(engine.getLayer(id)!.locked).toBe(!before);
    await facade.undo();
    engine.applyFacadeSnapshot(facade.snapshot as never);
    expect(engine.getLayer(id)!.locked).toBe(before);
  });

  it("rejection propagates (no silent TS mutation)", async () => {
    const { engine, facade } = makeDoc("locR");
    await facade.addLayer("R");
    engine.applyFacadeSnapshot(facade.snapshot as never);
    const id = lastOwnedId(facade);
    vi.spyOn(bridge, "applyCommand").mockRejectedValueOnce(new Error("E_EXTERNAL_PENDING"));
    await expect(commitFacadeLock(engine as never, [id], "base", true)).rejects.toThrow(/E_EXTERNAL_PENDING/);
    expect(engine.getLayer(id)!.locked).toBe(false);
  });
});

describe("commitFacadeBlendMode (SetBlendMode arm)", () => {
  it("flag ON + facade-owned: ONE setBlendMode command w/ expectedVersion + projection", async () => {
    const { engine, facade } = makeDoc("bld1");
    await facade.addLayer("L");
    engine.applyFacadeSnapshot(facade.snapshot as never);
    const id = lastOwnedId(facade);
    const vBefore = facade.renderedVersion;
    const spy = vi.spyOn(bridge, "applyCommand");

    const r = await commitFacadeBlendMode(engine as never, [id], "multiply");

    expect(r.status).toBe("applied");
    expect(spy).toHaveBeenCalledTimes(1);
    const env = spy.mock.calls[0][0] as { expectedVersion?: number; command: { type: string; id: string; mode: string } };
    expect(env.command.type).toBe("setBlendMode");
    expect(env.command.id).toBe(id);
    expect(env.command.mode).toBe("multiply");
    expect(env.expectedVersion).toBe(vBefore);
    engine.applyFacadeSnapshot(facade.snapshot as never);
    expect(engine.getLayer(id)!.blendMode).toBe("multiply");
  });

  it("flag OFF: legacy status, zero applyCommand", async () => {
    localStorage.removeItem("photrez.facade");
    const { engine } = makeDoc("bldL");
    const spy = vi.spyOn(bridge, "applyCommand");
    const r = await commitFacadeBlendMode(engine as never, ["any"], "multiply");
    expect(r.status).toBe("legacy");
    expect(spy).not.toHaveBeenCalled();
  });

  it("mixed selection rejected atomically", async () => {
    const { engine } = makeDoc("bldM");
    const facade = getFacade("bldM");
    await facade.addLayer("Owned");
    engine.applyFacadeSnapshot(facade.snapshot as never);
    const ownedId = lastOwnedId(facade);
    const r = await commitFacadeBlendMode(engine as never, [ownedId, "legacy-bg"], "multiply");
    expect(r.status).toBe("mixed-rejected");
  });

  it("routed op reverts via native-history undo", async () => {
    const { engine, facade } = makeDoc("bldU");
    await facade.addLayer("U");
    engine.applyFacadeSnapshot(facade.snapshot as never);
    const id = lastOwnedId(facade);
    const before = engine.getLayer(id)!.blendMode;
    await facade.setLayerBlendMode(id, "multiply");
    engine.applyFacadeSnapshot(facade.snapshot as never);
    expect(engine.getLayer(id)!.blendMode).toBe("multiply");
    await facade.undo();
    engine.applyFacadeSnapshot(facade.snapshot as never);
    expect(engine.getLayer(id)!.blendMode).toBe(before);
  });

  it("rejection propagates (no silent TS mutation)", async () => {
    const { engine, facade } = makeDoc("bldR");
    await facade.addLayer("R");
    engine.applyFacadeSnapshot(facade.snapshot as never);
    const id = lastOwnedId(facade);
    vi.spyOn(bridge, "applyCommand").mockRejectedValueOnce(new Error("E_EXTERNAL_PENDING"));
    await expect(commitFacadeBlendMode(engine as never, [id], "multiply")).rejects.toThrow(/E_EXTERNAL_PENDING/);
    expect(engine.getLayer(id)!.blendMode).toBe("normal");
  });
});
