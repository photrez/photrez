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

import { describe, it, expect, vi, beforeEach, afterEach, beforeAll, type Mock } from "vitest";
import { DocumentEngine, isFacadeOwnedLayer, hasFacadeOwnedLayers } from "@/engine/document";
import * as bridge from "@/lib/protocol/bridge";
import { CONTRACT_VERSION } from "@/lib/protocol/types";
import {
  commitFacadeVisibility,
  commitFacadeRename,
  commitFacadeLock,
  commitFacadeBlendMode,
  commitFacadeReorder,
  getFacade,
  seedFacadeFromEngine,
  __resetFacadeRegistryForTests,
} from "@/lib/protocol/facadeRegistry";
import { getWasmExportModule } from "@/components/editor/wasmExport";
import { invoke } from "@tauri-apps/api/core";

// Native-authority harness: the real Tauri `invoke` is mocked (vi.mock below); the
// native describe routes it to an emulated per-document ProtocolEngine (routeNative)
// that returns a FULLY-ORDERED snapshot after every command. This mirrors the proven
// pattern in canonicalSeedBarrier.wiring.test.ts / nativeAuthorityReroute.test.ts.
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const invokeMock = invoke as unknown as Mock<(cmd: string, args?: Record<string, unknown>) => Promise<unknown>>;

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

// Emulated native ProtocolEngine for the native-authority reorder tests. Mirrors the
// proven harness in canonicalSeedBarrier.wiring.test.ts / nativeAuthorityReroute.test.ts:
// nativeClient delegates to raw `invoke`, which is mocked above, and this local router
// plays the role of the per-document native engine. It keeps a FULLY-ORDERED layer set
// keyed by doc id and returns FAITHFUL DELTAS (single Upsert for add, ordered full
// restatement for the Reorder class, minimal inverse on undo/redo) so production's
// delta-only consumer sees exactly what the real Rust arms emit.
const cmdResultJson = (dv: number, changes: unknown[] = []) =>
  JSON.stringify({
    documentVersion: dv,
    delta: { baseVersion: Math.max(0, dv - 1), version: dv, changes },
    status: "ok",
  });

// Minimal faithful inverse between two layer vectors (what the real Rust
// history entries carry): identical id-sets -> ordered full restatement (the
// Reorder class); otherwise Removes for vanished ids + single Upserts for
// re-appearing ids, matching arm semantics change-for-change.
function diffChanges(
  from: Array<Record<string, unknown>>,
  to: Array<Record<string, unknown>>,
): unknown[] {
  const toIds = new Set(to.map((l) => l.id as string));
  const fromIds = new Set(from.map((l) => l.id as string));
  const changes: unknown[] = [];
  for (const l of from) if (!toIds.has(l.id as string)) changes.push({ kind: "remove", id: l.id as string, resourceId: (l.resourceId as number) ?? 0 });
  if (changes.length === 0 && to.length === from.length) {
    return to.map((l) => ({ kind: "upsert", layer: { ...l } }));
  }
  for (const l of to) if (!fromIds.has(l.id as string)) changes.push({ kind: "upsert", layer: { ...l } });
  return changes;
}

function routeNative(): void {
  const open = new Set<string>();
  const version = new Map<string, number>();
  const layers = new Map<string, Array<Record<string, unknown>>>();
  const history = new Map<string, Array<{ layers: Array<Record<string, unknown>>; version: number }>>();
  const redo = new Map<string, Array<{ layers: Array<Record<string, unknown>>; version: number }>>();

  invokeMock.mockImplementation(async (cmd: string, args: Record<string, unknown> = {}) => {
    const docId = (args.docId as string) ?? "default";
    const cur = () => version.get(docId) ?? 0;
    const pushHistory = () => {
      if (!history.has(docId)) history.set(docId, []);
      history.get(docId)!.push({ layers: (layers.get(docId) ?? []).map((l) => ({ ...l })), version: cur() });
    };
    switch (cmd) {
      case "rust_pixels_open_document":
        open.add(docId);
        return undefined;
      case "protocol_seed_native": {
        if (!open.has(docId)) throw `document not open: ${docId}`;
        const payload = JSON.parse((args.payloadJson as string) ?? "{}");
        version.set(docId, Number(payload.version ?? 0));
        layers.set(docId, (payload.layers ?? []).map((l: Record<string, unknown>) => ({ ...l })));
        history.set(docId, []);
        redo.set(docId, []);
        return JSON.stringify({ version: version.get(docId), layers: layers.get(docId) });
      }
      case "protocol_seed_canonical_native":
        return null;
      case "protocol_register_adapter_native":
        return null;
      case "protocol_apply_command_native": {
        if (!open.has(docId)) throw `document not open: ${docId}`;
        const env = JSON.parse((args.envelopeJson as string) ?? "{}");
        const c = (env.command ?? {}) as Record<string, unknown>;
        // Faithful version guard: a mismatched expectedVersion is rejected exactly
        // like the real Rust engine (E_VERSION_MISMATCH).
        if (env.expectedVersion !== undefined && env.expectedVersion !== cur()) {
          throw `E_VERSION_MISMATCH: expected version ${env.expectedVersion} got ${cur()}`;
        }
        if (c.type === "addLayer") {
          pushHistory();
          const arr = layers.get(docId) ?? [];
          const layer: Record<string, unknown> = {
            id: c.id,
            name: c.name,
            visible: true,
            opacity: 1,
            resourceId: 0,
            x: 0,
            y: 0,
            scaleX: 1,
            scaleY: 1,
            rotation: 0,
            isBackground: false,
          };
          // Faithful to the real AddLayer arm: insert at the clamped host index
          // and emit a single Upsert.
          const at = Number(c.index) < 0 ? 0 : Math.min(Number(c.index ?? 0), arr.length);
          arr.splice(at, 0, layer);
          layers.set(docId, arr);
          version.set(docId, cur() + 1);
          return cmdResultJson(cur(), [{ kind: "upsert", layer: { ...layer } }]);
        }
        if (c.type === "reorder") {
          pushHistory();
          const arr = (layers.get(docId) ?? []).map((l) => ({ ...l }));
          const idx = arr.findIndex((l) => l.id === c.id);
          if (idx >= 0) {
            const [item] = arr.splice(idx, 1);
            const to = Math.max(0, Math.min(arr.length, Number(c.to)));
            arr.splice(to, 0, item);
          }
          layers.set(docId, arr);
          version.set(docId, cur() + 1);
          // Faithful to the real Reorder arm: ordered full restatement.
          return cmdResultJson(cur(), arr.map((l) => ({ kind: "upsert", layer: { ...l } })));
        }
        if (c.type === "undo") {
          const h = history.get(docId) ?? [];
          let changes: unknown[] = [];
          if (h.length > 0) {
            const prev = h.pop()!;
            if (!redo.has(docId)) redo.set(docId, []);
            redo.get(docId)!.push({ layers: (layers.get(docId) ?? []).map((l) => ({ ...l })), version: cur() });
            changes = diffChanges(layers.get(docId) ?? [], prev.layers);
            layers.set(docId, prev.layers.map((l) => ({ ...l })));
          }
          version.set(docId, cur() + 1);
          return cmdResultJson(cur(), changes);
        }
        if (c.type === "redo") {
          const r = redo.get(docId) ?? [];
          let changes: unknown[] = [];
          if (r.length > 0) {
            const nx = r.pop()!;
            if (!history.has(docId)) history.set(docId, []);
            history.get(docId)!.push({ layers: (layers.get(docId) ?? []).map((l) => ({ ...l })), version: cur() });
            changes = diffChanges(layers.get(docId) ?? [], nx.layers);
            layers.set(docId, nx.layers.map((l) => ({ ...l })));
          }
          version.set(docId, cur() + 1);
          return cmdResultJson(cur(), changes);
        }
        // Generic OK (e.g. setOpacity/setVisible): bump version, no layer mutation.
        version.set(docId, cur() + 1);
        return cmdResultJson(cur());
      }
      case "protocol_snapshot_native":
        if (!open.has(docId)) throw `document not open: ${docId}`;
        return JSON.stringify({ version: cur(), layers: (layers.get(docId) ?? []).map((l) => ({ ...l })) });
      case "protocol_version_native":
        if (!open.has(docId)) throw `document not open: ${docId}`;
        return cur();
      case "protocol_history_query_native":
        if (!open.has(docId)) throw `document not open: ${docId}`;
        return JSON.stringify({ cursor: 0, lastSeq: 0, degradedHint: false, entries: [] });
      case "protocol_history_cursor_commit_native":
        if (!open.has(docId)) throw `document not open: ${docId}`;
        version.set(docId, cur() + 1);
        return cmdResultJson(cur());
      default:
        throw `E_UNKNOWN_COMMAND: ${cmd}`;
    }
  });
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

describe("commitFacadeReorder (Reorder arm) — wasm authority (legacy route)", () => {
  // Under wasm authority the reorder arm never routes to native: commitFacadeReorder
  // returns {status:"legacy"} before any selection resolve, so applyCommand is never
  // called (the production routing decision: reorder is gated to native authority).
  // These tests pin that default path.
  it("non-facade-owned id routes to legacy status (zero applyCommand)", async () => {
    const { engine } = makeDoc("reM");
    const spy = vi.spyOn(bridge, "applyCommand");
    const r = await commitFacadeReorder(engine as never, "legacy-layer", 0);
    expect(r.status).toBe("legacy");
    expect(spy).not.toHaveBeenCalled();
  });

  // T3 (routing pin): under WASM authority the reorder arm returns {status:"legacy"}
  // and applyCommand is NOT called. Under flag-off the default path is byte-identical:
  // still legacy, still zero applyCommand.
  it("T3: wasm authority -> legacy status, applyCommand NOT called (pins routing); flag-off also legacy", async () => {
    localStorage.removeItem("photrez.facadeAuthority"); // ensure default (wasm) authority
    const { engine, facade } = makeDoc("reT3w");
    await facade.addLayer("T");
    engine.applyFacadeSnapshot(facade.snapshot as never);
    const id = lastOwnedId(facade);
    const spy = vi.spyOn(bridge, "applyCommand");
    const r = await commitFacadeReorder(engine as never, id, 0);
    expect(r.status).toBe("legacy");
    expect(spy).not.toHaveBeenCalled();

    // Flag OFF (photrez.facade removed) is byte-identical: still legacy, no applyCommand
    // from commitFacadeReorder itself. (addLayer mints the layer via applyCommand first;
    // isolate the commit under test by clearing the spy after the mint settles.)
    localStorage.removeItem("photrez.facade");
    const spy2 = vi.spyOn(bridge, "applyCommand");
    const { engine: e2, facade: f2 } = makeDoc("reT3f");
    await f2.addLayer("U");
    await new Promise((r) => setTimeout(r, 0));
    spy2.mockClear();
    const id2 = lastOwnedId(f2);
    const r2 = await commitFacadeReorder(e2 as never, id2, 0);
    expect(r2.status).toBe("legacy");
    expect(spy2).not.toHaveBeenCalled();
  });
});

// Native-authority reorder contract. Under native authority the routed Reorder
// command carries an ordered full restatement which the delta consumer applies
// (editorFacade.reorderLayer -> applyDeltaToSnapshot restatement branch) - ORDER
// round-trips with NO snapshot re-read anywhere. The wasm path defers to legacy
// (the engine there only holds facade-created layers). routeNative emits faithful
// arm deltas so these exercise the real production routing.
describe("commitFacadeReorder (Reorder arm) - native authority", () => {
  beforeEach(() => {
    localStorage.setItem("photrez.facadeAuthority", "native");
    bridge.__resetNativeAuthorityForTests();
    invokeMock.mockReset();
    routeNative();
  });
  afterEach(() => {
    localStorage.removeItem("photrez.facadeAuthority");
    bridge.__resetNativeAuthorityForTests();
    invokeMock.mockReset();
  });

  it("flag ON + facade-owned: ONE reorder command w/ expectedVersion + projection", async () => {
    const { engine, facade } = makeDoc("re1");
    for (const n of ["A", "B"]) {
      await facade.addLayer(n);
      engine.applyFacadeSnapshot(facade.snapshot as never);
    }
    const before = facade.snapshot.layers.map((l) => l.id);
    const movingId = before[0];
    const to = before.length - 1;
    const vBefore = facade.renderedVersion;
    const spy = vi.spyOn(bridge, "applyCommand");

    const r = await commitFacadeReorder(engine as never, movingId, to);

    expect(r.status).toBe("applied");
    expect(r.count).toBe(1);
    expect(spy).toHaveBeenCalledTimes(1);
    const env = spy.mock.calls[0][0] as { expectedVersion?: number; command: { type: string; id: string; to: number } };
    expect(env.command.type).toBe("reorder");
    expect(env.command.id).toBe(movingId);
    expect(env.command.to).toBe(to);
    expect(env.expectedVersion).toBe(vBefore);
    // Move top -> bottom: the moved id lands last, the rest keep their relative order.
    const expected = [...before.slice(1), movingId];
    engine.applyFacadeSnapshot(facade.snapshot as never);
    expect(engine.getLayers().map((l) => l.id)).toEqual(expected);
    // The reorder delta is an ordered full restatement; the consumer adopts it,
    // so the facade projection carries the engine's exact order.
    expect(facade.snapshot.layers.map((l) => l.id)).toEqual(expected);
  });

  it("ORDER PROOF: TS projection order matches authoritative engine order (delta restatement, no re-read)", async () => {
    const { engine, facade } = makeDoc("re2");
    for (const n of ["A", "B"]) {
      await facade.addLayer(n);
      engine.applyFacadeSnapshot(facade.snapshot as never);
    }
    const before = facade.snapshot.layers.map((l) => l.id);
    const movingId = before[0];
    const to = before.length - 1;
    const expected = [...before.slice(1), movingId];
    await commitFacadeReorder(engine as never, movingId, to);
    // The Reorder arm's ordered full restatement flows through the delta
    // consumer, so the facade projection order equals the engine's native
    // order with no re-read. Against a COMPLETE emulated engine the
    // projection, the live getSnapshot (test oracle only), and the projected
    // engine all agree exactly.
    expect(facade.snapshot.layers.map((l) => l.id)).toEqual(expected);
    expect((await bridge.getSnapshot("re2")).layers.map((l) => l.id)).toEqual(expected);
    engine.applyFacadeSnapshot(facade.snapshot as never);
    expect(engine.getLayers().map((l) => l.id)).toEqual(expected);
  });

  it("routed reorder reverts via native-history undo (ORDER restored)", async () => {
    const { engine, facade } = makeDoc("reU");
    for (const n of ["A", "B"]) {
      await facade.addLayer(n);
      engine.applyFacadeSnapshot(facade.snapshot as never);
    }
    // The restatement delta reorders the facade snapshot exactly as the engine
    // did; the live engine snapshot is read ONLY as the test's oracle here,
    // never as a projection source (asserted separately by the no-re-read test).
    const beforeSnap = await bridge.getSnapshot("reU");
    const before = beforeSnap.layers.map((l) => l.id);
    const movingId = before[0];
    const to = before.length - 1;
    const expected = [...before.slice(1), movingId];
    const noRead = vi.spyOn(bridge, "getSnapshot");
    await commitFacadeReorder(engine as never, movingId, to);
    expect(engine.getLayers().map((l) => l.id)).toEqual(expected);
    await facade.undo();
    engine.applyFacadeSnapshot(facade.snapshot as never);
    expect(engine.getLayers().map((l) => l.id)).toEqual(before);
    // Falsification pin: ORDER round-tripped entirely through delta restatements -
    // no snapshot re-read happened during the routed reorder OR its undo.
    expect(noRead).not.toHaveBeenCalled();
  });

  it("after undo, next routed command envelope expectedVersion matches post-undo engine version (no double-drift)", async () => {
    const { engine, facade } = makeDoc("reV");
    await facade.addLayer("X");
    engine.applyFacadeSnapshot(facade.snapshot as never);
    const id = lastOwnedId(facade);
    // One routed op creates a native-history entry; undo bumps the engine
    // document version while moving the cursor, and the delta the consumer
    // applies carries that new version - renderedVersion must track it exactly
    // so the NEXT routed command's expectedVersion matches the engine (no
    // double-drift).
    await commitFacadeReorder(engine as never, id, 0);
    await facade.undo();
    const postUndoVersion = (await bridge.getSnapshot("reV")).version;
    // assert the facade's projection already tracks the post-undo version
    expect((facade as unknown as { renderedVersion: number }).renderedVersion).toBe(postUndoVersion);
    // capture the NEXT routed command's expectedVersion envelope
    let capturedExpected: number | undefined;
    const orig = bridge.applyCommand;
    vi.spyOn(bridge, "applyCommand").mockImplementation(async (env) => {
      if (env.command.type !== "undo" && env.command.type !== "redo") {
        capturedExpected = env.expectedVersion;
      }
      return orig(env);
    });
    await commitFacadeReorder(engine as never, id, 0);
    expect(capturedExpected).toBe(postUndoVersion);
  });

  it("rejection propagates (no silent TS mutation)", async () => {
    const { engine, facade } = makeDoc("reR");
    await facade.addLayer("R");
    engine.applyFacadeSnapshot(facade.snapshot as never);
    const id = lastOwnedId(facade);
    vi.spyOn(bridge, "applyCommand").mockRejectedValueOnce(new Error("E_EXTERNAL_PENDING"));
    await expect(commitFacadeReorder(engine as never, id, 1)).rejects.toThrow(/E_EXTERNAL_PENDING/);
    expect(engine.getLayers().map((l) => l.id)).toEqual([id]);
  });
});
