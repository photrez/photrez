// Routing of the five structural ops (duplicate / mergeDown / mergeSelected /
// flatten / rasterize) through the native command arms. Mirrors
// facadeMetadataOps.test.ts but exercises the STRUCTURAL arms under native
// authority (the structural funnels gate on native authority, since under wasm
// authority the engine only knows facade-created layers and a merge/flatten
// would hit a PARTIAL set and drop layers — the same partial-set danger that
// gates commitFacadeReorder to native).
//
// The arms already exist in the Rust engine and are proven MEASURED-EQUAL in the
// parity matrix; this file proves the PRODUCTION dispatch (editorFacade methods
// + commitFacadeX funnels) issues the correct envelope, projects the ordered
// restatement (victims removed, merged/clone appears at the engine position with
// a NULL bitmap), and that flag-OFF is byte-identical (zero applyCommand).
//
// The pixel composite is host-side and is intentionally NOT run here (jsdom has no
// OffscreenCanvas); the undo/restore test attaches fake bitmaps via
// setLayerImageBitmap exactly as the routed caller would, then pins retention.

import { describe, it, expect, vi, beforeEach, afterEach, beforeAll, type Mock } from "vitest";
import { DocumentEngine, isFacadeOwnedLayer } from "@/engine/document";
import { MAX_LAYERS } from "@/engine/types";
import * as bridge from "@/lib/protocol/bridge";
import { CONTRACT_VERSION } from "@/lib/protocol/types";
import {
  commitFacadeDuplicate,
  commitFacadeMergeDown,
  commitFacadeMergeSelected,
  commitFacadeFlatten,
  commitFacadeRasterize,
  getFacade,
  seedFacadeFromEngine,
  __resetFacadeRegistryForTests,
} from "@/lib/protocol/facadeRegistry";
import { routeMergeDown, routeDuplicate, routeRasterize } from "@/components/editor/layers/structuralRouting";
import { invoke } from "@tauri-apps/api/core";
import { stubTextOffscreenCanvas } from "@/__tests__/test-builders";
import { DEFAULT_TEXT_DATA } from "@/engine/textTypes";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const invokeMock = invoke as unknown as Mock<(cmd: string, args?: Record<string, unknown>) => Promise<unknown>>;

// Composite helpers are host-side pixel math; in jsdom they cannot run, so the
// routing tests drive routeMergeDown by injecting the composite result directly.
const { compositeTwoLayersMock, compositeAllLayersMock } = vi.hoisted(() => ({
  compositeTwoLayersMock: vi.fn(),
  compositeAllLayersMock: vi.fn(),
}));
vi.mock("@/engine/layerComposite", () => ({
  compositeTwoLayers: compositeTwoLayersMock,
  compositeAllLayers: compositeAllLayersMock,
}));

let wasmModule: { protocol_reset: (docId: string) => void } | null = null;

beforeAll(async () => {
  try {
    const m = await import("@/components/editor/wasmExport");
    wasmModule = (m as unknown as { getWasmExportModule: () => Promise<{ protocol_reset: (d: string) => void }> }).getWasmExportModule
      ? await (m as unknown as { getWasmExportModule: () => Promise<{ protocol_reset: (d: string) => void }> }).getWasmExportModule()
      : null;
  } catch {
    wasmModule = null;
  }
});

beforeEach(() => {
  localStorage.setItem("photrez.facade", "1");
  localStorage.setItem("photrez.facadeAuthority", "native");
  bridge.__resetNativeAuthorityForTests();
  invokeMock.mockReset();
  routeNative();
});
afterEach(() => {
  vi.restoreAllMocks();
  localStorage.removeItem("photrez.facade");
  localStorage.removeItem("photrez.facadeAuthority");
  bridge.__resetNativeAuthorityForTests();
  invokeMock.mockReset();
  __resetFacadeRegistryForTests();
  wasmModule?.protocol_reset("default");
  wasmModule?.protocol_reset("struct1");
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

const cmdResultJson = (dv: number, changes: unknown[] = []) =>
  JSON.stringify({
    documentVersion: dv,
    delta: { baseVersion: Math.max(0, dv - 1), version: dv, changes },
    status: "ok",
  });

function diffChanges(
  from: Array<Record<string, unknown>>,
  to: Array<Record<string, unknown>>,
): unknown[] {
  const toIds = new Set(to.map((l) => l.id as string));
  const fromIds = new Set(from.map((l) => l.id as string));
  const changes: unknown[] = [];
  for (const l of from) if (!toIds.has(l.id as string)) changes.push({ kind: "remove", id: l.id as string, resourceId: (l.resourceId as number) ?? 0 });
  if (changes.length === 0 && to.length === from.length) {
    const moved = to.some((l, i) => l.id !== from[i]?.id);
    return moved ? to.map((l) => ({ kind: "upsert", layer: { ...l } })) : [];
  }
  for (const l of to) if (!fromIds.has(l.id as string)) changes.push({ kind: "upsert", layer: { ...l } });
  return changes;
}

// Local copy of the emulator's numeric-suffix duplicate-name rule
// (bridge_emu.ts emuNextDuplicateName), which mirrors the Rust duplicate arm:
// "Layer 1" + "Layer 2" present -> duplicate "Layer 2" -> "Layer 3".
function nextDuplicateName(names: string[], layerName: string): string {
  const m = layerName.match(/^(.*?)\s*(\d+)$/);
  const base = m ? m[1].trimEnd() : layerName.trimEnd();
  const prefix = `${base} `;
  let maxNum = 1;
  for (const name of names) {
    if (name.startsWith(prefix)) {
      const num = parseInt(name.slice(prefix.length), 10);
      if (!isNaN(num) && num > maxNum) maxNum = num;
    }
  }
  return `${base} ${maxNum + 1}`;
}

// Faithful per-document native ProtocolEngine emulator extended for the five
// structural arms. Returns fully-ordered snapshots; on undo/redo emits the
// minimal inverse (Removes + ordered restatement) so the two-pass delta consumer
// reconstructs order without any snapshot re-read.
function routeNative(): void {
  const open = new Set<string>();
  const version = new Map<string, number>();
  const layers = new Map<string, Array<Record<string, unknown>>>();
  const history = new Map<string, Array<{ layers: Array<Record<string, unknown>>; version: number }>>();
  const redo = new Map<string, Array<{ layers: Array<Record<string, unknown>>; version: number }>>();

  const fullRestatement = (docId: string) =>
    (layers.get(docId) ?? []).map((l) => ({ kind: "upsert", layer: { ...l } }));

  invokeMock.mockImplementation(async (cmd: string, args: Record<string, unknown> = {}) => {
    const docId = (args.docId as string) ?? "default";
    const cur = () => version.get(docId) ?? 0;
    const pushHistory = () => {
      if (!history.has(docId)) history.set(docId, []);
      history.get(docId)!.push({ layers: (layers.get(docId) ?? []).map((l) => ({ ...l })), version: cur() });
    };
    const setLayers = (next: Array<Record<string, unknown>>) => {
      layers.set(docId, next);
      version.set(docId, cur() + 1);
      return cmdResultJson(cur(), fullRestatement(docId));
    };
    // Faithful arm-delta helper: callers pass the exact change list. The real
    // Rust merge/flatten arms emit Removes for the victims FIRST, then ordered
    // Upserts of every surviving layer (document_core_structural.rs); the
    // two-pass consumer needs the Removes to shed the victims.
    const setLayersResult = (
      next: Array<Record<string, unknown>>,
      changes: unknown[],
    ) => {
      layers.set(docId, next);
      version.set(docId, cur() + 1);
      return cmdResultJson(cur(), changes);
    };
    const removesOf = (victims: Array<Record<string, unknown>>) =>
      victims.map((l) => ({ kind: "remove", id: l.id as string, resourceId: (l.resourceId as number) ?? 0 }));
    const upsertsOf = (v: Array<Record<string, unknown>>) =>
      v.map((l) => ({ kind: "upsert", layer: { ...l } }));
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
        if (env.expectedVersion !== undefined && env.expectedVersion !== cur()) {
          throw `E_VERSION_MISMATCH: expected version ${env.expectedVersion} got ${cur()}`;
        }
        if (c.type === "addLayer") {
          pushHistory();
          const arr = layers.get(docId) ?? [];
          const layer: Record<string, unknown> = {
            id: c.id, name: c.name, visible: true, opacity: 1, resourceId: 0,
            x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, isBackground: false, type: "raster",
          };
          const at = Number(c.index) < 0 ? 0 : Math.min(Number(c.index ?? 0), arr.length);
          arr.splice(at, 0, layer);
          return setLayers(arr);
        }
        if (c.type === "duplicateLayer") {
          pushHistory();
          const arr = (layers.get(docId) ?? []).map((l) => ({ ...l }));
          const idx = arr.findIndex((l) => l.id === c.id);
          if (idx >= 0) {
            const src = arr[idx];
            const name = nextDuplicateName(arr.map((l) => String(l.name)), String(src.name));
            const clone = { ...src, id: c.new_id, name, type: src.type ?? "raster" };
            arr.splice(idx, 0, clone); // insert directly above the source
          }
          return setLayers(arr);
        }
        if (c.type === "mergeDown") {
          pushHistory();
          const arr = (layers.get(docId) ?? []).map((l) => ({ ...l }));
          const idx = arr.findIndex((l) => l.id === c.id);
          if (idx >= 0 && idx < arr.length - 1) {
            const top = arr[idx];
            const bottom = arr[idx + 1];
            const merged = {
              id: c.merged_id, name: `${top.name} + ${bottom.name}`, visible: true, opacity: 1,
              resourceId: 0, x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, isBackground: false, type: "raster",
            };
            arr.splice(idx, 2, merged); // merged lands at the source index
            return setLayersResult(arr, [...removesOf([top, bottom]), ...upsertsOf(arr)]);
          }
          return setLayers(arr);
        }
        if (c.type === "mergeSelected") {
          pushHistory();
          const arr = (layers.get(docId) ?? []).map((l) => ({ ...l }));
          const ids = (c.ids as string[]) ?? [];
          const present = arr.filter((l) => ids.includes(l.id as string));
          const minIdx = Math.min(...present.map((l) => arr.findIndex((x) => x.id === l.id)));
          const firstName = present[0]?.name ?? "Layer";
          const merged = {
            id: c.merged_id, name: ids.length === 2 ? `${present[0].name} + ${present[1].name}` : `${firstName} (+${present.length - 1} merged)`,
            visible: true, opacity: 1, resourceId: 0, x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0,
            isBackground: false, type: "raster",
          };
          const next = arr.filter((l) => !ids.includes(l.id as string));
          next.splice(Math.max(0, minIdx), 0, merged);
          return setLayersResult(next, [...removesOf(present), ...upsertsOf(next)]);
        }
        if (c.type === "flatten") {
          pushHistory();
          const arr = (layers.get(docId) ?? []).map((l) => ({ ...l }));
          const merged = {
            id: c.merged_id, name: "Background", visible: true, opacity: 1, resourceId: 0,
            x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, isBackground: true, type: "raster",
          };
          return setLayersResult([merged], [...removesOf(arr), ...upsertsOf([merged])]); // single background survives
        }
        if (c.type === "rasterizeLayer") {
          pushHistory();
          const arr = (layers.get(docId) ?? []).map((l) => ({ ...l }));
          const idx = arr.findIndex((l) => l.id === c.id);
          if (idx >= 0) {
            arr[idx].type = "raster";
            delete arr[idx].shapeParams;
            delete arr[idx].textData;
          }
          return setLayers(arr);
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

function fakeBitmap(label: string): ImageBitmap {
  return { width: 4, height: 4, label } as unknown as ImageBitmap;
}

describe("commitFacadeDuplicate (Duplicate arm)", () => {
  it("flag ON + facade-owned: ONE duplicateLayer command (carries newId) + projection, zero legacy mutation", async () => {
    const { engine, facade } = makeDoc("struct1");
    for (const n of ["A", "B 2", "C"]) {
      await facade.addLayer(n);
      engine.applyFacadeSnapshot(facade.snapshot as never);
    }
    const id = facade.snapshot.layers[1].id; // "B 2" (mid-stack)
    // Pre-capture the stack ids: the facade snapshot mutates in place, so the
    // expected order must be spelled from BEFORE the command.
    const beforeIds = facade.snapshot.layers.map((l) => l.id);
    const vBefore = facade.renderedVersion;
    const spy = vi.spyOn(bridge, "applyCommand");
    const newId = "layer-dup-1";

    const r = await commitFacadeDuplicate(engine as never, id, newId);

    expect(r.status).toBe("applied");
    expect(r.count).toBe(1);
    expect(spy).toHaveBeenCalledTimes(1);
    const env = spy.mock.calls[0][0] as unknown as { expectedVersion?: number; command: { type: string; id: string; newId: string } };
    expect(env.command.type).toBe("duplicateLayer");
    expect(env.command.id).toBe(id);
    expect(env.command.newId).toBe(newId); // TS envelope is camelCase; bridge maps to snake at the wire
    expect(env.expectedVersion).toBe(vBefore);
    engine.applyFacadeSnapshot(facade.snapshot as never);
    // clone inserted directly ABOVE the source (mid-stack position preserved)
    const ids = engine.getLayers().map((l) => l.id);
    expect(ids).toEqual([beforeIds[0], newId, id, beforeIds[2]]);
    // Clone name follows the numeric-suffix rule for an already-numbered source:
    // "B 2" duplicating with "B 2" in the stack -> the next free suffix is "B 3".
    expect(engine.getLayer(newId)?.name).toBe("B 3");
  });

  it("flag OFF: legacy status, zero applyCommand (byte-identical default path)", async () => {
    localStorage.removeItem("photrez.facade");
    const { engine } = makeDoc("struct1");
    const spy = vi.spyOn(bridge, "applyCommand");
    const r = await commitFacadeDuplicate(engine as never, "any", "layer-dup-2");
    expect(r.status).toBe("legacy");
    expect(spy).not.toHaveBeenCalled();
  });
});

describe("commitFacadeMergeDown (MergeDown arm)", () => {
  it("flag ON + facade-owned: ONE mergeDown command (carries mergedId) + mid-stack projection (victims removed, merged at source index)", async () => {
    const { engine, facade } = makeDoc("struct1");
    for (const n of ["A", "B", "C"]) {
      await facade.addLayer(n);
      engine.applyFacadeSnapshot(facade.snapshot as never);
    }
    const ids = facade.snapshot.layers.map((l) => l.id);
    const topId = ids[1]; // B
    const spy = vi.spyOn(bridge, "applyCommand");
    const mergedId = "layer-merge-1";

    const r = await commitFacadeMergeDown(engine as never, topId, mergedId);

    expect(r.status).toBe("applied");
    expect(r.count).toBe(1);
    expect(spy).toHaveBeenCalledTimes(1);
    const env = spy.mock.calls[0][0] as unknown as { command: { type: string; id: string; mergedId: string } };
    expect(env.command.type).toBe("mergeDown");
    expect(env.command.id).toBe(topId);
    expect(env.command.mergedId).toBe(mergedId);
    engine.applyFacadeSnapshot(facade.snapshot as never);
    const after = engine.getLayers().map((l) => l.id);
    // mergeDown collapses the top AND the layer directly below it (stack here is
    // [C, B, A], so B merges into A): both victims go, the merged node takes
    // the pair's position, C is untouched.
    expect(after).toEqual([ids[0], mergedId]);
  });

  it("flag OFF: legacy status, zero applyCommand", async () => {
    localStorage.removeItem("photrez.facade");
    const { engine } = makeDoc("struct1");
    const spy = vi.spyOn(bridge, "applyCommand");
    const r = await commitFacadeMergeDown(engine as never, "any", "layer-merge-2");
    expect(r.status).toBe("legacy");
    expect(spy).not.toHaveBeenCalled();
  });

  it("routed merge -> undo restores BOTH victims with their bitmap identities; redo restores merged node (truthfully pinned)", async () => {
    const { engine, facade } = makeDoc("struct1");
    for (const n of ["A", "B", "C"]) {
      await facade.addLayer(n);
      engine.applyFacadeSnapshot(facade.snapshot as never);
    }
    const ids = facade.snapshot.layers.map((l) => l.id);
    const topId = ids[1];
    const bottomId = ids[2];
    const aBmp = fakeBitmap("A");
    const bBmp = fakeBitmap("B");
    const cBmp = fakeBitmap("C");
    // Attach fake bitmaps to all layers (host composite would do this for the merged result).
    engine.setLayerImageBitmap(ids[0], aBmp);
    engine.setLayerImageBitmap(topId, bBmp);
    engine.setLayerImageBitmap(bottomId, cBmp);
    const mergedId = "layer-merge-undo-1";

    const r = await commitFacadeMergeDown(engine as never, topId, mergedId);
    expect(r.status).toBe("applied");
    // Simulate the host-side composite the caller attaches to the merged node.
    engine.setLayerImageBitmap(mergedId, fakeBitmap("merged"));
    engine.applyFacadeSnapshot(facade.snapshot as never);
    expect(engine.getLayer(mergedId)).toBeTruthy();

    await facade.undo();
    engine.applyFacadeSnapshot(facade.snapshot as never);
    // Both victims reappear with their original bitmap identities retained.
    expect(engine.getLayer(topId)?.imageBitmap).toBe(bBmp);
    expect(engine.getLayer(bottomId)?.imageBitmap).toBe(cBmp);
    expect(engine.getLayer(mergedId)).toBeFalsy();

    await facade.redo();
    engine.applyFacadeSnapshot(facade.snapshot as never);
    // The merged node returns; its bitmap is still carried by the dropped-node
    // retention (the composite we set before undo is re-attached on redo).
    expect(engine.getLayer(mergedId)).toBeTruthy();
    expect(engine.getLayer(mergedId)?.imageBitmap).toBeTruthy();
  });
});

describe("commitFacadeMergeSelected (MergeSelected arm)", () => {
  it("flag ON + facade-owned: ONE mergeSelected command (ids + mergedId) + projection", async () => {
    const { engine, facade } = makeDoc("struct1");
    for (const n of ["A", "B", "C", "D"]) {
      await facade.addLayer(n);
      engine.applyFacadeSnapshot(facade.snapshot as never);
    }
    const ids = facade.snapshot.layers.map((l) => l.id);
    const spy = vi.spyOn(bridge, "applyCommand");
    const mergedId = "layer-msel-1";

    const r = await commitFacadeMergeSelected(engine as never, [ids[0], ids[2]], mergedId);

    expect(r.status).toBe("applied");
    expect(spy).toHaveBeenCalledTimes(1);
    const env = spy.mock.calls[0][0] as unknown as { command: { type: string; ids: string[]; mergedId: string } };
    expect(env.command.type).toBe("mergeSelected");
    expect(env.command.ids).toEqual([ids[0], ids[2]]);
    expect(env.command.mergedId).toBe(mergedId);
    engine.applyFacadeSnapshot(facade.snapshot as never);
    // Merged node sits at the topmost selected index (A's index), the unselected
    // layers keep their relative order.
    const after = engine.getLayers().map((l) => l.id);
    expect(after).toContain(mergedId);
    expect(after).not.toContain(ids[0]);
    expect(after).not.toContain(ids[2]);
  });

  it("mixed ownership rejected ATOMICALLY with ZERO commands", async () => {
    const { engine, facade } = makeDoc("struct1");
    for (const n of ["A", "B", "C"]) {
      await facade.addLayer(n);
      engine.applyFacadeSnapshot(facade.snapshot as never);
    }
    const ownedId = lastOwnedId(facade);
    const spy = vi.spyOn(bridge, "applyCommand");
    const r = await commitFacadeMergeSelected(engine as never, [ownedId, "legacy-bg"], "layer-msel-2");
    expect(r.status).toBe("mixed-rejected");
    expect(spy).not.toHaveBeenCalled();
  });

  it("flag OFF: legacy status, zero applyCommand", async () => {
    localStorage.removeItem("photrez.facade");
    const { engine } = makeDoc("struct1");
    const spy = vi.spyOn(bridge, "applyCommand");
    const r = await commitFacadeMergeSelected(engine as never, ["a", "b"], "layer-msel-3");
    expect(r.status).toBe("legacy");
    expect(spy).not.toHaveBeenCalled();
  });
});

describe("commitFacadeFlatten (Flatten arm)", () => {
  it("flag ON: ONE flatten command (mergedId only, no id) + projection to single background", async () => {
    const { engine, facade } = makeDoc("struct1");
    for (const n of ["A", "B", "C"]) {
      await facade.addLayer(n);
      engine.applyFacadeSnapshot(facade.snapshot as never);
    }
    const spy = vi.spyOn(bridge, "applyCommand");
    const mergedId = "layer-flat-1";

    const r = await commitFacadeFlatten(engine as never, mergedId);

    expect(r.status).toBe("applied");
    expect(spy).toHaveBeenCalledTimes(1);
    const env = spy.mock.calls[0][0] as unknown as { command: { type: string; mergedId: string } };
    expect(env.command.type).toBe("flatten");
    expect(env.command.mergedId).toBe(mergedId);
    expect((env.command as unknown as { id?: string }).id).toBeUndefined(); // flatten carries no id
    engine.applyFacadeSnapshot(facade.snapshot as never);
    expect(engine.getLayers().map((l) => l.id)).toEqual([mergedId]);
  });

  it("flag OFF: legacy status, zero applyCommand", async () => {
    localStorage.removeItem("photrez.facade");
    const { engine } = makeDoc("struct1");
    const spy = vi.spyOn(bridge, "applyCommand");
    const r = await commitFacadeFlatten(engine as never, "layer-flat-2");
    expect(r.status).toBe("legacy");
    expect(spy).not.toHaveBeenCalled();
  });
});

describe("commitFacadeRasterize (Rasterize arm)", () => {
  it("flag ON + facade-owned: ONE rasterizeLayer command (id only); the funnel does NOT retype", async () => {
    const { engine, facade } = makeDoc("struct1");
    await facade.addLayer("Shape");
    engine.applyFacadeSnapshot(facade.snapshot as never);
    const id = lastOwnedId(facade);
    // Mark the layer as a shape layer so the flip is observable.
    const shape = engine.getLayer(id);
    if (shape) { (shape as unknown as { type: string }).type = "shape"; (shape as unknown as { shapeParams: unknown }).shapeParams = { kind: "rect" }; }
    const spy = vi.spyOn(bridge, "applyCommand");
    const r = await commitFacadeRasterize(engine as never, id);
    expect(r.status).toBe("applied");
    expect(spy).toHaveBeenCalledTimes(1);
    const env = spy.mock.calls[0][0] as unknown as { command: { type: string; id: string } };
    expect(env.command.type).toBe("rasterizeLayer");
    expect(env.command.id).toBe(id);
    // The funnel owns the COMMAND + graph delta only; the visible type flip to
    // "raster" is the routing helper's host-side job (the projected descriptor
    // carries no type field - graph ops never detach or retype nodes here).
    // The flip itself is pinned route-level in this file's routeRasterize test.
    expect(engine.getLayer(id)?.type).toBe("shape"); // funnel alone must NOT retype
  });

  it("flag OFF: legacy status, zero applyCommand", async () => {
    localStorage.removeItem("photrez.facade");
    const { engine } = makeDoc("struct1");
    const spy = vi.spyOn(bridge, "applyCommand");
    const r = await commitFacadeRasterize(engine as never, "any");
    expect(r.status).toBe("legacy");
    expect(spy).not.toHaveBeenCalled();
  });
});

// ── Routing layer: pre-dispatch pixel guard (P1) ─────────────────────────────
// The composite is computed BEFORE the facade command is dispatched. A null
// composite must abort the op with no graph mutation; a good composite must
// dispatch exactly one command and never commit TS history (the facade command
// owns undo/redo).
describe("structuralRouting pre-dispatch pixel guard", () => {
  const fakeBitmap = { width: 4, height: 4 } as unknown as ImageBitmap;

  beforeEach(() => {
    localStorage.setItem("photrez.facade", "1");
    localStorage.setItem("photrez.facadeAuthority", "native");
    bridge.__resetNativeAuthorityForTests();
    invokeMock.mockReset();
    routeNative();
    compositeTwoLayersMock.mockReset();
  });
  afterEach(() => {
    localStorage.removeItem("photrez.facade");
    localStorage.removeItem("photrez.facadeAuthority");
    bridge.__resetNativeAuthorityForTests();
    invokeMock.mockReset();
    __resetFacadeRegistryForTests();
    wasmModule?.protocol_reset("default");
    wasmModule?.protocol_reset("struct1");
  });

  it("routeMergeDown: null composite returns 'error' and dispatches ZERO applyCommand", async () => {
    const { engine } = makeDoc("struct1");
    const facade = getFacade("struct1");
    for (const n of ["A", "B", "C"]) {
      await facade.addLayer(n);
      engine.applyFacadeSnapshot(facade.snapshot as never);
    }
    const ids = engine.getLayers().map((l) => l.id);
    const activeId = ids[0]; // A has B directly below it
    const spy = vi.spyOn(bridge, "applyCommand");
    compositeTwoLayersMock.mockReturnValue(null);

    const res = await routeMergeDown(
      engine as never,
      { commit: vi.fn() } as never,
      { uploadImage: vi.fn() } as never,
      activeId,
    );

    expect(res).toBe("error");
    expect(spy).not.toHaveBeenCalled();
  });

  it("routeMergeDown: good composite dispatches one command and never commits history", async () => {
    const { engine } = makeDoc("struct1");
    const facade = getFacade("struct1");
    for (const n of ["A", "B", "C"]) {
      await facade.addLayer(n);
      engine.applyFacadeSnapshot(facade.snapshot as never);
    }
    const ids = engine.getLayers().map((l) => l.id);
    const activeId = ids[0];
    const historyCommit = vi.fn();
    const uploadImage = vi.fn();
    const spy = vi.spyOn(bridge, "applyCommand");
    compositeTwoLayersMock.mockReturnValue(fakeBitmap);

    const res = await routeMergeDown(
      engine as never,
      { commit: historyCommit } as never,
      { uploadImage } as never,
      activeId,
    );

    expect(res).toBe("applied");
    expect(spy).toHaveBeenCalledTimes(1);
    expect(historyCommit).not.toHaveBeenCalled();
    expect(uploadImage).toHaveBeenCalledWith(expect.any(String), fakeBitmap);
  });

  it("routeRasterize: applied route flips the node type and drops parametric payloads", async () => {
    const { engine, facade } = makeDoc("struct1");
    await facade.addLayer("Shape");
    engine.applyFacadeSnapshot(facade.snapshot as never);
    const id = lastOwnedId(facade);
    const l = engine.getLayer(id)!;
    (l as unknown as { type: string }).type = "shape";
    (l as unknown as { shapeParams: unknown }).shapeParams = { kind: "rect" };
    const spy = vi.spyOn(bridge, "applyCommand");

    const res = await routeRasterize(
      engine as never,
      { commit: vi.fn() } as never,
      { uploadImage: vi.fn() } as never,
      id,
    );

    expect(res).toBe("applied");
    expect(spy).toHaveBeenCalledTimes(1);
    expect(engine.getLayer(id)!.type).toBe("raster");
    expect((engine.getLayer(id) as unknown as { shapeParams?: unknown }).shapeParams).toBeUndefined();
  });
});

// ── Duplicate retention + authority gates (routing layer) ────────────────────
// Pins two defects: (1) under wasm authority routeDuplicate must defer to the
// caller's legacy path with zero commands, and (2) a routed clone keeps full
// fidelity (type/params/dims/pixels) because the route pre-seeds the projection
// retention with the clone node instead of letting applyFacadeSnapshot rebuild a
// metadata-only raster at the document dimensions.
describe("routeDuplicate retention + authority gates", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("wasm authority: legacy status, zero commands, no clone side effects", async () => {
    localStorage.setItem("photrez.facade", "1");
    localStorage.removeItem("photrez.facadeAuthority");
    const { engine } = makeDoc("struct1");
    const spy = vi.spyOn(bridge, "applyCommand");
    const res = await routeDuplicate(
      engine as never,
      { commit: vi.fn() } as never,
      { uploadImage: vi.fn() } as never,
      ["any-id"],
    );
    expect(res.status).toBe("legacy");
    expect(res.newIds).toEqual([]);
    expect(spy).not.toHaveBeenCalled();
  });

  it("mixed selection (owned + unowned): legacy up front with ZERO commands and ZERO clones", async () => {
    stubTextOffscreenCanvas();
    const { engine, facade } = makeDoc("struct1");
    await facade.addLayer("Owned");
    engine.applyFacadeSnapshot(facade.snapshot as never);
    const ownedId = lastOwnedId(facade);
    // Raw engine layer never projected through the facade -> not facade-owned.
    const raw = engine.addLayer("Raw", 40, 30);
    raw.imageBitmap = fakeBitmap("raw");
    const layersBefore = engine.getLayers().length;
    const seedSpy = vi.spyOn(engine, "seedRetainedNodeForProjection");
    const unseedSpy = vi.spyOn(engine, "unseedRetainedNodeForProjection");
    const spy = vi.spyOn(bridge, "applyCommand");
    const res = await routeDuplicate(
      engine as never,
      { commit: vi.fn() } as never,
      { uploadImage: vi.fn() } as never,
      [ownedId, raw.id],
    );
    expect(res.status).toBe("legacy");
    expect(res.newIds).toEqual([]);
    expect(spy).not.toHaveBeenCalled();
    // No pre-seed for the owned id -> the caller's legacy re-run cannot double-clone.
    expect(seedSpy).not.toHaveBeenCalled();
    expect(unseedSpy).not.toHaveBeenCalled();
    expect(engine.getLayers().length).toBe(layersBefore);
  });

  it("text clone keeps type/params/dims/bitmap through projection; undo+redo retain it", async () => {
    stubTextOffscreenCanvas();
    const { engine, facade } = makeDoc("struct1");
    await facade.addLayer("Text");
    engine.applyFacadeSnapshot(facade.snapshot as never);
    const srcId = lastOwnedId(facade);
    const src = engine.getLayer(srcId)!;
    // Text data that rasterizes to a 200x150 bitmap while the layer's own
    // document-space box is 100x75 (RASTER_SCALE 2 + 4px padding). This pins the
    // production reality that bitmap dims differ from doc dims: the routed attach
    // must NOT overwrite the doc dims with the bitmap dims.
    const textData = {
      ...DEFAULT_TEXT_DATA,
      content: "x",
      boxMode: "area" as const,
      boxWidth: 98,
      boxHeight: 75,
    };
    src.type = "text";
    src.textData = textData;
    src.width = 100;
    src.height = 75;
    src.imageBitmap = fakeBitmap("src-text");
    const upload = vi.fn();
    const spy = vi.spyOn(bridge, "applyCommand");

    const res = await routeDuplicate(
      engine as never,
      { commit: vi.fn() } as never,
      { uploadImage: upload } as never,
      [srcId],
    );

    expect(res.status).toBe("applied");
    expect(res.newIds).toHaveLength(1);
    const newId = res.newIds[0];
    expect(spy).toHaveBeenCalledTimes(1);
    const clone = engine.getLayer(newId)!;
    expect(clone.type).toBe("text");
    expect(clone.textData).toEqual(textData);
    expect(clone.width).toBe(100); // doc-space box, NOT the 200px bitmap width
    expect(clone.height).toBe(75);
    expect(clone.imageBitmap).toBeTruthy();
    expect((clone.imageBitmap as ImageBitmap).width).toBe(200);
    expect((clone.imageBitmap as ImageBitmap).height).toBe(150);
    expect(clone.imageBitmap).toBe(upload.mock.calls[0][1]);

    // Undo through the real walker delta removes the clone, source stays intact.
    await facade.undo();
    engine.applyFacadeSnapshot(facade.snapshot as never);
    expect(engine.getLayer(newId)).toBeFalsy();
    expect(engine.getLayer(srcId)?.type).toBe("text");
    expect(engine.getLayer(srcId)?.textData).toEqual(textData);

    // Redo re-appears the clone; retention restores its full node again.
    await facade.redo();
    engine.applyFacadeSnapshot(facade.snapshot as never);
    const restored = engine.getLayer(newId);
    expect(restored?.type).toBe("text");
    expect(restored?.width).toBe(100);
    expect((restored?.imageBitmap as ImageBitmap).width).toBe(200);
    expect(restored?.imageBitmap).toBeTruthy();
  });
});

