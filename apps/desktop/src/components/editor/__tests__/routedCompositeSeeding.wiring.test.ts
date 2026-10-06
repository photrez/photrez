// SPDX-License-Identifier: AGPL-3.0-or-later

/**
 * CANONICAL SEEDING ON THE ROUTED ARM - the arm the SHIPPED DEFAULT takes.
 *
 * WHY THIS FILE EXISTS. `1a34940` put the composite seeding on the LEGACY helpers
 * (`mergeActiveLayerDown` / `mergeSelectedLayers` / `flattenAllLayers` in
 * `layerOperations.ts`) and its tests drove those helpers directly. The real app
 * then measured the gap still open at the default: with `photrez.facade="1"` and
 * `facadeAuthority="native"`, `layerOps.ts` calls `routeFlatten` /
 * `routeMergeSelected` / `routeMergeDown` FIRST, they return `"applied"`, and
 * `afterRoute` takes the applied branch - so the legacy helpers that own the seed
 * are NEVER REACHED. Measured: `seeded: false` across 31 polls over 34s,
 * `rust_pixels_write_region` census delta 0, store read rejecting with
 * `layer not initialized: layer-xygc5dkm`. The `facade=0` control gave
 * `seeded: true` on the first poll with `byteForByte_store_equals_bitmap: true`,
 * which proved the seed+write code was CORRECT and merely unreachable.
 *
 * THE LESSON, WHICH IS THE POINT: a test that drives only the legacy arm is green
 * while the shipped default never reaches it. Every case below therefore drives the
 * ROUTED function itself, with the facade on and native authority active, and
 * asserts the ROUTE actually returns `"applied"` - so a future change that makes
 * the route fall back to legacy cannot leave these cases green on the wrong arm.
 *
 * AUTHORITY: native, the shipped default, through a Tauri transport shim over the
 * REAL wasm engine (the technique `externalEntryCapturesHostPreOpState.wiring`
 * uses). The pixel-store commands are served by the existing transport-faithful
 * `rustStoreEmulator`, which REJECTS where Rust rejects; the seed relies on that
 * rejection as its "no store yet" signal, so a mock that RESOLVED instead would
 * make every routed merge skip its seed and report convergence with nothing behind
 * it - the third shipped green-suite defect, avoided by construction.
 *
 * LEGACY CONTROL: one case pins the `facade=0` arm still seeding and writing. It is
 * the control that proves the code itself works, and it must never regress.
 */
import { describe, it, expect, vi, beforeAll, beforeEach, afterEach, type Mock } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { WorkspaceManager } from "@/engine/workspace";
import { getWasmExportModule } from "@/components/editor/wasmExport";
import { routeFlatten, routeMergeDown, routeMergeSelected } from "@/components/editor/layers/structuralRouting";
import { flattenAllLayers, mergeActiveLayerDown } from "@/components/editor/layers/layerOperations";
import {
  __resetFacadeRegistryForTests,
  seedFacadeFromEngine,
  getFacade,
} from "@/lib/protocol/facadeRegistry";
import { __resetNativeAuthorityForTests, awaitNativeSeed } from "@/lib/protocol/bridge";
import { __resetCanonicalRepushForTests } from "@/lib/protocol/canonicalSeed";
import {
  createRustStoreEmulator,
  type RustStoreEmulator,
} from "@/lib/paint/__tests__/rustStoreEmulator";
import { installFaithfulCanvas } from "@/__tests__/faithfulOffscreenCanvas";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const invokeMock = invoke as unknown as Mock<
  (cmd: string, args: Record<string, unknown>) => Promise<unknown>
>;

type WasmModule = {
  protocol_reset: (docId: string) => void;
  protocol_apply_command: (json: string, docId: string) => string;
  protocol_snapshot_json: (docId: string) => string;
  protocol_version: (docId: string) => bigint;
  protocol_history_query_json: (docId: string) => string;
  protocol_history_cursor_commit: (json: string, docId: string) => string;
  protocol_register_payload_adapter: (adapterId: string, docId: string) => void;
  protocol_seed_canonical: (json: string, docId: string) => string;
};

let wasmMod: WasmModule;
let store: RustStoreEmulator;
let restoreCanvas: (() => void) | undefined;

const TAURI_KEY = "__TAURI_INTERNALS__";
const NS = "native::";

/** The wasm bridge rejects with a JSON `{code,message}` string; the native command
 *  rejects with the bare `"CODE: message"` string (protocol_native_cmds.rs:59). */
function nativeRejection(e: unknown): string {
  const raw = e instanceof Error ? e.message : String(e);
  try {
    const parsed = JSON.parse(raw) as { code?: unknown; message?: unknown };
    if (typeof parsed?.code === "string" && typeof parsed?.message === "string") {
      return `${parsed.code}: ${parsed.message}`;
    }
  } catch {
    /* not a JSON envelope */
  }
  return raw;
}

/**
 * The production transport: real graph commands through the REAL wasm engine, pixel
 * commands through the transport-faithful emulator. Everything else REJECTS, so a
 * command the harness forgot cannot silently resolve.
 */
function installTransport(): void {
  const opened = new Set<string>();
  invokeMock.mockImplementation(async (cmd: string, args: Record<string, unknown> = {}, options?: any) => {
    if (cmd.startsWith("rust_pixels_")) return store.invoke(cmd, args, options);
    const rawDocId = (args.docId as string) ?? "";
    const docId = rawDocId === "" ? "default" : rawDocId;
    const key = `${NS}${docId}`;
    switch (cmd) {
      case "rust_pixels_open_document":
        if (!opened.has(key)) {
          opened.add(key);
          wasmMod.protocol_reset(key);
        }
        return undefined;
      case "protocol_seed_native":
        return JSON.stringify({ version: 0, layers: [] });
      case "protocol_seed_canonical_native":
        return wasmMod.protocol_seed_canonical(args.payloadJson as string, key);
      case "protocol_apply_command_native":
        try {
          return wasmMod.protocol_apply_command(args.envelopeJson as string, key);
        } catch (e) {
          throw nativeRejection(e);
        }
      case "protocol_snapshot_native":
        return wasmMod.protocol_snapshot_json(key);
      case "protocol_version_native":
        return Number(wasmMod.protocol_version(key));
      case "protocol_history_query_native":
        return wasmMod.protocol_history_query_json(key);
      case "protocol_history_cursor_commit_native":
        try {
          return wasmMod.protocol_history_cursor_commit(
            JSON.stringify({ seq: args.seq, direction: args.direction }),
            key,
          );
        } catch (e) {
          throw nativeRejection(e);
        }
      case "protocol_register_adapter_native":
        wasmMod.protocol_register_payload_adapter(args.adapterId as string, key);
        return null;
      default:
        throw `E_UNKNOWN_COMMAND: ${cmd}`;
    }
  });
}

const DOC = "routedCompositeSeed";
const SIZE = 64;

/** A renderer double: these ops only touch GPU bookkeeping. */
function makeRenderer(): never {
  return {
    uploadImage: vi.fn(),
    uploadSurfaceTiles: vi.fn(),
    destroyTexture: vi.fn(),
  } as never;
}

/**
 * Open a document the way production does and give every layer a raster, so the
 * routed arms composite real pixels rather than empty buffers.
 */
async function openSeededDocument(): Promise<{
  engine: ReturnType<WorkspaceManager["getActiveEngine"]> & object;
}> {
  const wm = new WorkspaceManager();
  const session = WorkspaceManager.createBlankDocument(DOC, "Routed", SIZE, SIZE);
  wm.addDocument(session);
  const engine = session.engine as never as ReturnType<WorkspaceManager["getActiveEngine"]> & object;
  await seedFacadeFromEngine(engine as never, getFacade(DOC));
  await awaitNativeSeed(DOC);
  return { engine };
}

/**
 * Add a FACADE-OWNED layer with a real raster, the way the app does when a layer
 * is added while the facade owns the graph.
 *
 * OWNERSHIP IS WHY: `resolveSelectionRoute` returns "legacy" unless EVERY id in
 * the selection is facade-owned, and ownership is granted only by
 * `applyFacadeSnapshot` calling `markFacadeOwned`. A layer added through
 * `engine.addLayer` is host-created and therefore NOT owned, so a routed op over
 * it declines to "legacy" - which is correct production behaviour and the reason
 * the routed arms need owned layers to be reachable at all. Going through the
 * facade's own `addLayer` mints the id, projects the layer, and claims it.
 */
async function addOwnedLayer(
  engine: ReturnType<WorkspaceManager["getActiveEngine"]> & object,
  name: string,
  fill: string,
): Promise<string> {
  const facade = getFacade(DOC);
  const before = new Set(engine.getLayers().map((l) => l.id));
  await facade.addLayer(name, SIZE, SIZE);
  await engine.applyFacadeSnapshot(facade.snapshot as never);
  const added = engine.getLayers().filter((l) => !before.has(l.id));
  expect(added.length, `${name} was added through the facade`).toBe(1);
  const canvas = new OffscreenCanvas(SIZE, SIZE);
  const ctx = canvas.getContext("2d") as unknown as CanvasRenderingContext2D;
  ctx.fillStyle = fill;
  ctx.fillRect(0, 0, SIZE, SIZE);
  engine.setLayerImageBitmap(added[0].id, canvas as unknown as ImageBitmap);
  return added[0].id;
}

/**
 * The measured fact, read through the store's OWN read command. Returns the tile
 * coverage rather than a hash, so nothing here can pass on a field that came back
 * undefined - the vacuous `hashMatch: true` failure mode.
 */
async function readStoreTileBoxes(layerId: string): Promise<Array<{ x: number; y: number; w: number; h: number }>> {
  const tiles = (await store.invoke("rust_pixels_snapshot_layer", {
    docId: DOC,
    layerId,
  })) as Array<{ x: number; y: number; w: number; h: number; data: number[] }>;
  expect(Array.isArray(tiles), "the store read returned an array").toBe(true);
  expect(tiles.length, "the store has real tiles for the merged layer").toBeGreaterThan(0);
  for (const t of tiles) {
    expect(typeof t.w, "tile width is a number").toBe("number");
    expect(t.data.length, "tile carries pixels").toBe(t.w * t.h * 4);
  }
  return tiles.map((t) => ({ x: t.x, y: t.y, w: t.w, h: t.h }));
}

function countWrites(): number {
  return store.calls.filter((c) => c.cmd === "rust_pixels_write_region").length;
}

/** Drain the fire-and-forget seed: production readers poll, so this polls too. */
async function pollForStore(
  layerId: string,
  attempts = 20,
): Promise<Array<{ x: number; y: number; w: number; h: number }>> {
  let last: unknown;
  for (let i = 0; i < attempts; i += 1) {
    try {
      return await readStoreTileBoxes(layerId);
    } catch (err) {
      last = err;
      await new Promise((r) => setTimeout(r, 5));
    }
  }
  throw new Error(`store never appeared for ${layerId}: ${String(last)}`);
}

beforeAll(async () => {
  const mod = (await getWasmExportModule()) as WasmModule | null;
  expect(mod, "the REAL wasm pkg must load").not.toBeNull();
  wasmMod = mod!;
  (globalThis as Record<string, unknown>)[TAURI_KEY] = { invoke: invokeMock };
});

beforeEach(() => {
  localStorage.clear();
  restoreCanvas = installFaithfulCanvas();
  invokeMock.mockReset();
  store = createRustStoreEmulator();
  __resetNativeAuthorityForTests();
  __resetCanonicalRepushForTests();
  __resetFacadeRegistryForTests();
  installTransport();
  wasmMod.protocol_reset(`${NS}${DOC}`);
});

afterEach(() => {
  store.dispose();
  localStorage.clear();
  restoreCanvas?.();
  restoreCanvas = undefined;
  __resetNativeAuthorityForTests();
  __resetCanonicalRepushForTests();
  __resetFacadeRegistryForTests();
  vi.restoreAllMocks();
});

describe("ROUTED arm (facade on, native authority - the shipped default)", () => {
  beforeEach(() => {
    localStorage.setItem("photrez.facade", "1");
    localStorage.setItem("photrez.facadeAuthority", "native");
  });

  it("routeFlatten seeds the store and emits ONE canonical write", async () => {
    const { engine } = await openSeededDocument();
    await addOwnedLayer(engine, "Second", "#cc3366");
    await addOwnedLayer(engine, "Third", "#33cc66");
    expect(engine.getLayers().length, "premise: flatten needs more than one layer").toBeGreaterThan(1);
    store.calls.length = 0;

    const status = await routeFlatten(engine as never, null, makeRenderer());

    // THE ARM ASSERTION. Without this the case could pass while the route fell
    // back to legacy - which is exactly how `1a34940` shipped green.
    expect(status, "the routed arm ran, not the legacy fallback").toBe("applied");

    const flattened = engine.getLayer(engine.getActiveLayerId() || "")!;
    expect(engine.getLayers().length, "premise: the routed flatten collapsed the stack").toBe(1);
    expect(
      await pollForStore(flattened.id),
      "the flattened layer is seeded in the Rust store",
    ).toEqual([{ x: 0, y: 0, w: SIZE, h: SIZE }]);
    expect(countWrites(), "exactly one canonical write for the flatten").toBe(1);
  });

  it("routeMergeSelected seeds the store and emits ONE canonical write", async () => {
    const { engine } = await openSeededDocument();
    await addOwnedLayer(engine, "Second", "#cc3366");
    const ids = engine.getLayers().map((l) => l.id);
    expect(ids.length, "premise: two or more owned layers to merge").toBeGreaterThanOrEqual(2);
    store.calls.length = 0;

    const status = await routeMergeSelected(engine as never, null, makeRenderer(), ids);

    expect(status, "the routed arm ran, not the legacy fallback").toBe("applied");
    const merged = engine.getLayer(engine.getActiveLayerId() || "")!;
    expect(
      await pollForStore(merged.id),
      "the merged layer is seeded in the Rust store",
    ).toEqual([{ x: 0, y: 0, w: SIZE, h: SIZE }]);
    expect(countWrites(), "exactly one canonical write for the merge").toBe(1);
  });

  it("routeMergeDown seeds the store and emits ONE canonical write", async () => {
    const { engine } = await openSeededDocument();
    await addOwnedLayer(engine, "Below", "#cc3366");
    // Stack order is top-first: `routeMergeDown` takes its partner at `idx + 1`,
    // so the layer being merged must be the one at index 0.
    const topId = engine.getLayers()[0].id;
    expect(
      engine.getLayers().length,
      "premise: a partner sits below the top layer",
    ).toBeGreaterThan(1);
    store.calls.length = 0;

    const status = await routeMergeDown(engine as never, null, makeRenderer(), topId);

    expect(status, "the routed arm ran, not the legacy fallback").toBe("applied");
    const merged = engine.getLayer(engine.getActiveLayerId() || "")!;
    expect(
      await pollForStore(merged.id),
      "the merged layer is seeded in the Rust store",
    ).toEqual([{ x: 0, y: 0, w: SIZE, h: SIZE }]);
    expect(countWrites(), "exactly one canonical write for the merge-down").toBe(1);
  });
});

describe("LEGACY control (facade off) - the arm that proved the code works", () => {
  beforeEach(() => {
    // Explicit OFF, not absent: isFacadeEnabled is `!== "0"`, so an UNSET key
    // means ENABLED and the control would silently drive the routed arm.
    localStorage.setItem("photrez.facade", "0");
    localStorage.setItem("photrez.facadeAuthority", "native");
  });

  it("the legacy flatten helper still seeds the store and writes once", async () => {
    const { engine } = await openSeededDocument();
    // Host-created layers, NOT facade-owned: the legacy arm is what a
    // non-owned selection takes, so adding owned layers here would route around
    // the very control this case exists to pin.
    const extra = engine.addLayer("Host", SIZE, SIZE);
    const canvas = new OffscreenCanvas(SIZE, SIZE);
    (canvas.getContext("2d") as unknown as CanvasRenderingContext2D).fillRect(0, 0, SIZE, SIZE);
    engine.setLayerImageBitmap(extra.id, canvas as unknown as ImageBitmap);
    expect(engine.getLayers().length, "premise: flatten needs more than one layer").toBeGreaterThan(1);
    store.calls.length = 0;

    // Prove the CONTROL is on the legacy arm: with the facade off the routed arm
    // must decline, or this control is not a control.
    expect(await routeFlatten(engine as never, null, makeRenderer())).toBe("legacy");

    expect(flattenAllLayers(engine as never, { commit: () => {} } as never, makeRenderer())).toBe(true);

    const flattened = engine.getLayer(engine.getActiveLayerId() || "")!;
    expect(
      await pollForStore(flattened.id),
      "the legacy control still seeds the store",
    ).toEqual([{ x: 0, y: 0, w: SIZE, h: SIZE }]);
    expect(countWrites(), "the legacy control still writes once").toBe(1);
  });

  it("the legacy merge-down helper still seeds the store and writes once", async () => {
    const { engine } = await openSeededDocument();
    const extra = engine.addLayer("Host", SIZE, SIZE);
    const canvas = new OffscreenCanvas(SIZE, SIZE);
    (canvas.getContext("2d") as unknown as CanvasRenderingContext2D).fillRect(0, 0, SIZE, SIZE);
    engine.setLayerImageBitmap(extra.id, canvas as unknown as ImageBitmap);
    const activeId = engine.getLayer(engine.getActiveLayerId() || "")!.id;
    store.calls.length = 0;

    expect(await routeMergeDown(engine as never, null, makeRenderer(), activeId)).toBe("legacy");

    expect(mergeActiveLayerDown(engine as never, { commit: () => {} } as never, makeRenderer(), activeId)).toBe(true);

    const merged = engine.getLayer(engine.getActiveLayerId() || "")!;
    expect(
      await pollForStore(merged.id),
      "the legacy control still seeds the store",
    ).toEqual([{ x: 0, y: 0, w: SIZE, h: SIZE }]);
    expect(countWrites(), "the legacy control still writes once").toBe(1);
  });
});