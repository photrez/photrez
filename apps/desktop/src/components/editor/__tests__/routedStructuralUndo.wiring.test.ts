// SPDX-License-Identifier: AGPL-3.0-or-later

/**
 * STRUCTURAL UNDO OF A ROUTED COMPOSITE OP - regression lock for the pixels/structure
 * split that `b124338` opened.
 *
 * THE DEFECT THIS FILE PINS. A merge down / merge selected / flatten seeds the Rust
 * pixel store for its composite destination, and the canonical write for that seed is
 * `rust_pixels_write_region`, which opens a `Pixel` entry on the SAME per-document
 * cursor the structural command records on (`crates/core/src/pixel_store.rs`: the
 * store's `history: ProtocolEngine` is the registry's per-doc engine, and
 * `protocol_apply_command_native` drives that same registry - one cursor, shared).
 * One undo press steps exactly one entry, so an entry left ON TOP of the structural one
 * means the press steps the pixel entry instead: the handoff claims the undo, returns
 * true, `useEditorCommands` returns early, and the structural restore never runs. The
 * app's own log names it - `[facade-history] Rust took a pixel step with no TS twin to
 * drain undo <id>` - and the layer vector is never resurrected.
 *
 * `1a34940` shipped green precisely because NO pixel entry existed, so the handoff
 * FELL THROUGH to the structural branch and the undo worked by accident. The seeding
 * removed the accident. So the invariant here is not "the store is seeded" (that is
 * `routedCompositeSeeding.wiring.test.ts`); it is that the composite destination must
 * NOT carry an independent pixel cursor step, because the gesture's only step is the
 * structural one.
 *
 * COVERAGE BOUNDARY, STATED HONESTLY. In production both surfaces share one Rust
 * cursor. In a vitest harness the pixel surface must be emulated (there is no wasm
 * pixel export) while the graph surface runs on the REAL wasm `ProtocolEngine`, so the
 * two cursors are necessarily distinct here. The witness below is therefore the
 * TS-side half of the defect, stated on the surface that owns it: the seed must leave
 * the destination with zero pixel history steps. The structural contract (one undo
 * press restores every layer in exact original id order with its own raster, redo
 * re-collapses) is asserted in the same cases, over the real routed arms and the real
 * handoff, so a "fix" that drops the structural entry to make the cursor agree fails
 * here too.
 *
 * THE ARM. Every routed case drives the ROUTED function at the shipped origin
 * (`photrez.facade=1`, `facadeAuthority=native`) and asserts the route returned
 * `"applied"`, so a silent fall back to the legacy helper cannot leave these green on
 * the wrong arm. The pixel surface is the existing transport-faithful emulator, which
 * REJECTS where Rust rejects.
 */
import { describe, it, expect, vi, beforeAll, beforeEach, afterEach, type Mock } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { WorkspaceManager } from "@/engine/workspace";
import { getWasmExportModule } from "@/components/editor/wasmExport";
import {
  routeFlatten,
  routeMergeDown,
  routeMergeSelected,
} from "@/components/editor/layers/structuralRouting";
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
import { runFacadeExternalHandoff } from "../facadeHistoryHandoff";
import type { EditorContextValue } from "../shell/EditorContext";
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
const DOC = "routedStructuralUndo";
const SIZE = 64;

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

function installTransport(): void {
  const opened = new Set<string>();
  invokeMock.mockImplementation(async (cmd: string, args: Record<string, unknown> = {}) => {
    if (cmd.startsWith("rust_pixels_")) return store.invoke(cmd, args);
    const rawDocId = (args.docId as string) ?? "";
    const key = `${NS}${rawDocId === "" ? "default" : rawDocId}`;
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

type Engine = ReturnType<WorkspaceManager["getActiveEngine"]> & object;

function makeRenderer(): never {
  return {
    uploadImage: vi.fn(),
    uploadSurfaceTiles: vi.fn(),
    destroyTexture: vi.fn(),
  } as never;
}

async function openSeededDocument(): Promise<Engine> {
  const wm = new WorkspaceManager();
  const session = WorkspaceManager.createBlankDocument(DOC, "Routed", SIZE, SIZE);
  wm.addDocument(session);
  const engine = session.engine as never as Engine;
  await seedFacadeFromEngine(engine as never, getFacade(DOC));
  await awaitNativeSeed(DOC);
  return engine;
}

/** Add a FACADE-OWNED layer with a real raster (see routedCompositeSeeding for why
 *  ownership is required for the routed arms to be reachable at all). */
async function addOwnedLayer(engine: Engine, name: string, fill: string): Promise<string> {
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
  // A real IMAGEBITMAP, not the canvas itself: production stores `ImageBitmap`, and
  // the harness's canvas double can only be read back through `getImageData`, which
  // is what a transferred bitmap exposes. Handing the canvas over instead would make
  // every `drawImage` a silent no-op and every byte comparison vacuously true.
  engine.setLayerImageBitmap(added[0].id, canvas.transferToImageBitmap());
  return added[0].id;
}

/** The layer's real pixels: `ImageBitmap` in production, so read it as one. */
function rasterPixels(engine: Engine, layerId: string): Uint8ClampedArray | null {
  const layer = engine.getLayer(layerId);
  const bitmap = layer?.imageBitmap as unknown as { getImageData?: () => { data: Uint8ClampedArray } };
  if (!bitmap) return null;
  if (typeof bitmap.getImageData === "function") return bitmap.getImageData().data;
  return null;
}

/**
 * A raster signature built from REAL pixel counts, never a hash of a possibly-undefined
 * field: how many pixels carry any alpha or colour, plus a rolling digest over every
 * pixel. Two different rasters must produce different signatures - asserted by
 * `comparatorCanSeeDisagreement` below, because a comparator that cannot see a
 * disagreement proves nothing about an agreement.
 */
function rasterSignature(engine: Engine, layerId: string): string {
  const data = rasterPixels(engine, layerId);
  if (!data) return "no-raster";
  let painted = 0;
  let digest = 0;
  for (let i = 0; i < data.length; i += 4) {
    const r = data[i];
    const g = data[i + 1];
    const b = data[i + 2];
    const a = data[i + 3];
    if (r !== 0 || g !== 0 || b !== 0 || a !== 0) painted += 1;
    digest = (digest * 31 + r + g * 3 + b * 7 + a * 11) >>> 0;
  }
  return `painted=${painted}/${data.length / 4} digest=${digest}`;
}

function layerVector(engine: Engine): string[] {
  return engine.getLayers().map((l) => l.id);
}

function makeEditor(engine: Engine): EditorContextValue {
  return {
    workspace: {
      getActiveEngine: () => engine,
      notifyVisualChange: vi.fn(),
    },
    renderer: { uploadImage: vi.fn() },
    scheduler: { requestRender: vi.fn() },
  } as unknown as EditorContextValue;
}

function countWrites(): number {
  return store.calls.filter((c) => c.cmd === "rust_pixels_write_region").length;
}

/**
 * Wait until the destination's pixel cursor steps settle, then report how many the
 * seed left behind. Polling rather than a fixed sleep: the seed is fire-and-forget in
 * production, so a test that reads one tick early would race the implementation rather
 * than measure it.
 */
async function settledPixelSteps(
  layerId: string,
  attempts = 25,
): Promise<{ present: boolean; steps: number; tiles: number; bytes: number }> {
  let last: { present: boolean; steps: number; tiles: number; bytes: number } = {
    present: false,
    steps: -1,
    tiles: -1,
    bytes: -1,
  };
  for (let i = 0; i < attempts; i += 1) {
    const l = store.layers.get(layerId);
    if (l) {
      last = { present: true, steps: l.history.length, tiles: -1, bytes: l.pixels.length };
      const tiles = (await store.invoke("rust_pixels_snapshot_layer", {
        docId: DOC,
        layerId,
      })) as Array<{ w: number; h: number }>;
      last.tiles = tiles.reduce((n, t) => n + t.w * t.h, 0);
      return last;
    }
    await new Promise((r) => setTimeout(r, 5));
  }
  return last;
}

/** Byte-for-byte: the store buffer must equal the raster the layer holds. */
function storeEqualsBitmap(engine: Engine, layerId: string): boolean {
  const want = rasterPixels(engine, layerId);
  const stored = store.layers.get(layerId);
  if (!want || !stored) return false;
  if (want.length !== stored.pixels.length) return false;
  for (let i = 0; i < want.length; i += 1) {
    if (want[i] !== stored.pixels[i]) return false;
  }
  return true;
}

beforeAll(async () => {
  const mod = (await getWasmExportModule()) as WasmModule | null;
  expect(mod, "the REAL wasm pkg must load").not.toBeNull();
  wasmMod = mod!;
  (globalThis as Record<string, unknown>)[TAURI_KEY] = { invoke: invokeMock };
});

beforeEach(() => {
  localStorage.clear();
  localStorage.setItem("photrez.facade", "1");
  localStorage.setItem("photrez.facadeAuthority", "native");
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

describe("comparator sanity - a signature that cannot see a disagreement proves nothing", () => {
  it("two different rasters produce different signatures", async () => {
    const engine = await openSeededDocument();
    const a = await addOwnedLayer(engine, "A", "#cc3366");
    const b = await addOwnedLayer(engine, "B", "#33cc66");
    const sa = rasterSignature(engine, a);
    const sb = rasterSignature(engine, b);
    expect(sa).not.toBe("no-raster");
    expect(sb).not.toBe("no-raster");
    expect(sa).not.toBe(sb);
    // Real counts, not just a digest: both fills cover every pixel.
    expect(sa.startsWith(`painted=${SIZE * SIZE}/`)).toBe(true);
    expect(sb.startsWith(`painted=${SIZE * SIZE}/`)).toBe(true);
  });
});

describe("routed composite op: one gesture, one undo, structure restored", () => {
  /** Drive one routed op end to end and assert the whole contract. */
  async function checkOp(
    name: string,
    drive: (engine: Engine) => Promise<unknown>,
  ): Promise<void> {
    const engine = await openSeededDocument();
    await addOwnedLayer(engine, "Second", "#cc3366");
    await addOwnedLayer(engine, "Third", "#33cc66");
    const beforeIds = layerVector(engine);
    const beforeSignatures = beforeIds.map((id) => rasterSignature(engine, id));
    expect(beforeIds.length, `premise: ${name} needs more than one layer`).toBeGreaterThan(1);
    store.calls.length = 0;

    // THE ARM ASSERTION. Without it a silent fall back to the legacy helper would
    // leave this green on the arm the shipped default never takes.
    const status = await drive(engine);
    expect(status, `${name} ran on the routed arm`).toBe("applied");

    const destination = engine.getLayer(engine.getActiveLayerId() || "");
    expect(destination, "the op produced a composite destination").toBeTruthy();
    const mergedId = destination!.id;
    expect(layerVector(engine), `${name} changed the layer vector`).not.toEqual(beforeIds);

    // --- the seeding survives (do NOT regress b124338) -------------------------
    const settled = await settledPixelSteps(mergedId);
    expect(settled.present, `the destination ${mergedId} is seeded in the Rust store`).toBe(true);
    expect(settled.tiles, "the store holds the whole composite").toBe(SIZE * SIZE);
    expect(countWrites(), `${name} emitted exactly one canonical pixel write`).toBe(1);
    // Non-vacuous by construction: the byte comparison below is only meaningful
    // because the destination really carries pixels.
    expect(
      rasterSignature(engine, mergedId).startsWith(`painted=${SIZE * SIZE}/`),
      `${name}: the composite destination holds real pixels`,
    ).toBe(true);
    expect(
      storeEqualsBitmap(engine, mergedId),
      `store bytes for ${mergedId} equal layer.imageBitmap byte for byte`,
    ).toBe(true);

    // --- THE INVARIANT: the gesture is ONE cursor step, and it is structural ----
    // A `Pixel` entry sitting on top of the structural one is what made undo claim
    // a step it could not complete. The destination's own history must therefore be
    // EMPTY: the composite's pixels are a projection under the structural entry,
    // not an independent edit with its own step to consume.
    expect(
      settled.steps,
      `${name}: the composite destination carries no independent pixel cursor step`,
    ).toBe(0);

    // --- ONE undo press restores every layer, in order, with its own raster -----
    const collapsedIds = layerVector(engine);
    const handled = await runFacadeExternalHandoff(makeEditor(engine), "undo");
    expect(handled, `${name}: the handoff owns the undo`).toBe(true);
    expect(layerVector(engine), `${name}: undo restored the exact original id order`).toEqual(
      beforeIds,
    );
    const afterIds = layerVector(engine);
    const afterSignatures = afterIds.map((id) => rasterSignature(engine, id));
    expect(afterSignatures, `${name}: every layer came back with its own raster`).toEqual(
      beforeSignatures,
    );

    // --- redo re-collapses identically ----------------------------------------
    const redone = await runFacadeExternalHandoff(makeEditor(engine), "redo");
    expect(redone, `${name}: the handoff owns the redo`).toBe(true);
    expect(layerVector(engine), `${name}: redo re-collapsed the stack`).toEqual(collapsedIds);
    expect(rasterSignature(engine, mergedId), `${name}: redo restored the composite`).not.toBe(
      "no-raster",
    );
    void name;
  }

  it("routeFlatten: one undo restores every layer; the seed leaves no extra step", async () => {
    await checkOp("flatten", (engine) => routeFlatten(engine as never, null, makeRenderer()));
  });

  it("routeMergeSelected: one undo restores every layer; the seed leaves no extra step", async () => {
    await checkOp("mergeSelected", async (engine) => {
      const ids = engine.getLayers().map((l) => l.id);
      return routeMergeSelected(engine as never, null, makeRenderer(), ids);
    });
  });

  it("routeMergeDown: one undo restores every layer; the seed leaves no extra step", async () => {
    await checkOp("mergeDown", async (engine) => {
      // Stack order is top-first; the merge partner is at idx + 1.
      const topId = engine.getLayers()[0].id;
      return routeMergeDown(engine as never, null, makeRenderer(), topId);
    });
  });
});
