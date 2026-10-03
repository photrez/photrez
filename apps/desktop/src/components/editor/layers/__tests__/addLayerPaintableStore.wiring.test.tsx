// SPDX-License-Identifier: AGPL-3.0-or-later

// ADD-LAYER MUST LAND THE USER ON A PAINTABLE LAYER - regression lock for the
// single-owner violation where the most ordinary sequence a user performs escaped the
// Rust pixel owner entirely.
//
// THE DEFECTS. Both measured in the running app.
//
//  1. A freshly added `New Layer` arrives from the facade projection with NO raster.
//     `applyFacadeSnapshot` rebuilds a layer it has no retained node for as metadata
//     only (document.ts:2258-2268) - bitmaps live in the JS heap and are re-attached
//     by id, and a graph-minted id has none. `getPaintSurface` then returns null
//     (document.ts:1447), the brush's tile path is skipped
//     (useBrushOverlay.ts:1205-1221), and the stroke commits through the LEGACY
//     TYPESCRIPT path. Observed: `[perf] commitBrushStroke(tile): no paint surface for
//     layer, legacy fallback`, a `ts:doc-...:<uuid>` payloadRef with whole-document
//     affectedLayerIds, and NO pixel store row on any layer. Absent, not a rejected
//     read. The store does NOT need seeding here: `c4CoreCommit` already seeds it from
//     the surface on first paint (`rust_pixels_init`, useBrushOverlay.ts:217-220).
//     The raster is the whole defect.
//
//  2. The facade arm of `handleAddLayer` never selected what it added, while the
//     flag-off arm's `engine.addLayer` does and the routed duplicate arm does
//     (`setSelectedLayerIds(res.newIds)`, useLayerActions.ts:99).
//     `applyFacadeSnapshot` only reassigns the active layer when the current one LEFT
//     the projection (document.ts:2302-2304). Observed: after two Ctrl+Shift+N the
//     active layer was still `Background` - so the user painted on the wrong layer.
//
// THE ARM. The REAL `handleAddLayer` from the REAL hook, under the shipped default
// (`photrez.facade=1`, `facadeAuthority=native`), with the real wasm ProtocolEngine
// behind the transport and the transport-faithful pixel emulator that REJECTS where
// Rust rejects. The flag-off arm is asserted separately and must be unchanged.
//
// NOT ASSERTED HERE, AND DELIBERATELY SO: the stroke's own pixel bytes. Driving
// `useBrushOverlay`'s pointer chain needs the overlay canvas, the C4 deferred enqueue
// and the GPU surface, none of which jsdom provides honestly. What IS asserted is the
// exact gate the tile path turns on (`surface` non-null, useBrushOverlay.ts:1221) plus
// the store-level proof that a canonical stroke can then be accepted - which is the
// part that was escaping the Rust owner.

import { describe, it, expect, vi, beforeAll, beforeEach, afterEach, afterAll, type Mock } from "vitest";
import { render } from "solid-js/web";
import { invoke } from "@tauri-apps/api/core";
import { WorkspaceManager } from "@/engine/workspace";
import { getWasmExportModule } from "@/components/editor/wasmExport";
import { EditorProvider, useEditor } from "../../shell/EditorContext";
import { useLayerActions } from "../useLayerActions";
import {
  __resetFacadeRegistryForTests,
  seedFacadeFromEngine,
  getFacade,
  installFacadeCommitShim,
} from "@/lib/protocol/facadeRegistry";
import { __resetNativeAuthorityForTests, awaitNativeSeed, flushExternalTransitions } from "@/lib/protocol/bridge";
import { __resetCanonicalRepushForTests } from "@/lib/protocol/canonicalSeed";
import { createRustStoreEmulator, type RustStoreEmulator } from "@/lib/paint/__tests__/rustStoreEmulator";
import { installFaithfulCanvas } from "@/__tests__/faithfulOffscreenCanvas";
import * as Toast from "../../Toast";

// `handleAddLayer` swallows every failure into a toast (useLayerActions.ts:789-797), so
// an exception inside its post-projection block - a canvas transferred before its 2d
// context exists throws InvalidStateError - is invisible to any assertion about engine
// state. Collecting the toasts turns that silent catch into a reported failure.
const toastCalls: Array<{ message: string; kind?: string }> = [];
vi.mock("../../Toast", () => ({
  showToast: (message: string, kind?: string) => {
    toastCalls.push({ message, kind });
  },
}));

/**
 * ONLY the toasts `handleAddLayer` itself can emit, from either arm
 * (useLayerActions.ts:792, :795, :805). Startup also toasts, and it must not be able to
 * fail this assertion: the transport-faithful mock correctly REJECTS
 * `get_pending_open_path`, which surfaces as "Failed to open file from command line:
 * E_UNKNOWN_COMMAND: get_pending_open_path". That is the mock being faithful, not the
 * handler failing, so the filter is by MESSAGE rather than by dropping the assertion -
 * the assertion is what named the real cause and it has to survive.
 */
function handlerToasts(): Array<{ message: string; kind?: string }> {
  return toastCalls.filter(
    (t) => t.message.includes("Cannot add layer") || t.message.includes("Version conflict"),
  );
}

/**
 * The arm this body is about to run, asserted where it cannot be misread. `isFacadeEnabled`
 * is `localStorage.getItem("photrez.facade") !== "0"` (bridge.ts:78-85), so REMOVING the key
 * ENABLES the facade - `null !== "0"` is true. A test named for the flag-off arm that clears
 * the key silently runs the facade arm instead, and then its pass/fail tracks only the facade
 * fix. That is what happened here for three rounds.
 */
function assertArm(expected: "on" | "off"): void {
  const raw = localStorage.getItem("photrez.facade");
  const enabled = raw !== "0";
  expect(
    { raw, enabled, expected, expectedRaw: expected === "on" ? "1" : "0" },
    "the arm under test must be the arm this case is named for: isFacadeEnabled reads !== \"0\", so an absent key ENABLES the facade",
  ).toMatchObject({ enabled: expected === "on", expectedRaw: raw });
}

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const invokeMock = invoke as unknown as Mock<(cmd: string, args: Record<string, unknown>) => Promise<unknown>>;

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

type Engine = ReturnType<WorkspaceManager["getActiveEngine"]> & object;

let wasmMod: WasmModule;
let store: RustStoreEmulator;
let restoreCanvas: (() => void) | undefined;
let liveEngine: { getId(): string; getLayers(): Array<{ id: string }> } | null = null;

const TAURI_KEY = "__TAURI_INTERNALS__";
const NS = "native::";
// Unique per file: the graph mirror is keyed by document id and cases bleed without one.
const DOC = "addLayerPaintableStore";
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

/** `rust_pixels_*` to the emulator (it REJECTS where Rust rejects), `protocol_*_native` to the REAL wasm engine. */
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
          return wasmMod.protocol_history_cursor_commit(JSON.stringify({ seq: args.seq, direction: args.direction }), key);
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

function makeRenderer() {
  return { uploadImage: vi.fn(), uploadSurfaceTiles: vi.fn(), destroyTexture: vi.fn() } as never;
}
function makeScheduler() {
  return { requestRender: vi.fn() } as never;
}

/** REAL pixel count, never a hash. `-1` is the "carries no raster at all" sentinel. */
function paintedPixelCount(engine: Engine, layerId: string): number {
  const bitmap = engine.getLayer(layerId)?.imageBitmap as unknown as
    | { getImageData?: () => { data: Uint8ClampedArray } }
    | undefined;
  if (!bitmap) return -1;
  if (typeof bitmap.getImageData !== "function") return -1;
  const data = bitmap.getImageData().data;
  let painted = 0;
  for (let i = 0; i < data.length; i += 4) {
    if (data[i] !== 0 || data[i + 1] !== 0 || data[i + 2] !== 0 || data[i + 3] !== 0) painted += 1;
  }
  return painted;
}

interface Mounted {
  engine: Engine;
  addLayer: () => Promise<void>;
  selectedLayerId: () => string | null;
  dispose: () => void;
}

function mountEditor(docId: string): Mounted {
  const wm = new WorkspaceManager();
  const session = WorkspaceManager.createBlankDocument(docId, "Add Layer", SIZE, SIZE);
  wm.addDocument(session);
  const engine = session.engine as never as Engine;
  liveEngine = engine as never;

  let addLayer: () => Promise<void> = async () => {};
  let selectedLayerId: () => string | null = () => null;
  const container = document.createElement("div");
  document.body.appendChild(container);
  const dispose = render(
    () => (
      <EditorProvider workspace={wm} renderer={makeRenderer()} scheduler={makeScheduler()}>
        <Capture
          onReady={(fn, sel) => {
            addLayer = fn;
            selectedLayerId = sel;
          }}
        />
      </EditorProvider>
    ),
    container,
  );
  return { engine, addLayer: () => addLayer(), selectedLayerId: () => selectedLayerId(), dispose };
}

/** Captures the real hook's handlers once. Writes nothing during render. */
function Capture(props: { onReady: (addLayer: () => Promise<void>, selectedLayerId: () => string | null) => void }) {
  const editor = useEditor();
  const actions = useLayerActions();
  props.onReady(actions.handleAddLayer, () => editor.selectedLayerId());
  return null;
}

async function primeFacade(engine: Engine, docId: string): Promise<void> {
  await seedFacadeFromEngine(engine as never, getFacade(docId));
  await awaitNativeSeed(docId);
}

/**
 * The store-level proof the tile path depends on, replaying exactly what
 * `c4CoreCommit` does on a first paint (useBrushOverlay.ts:212-220): probe the epoch,
 * seed when absent, then take the one canonical write. If this rejects, the stroke
 * could not have reached the Rust owner no matter what the surface looked like.
 */
async function firstCanonicalStroke(layerId: string): Promise<{ seeded: boolean; wroteBytes: number; steps: number }> {
  let seeded = false;
  try {
    await store.invoke("rust_pixels_get_epoch", { docId: DOC, layerId });
  } catch {
    seeded = true;
    await store.invoke("rust_pixels_init", { docId: DOC, layerId, width: SIZE, height: SIZE, bytes: new Uint8Array(SIZE * SIZE * 4) });
  }
  const row = store.layers.get(layerId);
  const rgba = new Uint8Array(SIZE * SIZE * 4);
  for (let i = 3; i < rgba.length; i += 4) rgba[i] = 255;
  await store.invoke("rust_pixels_write_region", { docId: DOC, layerId, x: 0, y: 0, w: SIZE, h: SIZE, rgba });
  return { seeded, wroteBytes: rgba.length, steps: row?.history.length ?? -1 };
}

const priorTauri = (globalThis as Record<string, unknown>)[TAURI_KEY];

beforeAll(async () => {
  const mod = (await getWasmExportModule()) as WasmModule | null;
  expect(mod, "the REAL wasm pkg must load").not.toBeNull();
  wasmMod = mod!;
  (globalThis as Record<string, unknown>)[TAURI_KEY] = { invoke: invokeMock };
  // Installed ONCE for the whole file with a per-test engine holder: the shim is
  // module-sticky and its mirror is fire-and-forget.
  installFacadeCommitShim({
    getEngine: () => liveEngine,
    getDocId: () => liveEngine?.getId() ?? "default",
  });
});

afterAll(() => {
  if (priorTauri === undefined) delete (globalThis as Record<string, unknown>)[TAURI_KEY];
  else (globalThis as Record<string, unknown>)[TAURI_KEY] = priorTauri;
});

beforeEach(() => {
  localStorage.clear();
  toastCalls.length = 0;
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
  liveEngine = null;
});

afterEach(() => {
  void flushExternalTransitions(DOC);
  store.dispose();
  document.body.replaceChildren();
  localStorage.clear();
  restoreCanvas?.();
  restoreCanvas = undefined;
  liveEngine = null;
  __resetNativeAuthorityForTests();
  __resetCanonicalRepushForTests();
  __resetFacadeRegistryForTests();
  vi.restoreAllMocks();
});

describe("adding a layer leaves the user on a paintable, Rust-owned layer", () => {
  it("the added layer carries a raster, is selected, and can take the canonical stroke", async () => {
    const m = mountEditor(DOC);
    try {
      await primeFacade(m.engine, DOC);
      assertArm("on");

      const beforeIds = m.engine.getLayers().map((l) => l.id);
      const beforeActive = m.engine.getActiveLayerId();
      expect(beforeIds.length, "premise: the document starts with a background layer").toBeGreaterThanOrEqual(1);

      await m.addLayer();

      // THE SWALLOWED-THROW DIAGNOSTIC. Everything below asserts engine state, and
      // `handleAddLayer` turns any exception in its own block into a toast rather than a
      // rejection, so a throw here would present as "the raster is missing" with no
      // cause. If this assertion fails, the message IS the cause.
      expect(
        {
          handlerToasts: handlerToasts(),
          allToasts: toastCalls.map((t) => t.message),
          facadeFlag: localStorage.getItem("photrez.facade"),
          authorityFlag: localStorage.getItem("photrez.facadeAuthority"),
          beforeIdsCount: beforeIds.length,
          idsAfter: m.engine.getLayers().map((l) => l.id),
        },
        "handleAddLayer must not have taken its error path; a handler toast means the post-projection block threw and the raster was never set. allToasts is here because startup toasts are expected and are not the handler's",
      ).toMatchObject({ handlerToasts: [], facadeFlag: "1" });

      const afterIds = m.engine.getLayers().map((l) => l.id);
      const added = afterIds.filter((id) => !beforeIds.includes(id));
      expect(added.length, "the routed add produced exactly one new layer").toBe(1);
      const newId = added[0];

      // DEFECT 2 - the raster. Without it `getPaintSurface` returns null and the brush
      // drops to the legacy TypeScript path with no Rust store row at all.
      expect(
        { newId, painted: paintedPixelCount(m.engine, newId), hasBitmap: Boolean(m.engine.getLayer(newId)?.imageBitmap) },
        "the added layer must carry a raster, or getPaintSurface returns null and the stroke escapes the Rust pixel owner",
      ).toEqual({ newId, painted: 0, hasBitmap: true });

      // And it must be TRANSPARENT: the fix must not fabricate visible content. A
      // non-transparent new layer would be a regression, not a fix.
      expect(
        paintedPixelCount(m.engine, newId),
        "the added layer's raster must be transparent - a new layer has no visible content",
      ).toBe(0);

      // The exact gate the tile path turns on (useBrushOverlay.ts:1221).
      const surface = (m.engine as unknown as { getPaintSurface?: (id: string) => unknown }).getPaintSurface?.(newId) ?? null;
      expect(
        { newId, surfacePresent: surface !== null },
        "getPaintSurface must return a surface for the added layer, or commitBrushStroke takes the legacy fallback",
      ).toEqual({ newId, surfacePresent: true });

      // The graph must hold the added layer, or strokeBlockedByRust refuses the stroke.
      //
      // The contract is NOT "true": `rustHoldsLayer` returns `true | false | null`
      // (document.ts:297-310) and `null` means UNKNOWN - no wasm engine bound, facade
      // off, or an unreadable mirror - which every caller must treat as "do not block",
      // never as "absent" (document.ts:283-289). So `false` is the ONLY value that
      // blocks the stroke, and that is what is forbidden here. Asserting `true` would
      // be asserting that this harness happens to have a bound mirror, which is an
      // environment fact rather than the contract, and it would fail for a reason that
      // has nothing to do with the fix.
      const rustHolds = (
        m.engine as unknown as { rustHoldsLayer?: (id: string) => boolean | null }
      ).rustHoldsLayer?.(newId);
      expect(
        { newId, rustHolds, strokeBlocked: rustHolds === false },
        "the added layer must never read as absent from the Rust graph: false blocks the stroke outright, null is unknown-and-paints",
      ).toMatchObject({ newId, strokeBlocked: false });

      // DEFECT 1 - the selection.
      expect(
        { engineActive: m.engine.getActiveLayerId(), editorSelected: m.selectedLayerId(), newId },
        "both arms select the layer they just added; the user must not be left painting the previous layer",
      ).toEqual({ engineActive: newId, editorSelected: newId, newId });

      // The store-level proof: the first canonical stroke seeds and writes.
      const stroke = await firstCanonicalStroke(newId);
      expect(
        stroke,
        "the first stroke on the added layer must seed the Rust store and take the one canonical write",
      ).toEqual({ seeded: true, wroteBytes: SIZE * SIZE * 4, steps: 1 });
    } finally {
      m.dispose();
    }
  });

  it("the flag-off arm is unchanged: it still selects through engine.addLayer", async () => {
    // "0", NOT removeItem. `isFacadeEnabled` is `getItem(...) !== "0"` (bridge.ts:78-85),
    // so removing the key leaves null and ENABLES the facade - this case would run the
    // facade arm and its result would only ever track the facade fix.
    localStorage.setItem("photrez.facade", "0");
    const m = mountEditor(DOC);
    try {
      assertArm("off");
      const beforeIds = m.engine.getLayers().map((l) => l.id);
      await m.addLayer();
      // The handler's own error path must not have run here either.
      expect(
        { handlerToasts: handlerToasts(), allToasts: toastCalls.map((t) => t.message) },
        "the flag-off arm must not have thrown",
      ).toMatchObject({ handlerToasts: [] });
      const afterIds = m.engine.getLayers().map((l) => l.id);
      const added = afterIds.filter((id) => !beforeIds.includes(id));
      expect(added.length, "the flag-off arm still adds exactly one layer").toBe(1);
      expect(
        { engineActive: m.engine.getActiveLayerId(), newId: added[0] },
        "the flag-off arm selected the new layer before this change and must still",
      ).toEqual({ engineActive: added[0], newId: added[0] });
    } finally {
      m.dispose();
    }
  });
});
