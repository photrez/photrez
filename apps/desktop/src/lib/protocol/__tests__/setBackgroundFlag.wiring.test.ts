// Wiring proof for the guarded background-flag protocol commit funnel
// (commitFacadeBackgroundFlag) and its two document-factory call sites in
// engine/workspace.ts (createBlankDocument, createDocumentFromImage): the
// factory's "mark layer as background" step routes through exactly one native
// protocol apply per commit while the photrez.bgFlagRoute guard is armed
// (default ON: a missing key arms it), and keeps the synchronous direct-setter
// path only when the guard is explicitly opted out with photrez.bgFlagRoute=0.
//
// Wire rules covered here: (c) caller -> funnel -> facade envelope -> native
// apply, (d) exactly one native apply per commit, (e) invalid/unknown-id input
// failing loud. Rules (a) envelope-parse and (b) projection field coverage are
// pinned by bridgeEmuArms.test.ts, hostPlumbingParity.nativeVsWasm.test.ts and
// the Rust arm tests in crates/core.
//
// Mock fidelity: routeNative reproduces the Rust command layer's observable
// contract for the commands this file drives rather than resolving {ok:false}:
// Tauri v2 invoke REJECTS with bare strings, an unseeded doc rejects with
// "document not open", the expectedVersion guard rejects with
// E_VERSION_MISMATCH, SetBackgroundFlag writes isBackground/lockPosition/
// lockRotation plus a 1x1 dirtyRect for a known id, an UNKNOWN id yields an
// empty delta (the Rust arm has no error branch), and documentVersion bumps on
// every accepted apply.

import { describe, it, expect, vi, beforeEach, afterEach, type Mock } from "vitest";
import { DocumentEngine } from "@/engine/document";
import { WorkspaceManager } from "@/engine/workspace";
import {
  getFacade,
  seedFacadeFromEngine,
  __resetFacadeRegistryForTests,
} from "@/lib/protocol/facadeRegistry";
import {
  commitFacadeBackgroundFlag,
  isBackgroundFlagRouteArmed,
  isBackgroundFlagRouteEnabled,
} from "@/lib/protocol/backgroundFlagRouting";
import { invoke } from "@tauri-apps/api/core";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const invokeMock = invoke as unknown as Mock<(cmd: string, args?: Record<string, unknown>) => Promise<unknown>>;

const GUARD_KEY = "photrez.bgFlagRoute";

let applyCount = 0;
let bgFlagApplies = 0;
let lastApplyEnvelope: { expectedVersion?: number; command: Record<string, unknown> } | null = null;
let forceApplyRejection = false;

beforeEach(() => {
  localStorage.setItem("photrez.facade", "1");
  localStorage.setItem("photrez.facadeAuthority", "native");
  // No photrez.bgFlagRoute key: the guard arms by default (missing key = ON).
  localStorage.removeItem(GUARD_KEY);
  applyCount = 0;
  bgFlagApplies = 0;
  lastApplyEnvelope = null;
  forceApplyRejection = false;
  routeNative();
});
afterEach(() => {
  localStorage.removeItem("photrez.facade");
  localStorage.removeItem("photrez.facadeAuthority");
  localStorage.removeItem(GUARD_KEY);
  __resetFacadeRegistryForTests();
  vi.restoreAllMocks();
});

async function makeSeededDoc(docId: string) {
  const engine = new DocumentEngine(docId, docId, 80, 60);
  const facade = getFacade(docId);
  await seedFacadeFromEngine(engine as never, facade);
  await facade.addLayer("L");
  engine.applyFacadeSnapshot(facade.snapshot as never);
  const id = facade.snapshot.layers[facade.snapshot.layers.length - 1].id;
  return { engine, facade, id };
}

const cmdResultJson = (dv: number, changes: unknown[] = []) =>
  JSON.stringify({
    documentVersion: dv,
    delta: { baseVersion: Math.max(0, dv - 1), version: dv, changes },
    status: "ok",
  });

// Faithful mirror of canonical_bridge.rs up_project_fields: the canonical
// push's shared metadata subset into the flat RenderLayer wire shape;
// resourceId and dirtyRect are engine-owned and supplied by the caller.
function upProjectCanonicalFields(c: Record<string, unknown>): Record<string, unknown> {
  const t = (c.transform ?? {}) as {
    x?: number;
    y?: number;
    scaleX?: number;
    scaleY?: number;
    rotation?: number;
    flipH?: boolean;
    flipV?: boolean;
  };
  return {
    id: c.id,
    name: c.name,
    visible: c.visible,
    opacity: c.opacity,
    x: t.x,
    y: t.y,
    scaleX: t.scaleX,
    scaleY: t.scaleY,
    rotation: t.rotation,
    layerType: c.type,
    blendMode: c.blendMode,
    locked: c.locked,
    lockTransparency: c.lockTransparency,
    lockPosition: c.lockPosition,
    lockRotation: c.lockRotation,
    isBackground: c.isBackground,
    hasAdjustments: c.hasAdjustments,
    width: c.width,
    height: c.height,
    flipH: t.flipH,
    flipV: t.flipV,
    shapeParams: c.shapeParams,
    textData: c.textData,
    basicAdjustment: c.basicAdjustment,
  };
}

// Emulated native ProtocolEngine (pattern proven in facadeMetadataOps.test.ts):
// nativeClient delegates to raw invoke, which is mocked above, and this router
// plays the per-document native engine with the contract documented in the
// header.
function routeNative(): void {
  const open = new Set<string>();
  const version = new Map<string, number>();
  const layers = new Map<string, Array<Record<string, unknown>>>();
  const history = new Map<string, Array<{ layers: Array<Record<string, unknown>>; version: number }>>();

  invokeMock.mockImplementation(async (cmd: string, args: Record<string, unknown> = {}) => {
    const docId = (args.docId as string) ?? "default";
    const cur = () => version.get(docId) ?? 0;
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
        return JSON.stringify({ version: version.get(docId), layers: layers.get(docId) });
      }
      case "protocol_seed_canonical_native": {
        if (!open.has(docId)) throw `document not open: ${docId}`;
        // Faithful to seed_canonical: up-project the pushed vector in push
        // order; known ids preserve the engine's resourceId, unknown ids mint
        // fresh ones, ids absent from the push drop. No documentVersion bump,
        // no history entry (document_core.rs seed_canonical).
        const doc = JSON.parse((args.payloadJson as string) ?? "null");
        if (doc && Array.isArray(doc.layers)) {
          const cur2 = layers.get(docId) ?? [];
          const byId = new Map(cur2.map((l) => [l.id as string, l]));
          let frontier = Math.max(0, ...cur2.map((l) => Number(l.resourceId) || 0)) + 1;
          const next: Array<Record<string, unknown>> = [];
          for (const c of doc.layers as Array<Record<string, unknown>>) {
            const ex = byId.get(c.id as string);
            if (ex) {
              next.push({
                ...upProjectCanonicalFields(c),
                resourceId: ex.resourceId,
                dirtyRect: ex.dirtyRect,
              });
            } else {
              const rid = frontier++;
              next.push({ ...upProjectCanonicalFields(c), resourceId: rid });
            }
          }
          layers.set(docId, next);
        }
        return null;
      }
      case "protocol_register_adapter_native":
        return null;
      case "protocol_apply_command_native": {
        if (!open.has(docId)) throw `document not open: ${docId}`;
        const env = JSON.parse((args.envelopeJson as string) ?? "{}");
        applyCount += 1;
        const c = (env.command ?? {}) as Record<string, unknown>;
        if (c.type === "setBackgroundFlag") bgFlagApplies += 1;
        lastApplyEnvelope = env;
        if (env.expectedVersion !== undefined && env.expectedVersion !== cur()) {
          throw `E_VERSION_MISMATCH: expected version ${env.expectedVersion} got ${cur()}`;
        }
        if (forceApplyRejection) throw "E_APPLY_FAILED: forced apply failure";
        if (c.type === "addLayer") {
          history.get(docId)!.push({ layers: (layers.get(docId) ?? []).map((l) => ({ ...l })), version: cur() });
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
          const at = Number(c.index) < 0 ? 0 : Math.min(Number(c.index ?? 0), arr.length);
          arr.splice(at, 0, layer);
          layers.set(docId, arr);
          version.set(docId, cur() + 1);
          return cmdResultJson(cur(), [{ kind: "upsert", layer: { ...layer } }]);
        }
        if (c.type === "setBackgroundFlag") {
          // Known id: flag + position/rotation locks + 1x1 dirtyRect, exactly
          // like document_core_apply.rs SetBackgroundFlag; unknown id: empty
          // changes. Either way the unconditional documentVersion bump at the
          // end of the real apply() runs.
          const arr = (layers.get(docId) ?? []).map((l) => ({ ...l }));
          const idx = arr.findIndex((l) => l.id === c.id);
          version.set(docId, cur() + 1);
          if (idx < 0) return cmdResultJson(cur());
          arr[idx] = {
            ...arr[idx],
            isBackground: true,
            lockPosition: true,
            lockRotation: true,
            dirtyRect: { x: 0, y: 0, width: 1, height: 1 },
          };
          layers.set(docId, arr);
          return cmdResultJson(cur(), [{ kind: "upsert", layer: { ...arr[idx] } }]);
        }
        // Generic OK: bump version, no layer mutation.
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

describe("commitFacadeBackgroundFlag (SetBackgroundFlag native arm)", () => {
  it("armed: ONE setBackgroundFlag apply with expectedVersion; facade and engine settle all three flags", async () => {
    const { engine, facade, id } = await makeSeededDoc("bg-armed-1");
    const vBefore = facade.renderedVersion;
    const appliesBefore = applyCount;

    const r = await commitFacadeBackgroundFlag(engine as never, [id]);

    expect(r.status).toBe("applied");
    expect(r.count).toBe(1);
    expect(bgFlagApplies).toBe(1);
    expect(applyCount - appliesBefore).toBe(1);
    const env = lastApplyEnvelope!;
    expect(env.command.type).toBe("setBackgroundFlag");
    expect(env.command.id).toBe(id);
    expect(env.expectedVersion).toBe(vBefore);
    const fl = facade.snapshot.layers.find((l) => l.id === id)!;
    expect(fl.isBackground).toBe(true);
    expect(fl.lockPosition).toBe(true);
    expect(fl.lockRotation).toBe(true);
    const el = engine.getLayer(id)!;
    expect(el.isBackground).toBe(true);
    expect(el.lockPosition).toBe(true);
    expect(el.lockRotation).toBe(true);
  });

  it("unknown id: rejects loud; native accepted the arm with an empty delta and no layer was flipped", async () => {
    const { engine, id } = await makeSeededDoc("bg-ghost-1");
    const appliesBefore = applyCount;

    await expect(commitFacadeBackgroundFlag(engine as never, ["ghost-layer"])).rejects.toThrow(
      /native engine does not hold this layer/,
    );

    expect(bgFlagApplies).toBe(1);
    expect(applyCount - appliesBefore).toBe(1);
    expect(engine.getLayer(id)!.isBackground ?? false).toBe(false);
  });

  it("empty id list: no-op status, zero applies", async () => {
    const { engine } = await makeSeededDoc("bg-empty-1");
    const appliesBefore = applyCount;

    const r = await commitFacadeBackgroundFlag(engine as never, []);

    expect(r.status).toBe("empty");
    expect(bgFlagApplies).toBe(0);
    expect(applyCount - appliesBefore).toBe(0);
  });
});

describe("photrez.bgFlagRoute default ON (missing key arms the guard)", () => {
  it("guard reader defaults ON when the key is missing; armed with facade + native authority", () => {
    localStorage.removeItem(GUARD_KEY);
    expect(isBackgroundFlagRouteEnabled()).toBe(true);
    expect(isBackgroundFlagRouteArmed()).toBe(true);
  });

  it("armed by default: the funnel fires exactly one native setBackgroundFlag apply", async () => {
    localStorage.removeItem(GUARD_KEY);
    const { engine, id } = await makeSeededDoc("bg-default-on");
    const appliesBefore = applyCount;

    const r = await commitFacadeBackgroundFlag(engine as never, [id]);

    expect(r.status).toBe("applied");
    expect(applyCount - appliesBefore).toBe(1);
    expect(bgFlagApplies).toBe(1);
    expect(lastApplyEnvelope!.command.type).toBe("setBackgroundFlag");
  });
});

describe("photrez.bgFlagRoute=0 opt-out (legacy byte-identical path)", () => {
  it("explicit 0 disables the guard even though the default is ON", () => {
    localStorage.setItem(GUARD_KEY, "0");
    expect(isBackgroundFlagRouteEnabled()).toBe(false);
    expect(isBackgroundFlagRouteArmed()).toBe(false);
  });

  it("explicit 0: funnel reports legacy, zero applies, layer untouched", async () => {
    localStorage.setItem(GUARD_KEY, "0");
    const { engine, id } = await makeSeededDoc("bg-off-fn");
    const appliesBefore = applyCount;

    const r = await commitFacadeBackgroundFlag(engine as never, [id]);

    expect(r.status).toBe("legacy");
    expect(bgFlagApplies).toBe(0);
    expect(applyCount - appliesBefore).toBe(0);
    expect(engine.getLayer(id)!.isBackground ?? false).toBe(false);
  });

  it("explicit 0: createBlankDocument keeps the synchronous direct setter (zero protocol applies)", async () => {
    localStorage.setItem(GUARD_KEY, "0");
    const session = WorkspaceManager.createBlankDocument("bg-off-factory", "Off", 8, 8);
    const bg = session.engine.getLayers().find((l) => l.name === "Background")!;
    expect(bg.isBackground).toBe(true);
    expect(bg.lockPosition).toBe(true);
    expect(bg.lockRotation).toBe(true);
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(applyCount).toBe(0);
    expect(bgFlagApplies).toBe(0);
    expect(isBackgroundFlagRouteArmed()).toBe(false);
  });
});

describe("createBlankDocument background-flag factory wiring (photrez.bgFlagRoute default ON)", () => {
  it("armed: factory commits exactly one native apply and the Background layer settles", async () => {
    const session = WorkspaceManager.createBlankDocument("bg-on-factory", "On", 8, 8);
    const wm = new WorkspaceManager();
    wm.addDocument(session);
    const bg = session.engine.getLayers().find((l) => l.name === "Background")!;

    await vi.waitFor(() => expect(bgFlagApplies).toBe(1));
    await vi.waitFor(() => expect(session.engine.getLayer(bg.id)!.isBackground).toBe(true));

    expect(applyCount).toBe(1);
    const env = lastApplyEnvelope!;
    expect(env.command.type).toBe("setBackgroundFlag");
    expect(env.command.id).toBe(bg.id);
    expect(env.expectedVersion).toBe(0);
    const el = session.engine.getLayer(bg.id)!;
    expect(el.lockPosition).toBe(true);
    expect(el.lockRotation).toBe(true);
  });

  it("armed: createDocumentFromImage commits exactly one native apply and the layer settles", async () => {
    const bitmap = { width: 8, height: 8 } as unknown as ImageBitmap;
    const session = WorkspaceManager.createDocumentFromImage("bg-img-factory", "Img", bitmap);
    const wm = new WorkspaceManager();
    wm.addDocument(session);
    const bg = session.engine.getLayers().find((l) => l.name === "Background")!;

    await vi.waitFor(() => expect(bgFlagApplies).toBe(1));
    await vi.waitFor(() => expect(session.engine.getLayer(bg.id)!.isBackground).toBe(true));

    expect(applyCount).toBe(1);
    expect(lastApplyEnvelope!.command.id).toBe(bg.id);
  });

  it("armed: the funnel opens the native document itself before its first apply (addDocument not called)", async () => {
    // Race guard: the factory fires the funnel before addDocument registers the
    // open/seed barrier, so the first apply must be preceded by the funnel's own
    // seedFacadeFromEngine -> createNativeSeed open. routeNative rejects any
    // apply against an unopened document, which would trip the loud fallback.
    const spy = vi.spyOn(console, "error").mockImplementation(() => {});
    const session = WorkspaceManager.createBlankDocument("bg-self-open", "Self Open", 8, 8);
    const bg = session.engine.getLayers().find((l) => l.name === "Background")!;

    await vi.waitFor(() => expect(bgFlagApplies).toBe(1));
    await vi.waitFor(() => expect(session.engine.getLayer(bg.id)!.isBackground).toBe(true));

    expect(applyCount).toBe(1);
    expect(spy.mock.calls.some((c) => String(c[0]).includes("Background-flag"))).toBe(false);
  });

  it("armed: the open-path canonical shadow seed waits for the factory flag commit and the native flag persists", async () => {
    // Prior tests in this file share one invoke mock; index-based assertions
    // below must see only this test's calls.
    invokeMock.mockClear();
    const session = WorkspaceManager.createBlankDocument("bg-seed-race", "Seed Race", 8, 8);
    const wm = new WorkspaceManager();
    wm.addDocument(session);
    const bg = session.engine.getLayers().find((l) => l.name === "Background")!;

    await vi.waitFor(() => {
      expect(bgFlagApplies).toBe(1);
      expect(invokeMock.mock.calls.some((c) => c[0] === "protocol_seed_canonical_native")).toBe(true);
    });

    const calls = invokeMock.mock.calls;
    const seedIdx = calls.findIndex((c) => c[0] === "protocol_seed_canonical_native");
    const applyIdx = calls.findIndex((c) => {
      if (c[0] !== "protocol_apply_command_native") return false;
      const env = JSON.parse(String((c[1] as Record<string, unknown>).envelopeJson)) as {
        command?: { type?: string };
      };
      return env.command?.type === "setBackgroundFlag";
    });

    // The flag apply must land BEFORE the canonical push: seed_canonical
    // replaces the native layer set with the pushed payload, so a push built
    // before the commit would clobber the native background flag (and a
    // pre-commit payload would not carry isBackground at all).
    expect(applyIdx).toBeGreaterThanOrEqual(0);
    expect(seedIdx).toBeGreaterThan(applyIdx);

    const seedPayload = JSON.parse(
      String((calls[seedIdx][1] as Record<string, unknown>).payloadJson),
    ) as { layers: Array<{ id: string; isBackground?: boolean }> };
    expect(seedPayload.layers.find((l) => l.id === bg.id)?.isBackground).toBe(true);

    const snap = JSON.parse(
      String(await invoke("protocol_snapshot_native", { docId: "bg-seed-race" })),
    ) as { layers: Array<{ id: string; isBackground?: boolean }> };
    expect(snap.layers.find((l) => l.id === bg.id)?.isBackground).toBe(true);
  });

  it("armed + native rejection: console.error fires and the direct setter still settles the layer", async () => {
    forceApplyRejection = true;
    const spy = vi.spyOn(console, "error").mockImplementation(() => {});
    const session = WorkspaceManager.createBlankDocument("bg-fallback-factory", "Fallback", 8, 8);
    const wm = new WorkspaceManager();
    wm.addDocument(session);

    await vi.waitFor(() =>
      expect(spy.mock.calls.some((c) => String(c[0]).includes("Background-flag"))).toBe(true),
    );

    const bg = session.engine.getLayers().find((l) => l.name === "Background")!;
    expect(bg.isBackground).toBe(true);
    expect(bg.lockPosition).toBe(true);
    expect(bg.lockRotation).toBe(true);
    expect(bgFlagApplies).toBe(1);
    // Fallback-state proof: the direct setter's notifyChange re-projects the
    // engine model into the facade snapshot, so the facade is not left
    // unflagged while the engine is flagged.
    await vi.waitFor(() => {
      const fl = getFacade("bg-fallback-factory").snapshot.layers.find((l) => l.id === bg.id);
      expect(fl?.isBackground).toBe(true);
      expect(fl?.lockPosition).toBe(true);
      expect(fl?.lockRotation).toBe(true);
    });
  });
});
