/**
 * Routing matrix for the TILE/PIXEL undo/redo cursor-sync step.
 *
 * Two production sites can fire rust_pixels_undo / rust_pixels_redo for ONE
 * step (both inside the tile-patch branch of useEditorCommands):
 *   - the photrez.rustPixels tile path: reads the flag with no runtime gate,
 *   - the photrez.historyBridge cursor-sync site: needs historyBridgeEnabled(),
 *     i.e. the gate key AND the Tauri runtime, and it is skipped while the flag
 *     is on so a step can never take two Rust cursor steps.
 * Exactly one site may fire per direction, so all eight
 * flag x gate x runtime combinations are pinned here:
 *
 *   flag 1 (any gate, any runtime) -> 1: the tile path fires and the bridge
 *     site stays skipped.
 *   flag 0 + gate 1 + Tauri -> 1: only the bridge site owns the cursor step.
 *   flag 0 + gate 1 + jsdom -> 0: the gate predicate also needs the runtime.
 *   flag 0 + gate 0 (any runtime) -> 0: no site is armed.
 *
 * A metadata-only step carries no tile patches, so both sites are unreachable
 * for it; those rows assert zero invokes AND that the model restore still ran
 * (otherwise a zero would only prove the step aborted early).
 *
 * Census reads go through flushPixelInvokeCensus(), which awaits in-flight
 * invokes first; a sleep is not a drain.
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render } from "solid-js/web";
import { ImageData as NodeImageData } from "canvas";
import { invoke } from "@tauri-apps/api/core";
import { flushPixelInvokeCensus, getPixelCensusSnapshot } from "@/lib/protocol/pixelInvokeCensus";
import { getPixelHistoryDepth } from "@/lib/protocol/pixelHistoryDepth";
import { invokePixelCommand } from "@/lib/protocol/bridge";
import { isTauriRuntime } from "@/lib/desktop/tauriWindow";
import { WorkspaceManager } from "@/engine/workspace";
import { useEditorCommands } from "../useEditorCommands";
import { EditorProvider, useEditor } from "../shell/EditorContext";
import { CanvasViewport } from "../canvas/CanvasViewport";
import { c4PendingCommits, flushC4Commits } from "../useBrushOverlay";
import { mockUseEditor } from "@/__tests__/mockUseEditor";
import * as DialogProviderModule from "../dialogs/DialogProvider";

// The runtime detector feeds historyBridgeEnabled() and the undo/redo gate.
vi.mock("@/lib/desktop/tauriWindow", () => ({
  isTauriRuntime: vi.fn(),
}));

// The undo/redo tile path dynamic-imports this module, so the mock intercepts
// the dynamic import too and `invoke` is the shared spy.
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}));
vi.mock("@tauri-apps/api/app", () => ({
  getVersion: vi.fn(() => Promise.resolve("0.0.0")),
}));

// The undo path checks hasFacadeOwnedLayers() (Rust-owned history). Default off.
// Real exports stay reachable: the mounted viewport below needs the actual
// DocumentEngine, while these two predicates stay stubbed for the matrix.
vi.mock("@/engine/document", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/engine/document")>();
  return {
    ...actual,
    hasFacadeOwnedLayers: vi.fn(() => false),
    isFacadeOwnedLayer: vi.fn(() => false),
  };
});

// -- Viewport boundaries the mounted CanvasViewport needs (allowed mocks: the
//    render/viewport layer only). useBrushOverlay, useCanvasPointerTools and
//    the region producer stay REAL so the pointer chain below is a real one.
vi.mock("../canvas/useViewportRenderer", () => ({
  useViewportRenderer: () => ({
    isFitTransition: () => false,
    fitToScreenAndRender: vi.fn(),
    resizeRenderer: vi.fn(),
  }),
}));
vi.mock("../canvas/usePanNavigation", () => ({
  usePanNavigation: () => ({
    isSpacePressed: () => false,
    setIsSpacePressed: vi.fn(),
    isPanning: () => false,
    setIsPanning: vi.fn(),
    stopMomentum: vi.fn(),
    handleWheel: vi.fn(),
    onViewportPointerDown: vi.fn(),
    onViewportPointerMove: vi.fn(),
    onViewportPointerUp: vi.fn(),
    onViewportPointerCancel: vi.fn(),
    onViewportLostPointerCapture: vi.fn(),
  }),
}));
vi.mock("../canvas/useCanvasDerivedState", () => ({
  useCanvasDerivedState: () => ({ cropSnapTargets: () => [] }),
}));
vi.mock("../canvas/useCanvasKeyboard", () => ({
  useCanvasKeyboard: vi.fn(),
}));

// -- jsdom shims (jsdom has no ImageData / OffscreenCanvas / createImageBitmap;
//    node-canvas backs <canvas>.getContext("2d")). FakeImageData extends the
//    node-canvas class because its 2D context rejects foreign ImageData
//    instances, and widens the constructor to any ArrayLike<number> - which is
//    how Rust tile payloads arrive.
class FakeImageData extends NodeImageData {
  constructor(data: ArrayLike<number> | number, width?: number, height?: number) {
    if (typeof data === "number") {
      super(data, width as number);
    } else {
      const bytes = data instanceof Uint8ClampedArray ? data : Uint8ClampedArray.from(data);
      super(bytes as Uint8ClampedArray<ArrayBuffer>, width as number, height as number);
    }
  }
}
if (typeof (globalThis as { ImageData?: unknown }).ImageData === "undefined") {
  (globalThis as { ImageData?: unknown }).ImageData = FakeImageData;
}
if (typeof (globalThis as { OffscreenCanvas?: unknown }).OffscreenCanvas === "undefined") {
  const stubOffscreen = function OffscreenCanvas(w: number, h: number) {
    const el = document.createElement("canvas");
    el.width = w;
    el.height = h;
    (el as HTMLCanvasElement & { transferToImageBitmap: () => ImageBitmap; close: () => void }).transferToImageBitmap =
      () => el as unknown as ImageBitmap;
    (el as HTMLCanvasElement & { transferToImageBitmap: () => ImageBitmap; close: () => void }).close = () => {};
    return el;
  };
  (globalThis as { OffscreenCanvas?: unknown }).OffscreenCanvas = stubOffscreen;
}
if (typeof (globalThis as { createImageBitmap?: unknown }).createImageBitmap === "undefined") {
  (globalThis as { createImageBitmap?: unknown }).createImageBitmap = async (source: unknown) => source;
}

const GATE_KEY = "photrez.historyBridge";
const RUST_PIXELS_KEY = "photrez.rustPixels";

type WirePatch = {
  layerId: string;
  surfaceWidth: number;
  surfaceHeight: number;
  before: { x: number; y: number; w: number; h: number; data: number[] }[];
  after: { x: number; y: number; w: number; h: number; data: number[] }[];
};

const makePatches = (): WirePatch => ({
  layerId: "l1",
  surfaceWidth: 10,
  surfaceHeight: 10,
  before: [],
  after: [],
});

/** Minimal TS CommandHistory shape; `patches` null = metadata-only step. */
function makeHistory(patches: WirePatch | null) {
  const snapshot = { layers: [], activeLayerId: null };
  return {
    canUndo: () => true,
    canRedo: () => true,
    undo: () => snapshot,
    redo: () => snapshot,
    consumeLastUndoPatches: () => patches,
    consumeLastRedoPatches: () => patches,
  };
}

/** Editor context + engine the hook body and the metadata path consume. */
function makeEditorContext(
  history: ReturnType<typeof makeHistory>,
  engine: Record<string, unknown>,
) {
  return {
    workspace: {
      getActiveEngine: () => engine,
      getActiveHistory: () => history,
      getActiveDocumentId: () => "doc-1",
      notifyVisualChange: vi.fn(),
    },
    renderer: { uploadImage: vi.fn(), uploadSurfaceTiles: vi.fn() },
    scheduler: { requestRender: vi.fn() },
    activeDocumentId: () => "doc-1",
    layerTransformSession: () => null,
    setLayerTransformSession: vi.fn(),
    activeTool: () => "brush",
    cropInteractionMode: () => "modern",
    canCropUndo: () => false,
    canCropRedo: () => false,
    canModernCropUndo: () => false,
    canModernCropRedo: () => false,
    layers: () => [],
    activeLayerId: () => null,
    selectedLayerIds: () => [],
    setSelectedLayerIds: vi.fn(),
    toggleLayerSelection: vi.fn(),
    rangeSelectLayers: vi.fn(),
    selectedLayerId: () => null,
    setSelectedLayerId: vi.fn(),
    textEditSession: () => null,
    setTextEditSession: vi.fn(),
    setStatusLoadingMessage: vi.fn(),
    setShowExportDialog: vi.fn(),
    setShowPrintDialog: vi.fn(),
    setShowResizeDialog: vi.fn(),
  };
}

function makeEngine() {
  const layer = { id: "l1", basicAdjustment: undefined, imageBitmap: undefined };
  return {
    getActiveLayerId: () => "l1",
    getLayer: () => layer,
    getLayers: () => [] as { id: string }[],
    restore: vi.fn(),
    snapshot: () => ({ layers: [], activeLayerId: null }),
    getPaintSurface: () => null,
  };
}

// Two turns: the cursor-sync site does its own dynamic import after the first.
const flush = async () => {
  await new Promise<void>((resolve) => setTimeout(resolve, 0));
  await new Promise<void>((resolve) => setTimeout(resolve, 0));
};

function countInvokes(...commands: string[]): number {
  return (vi.mocked(invoke).mock.calls as [string][]).filter(([cmd]) =>
    commands.includes(cmd),
  ).length;
}

type Row = {
  rustPixels: "0" | "1";
  bridge: "0" | "1";
  tauri: boolean;
  expected: number;
};

const ROWS: Row[] = [
  { rustPixels: "1", bridge: "0", tauri: true, expected: 1 },
  { rustPixels: "1", bridge: "1", tauri: true, expected: 1 },
  { rustPixels: "1", bridge: "0", tauri: false, expected: 1 },
  { rustPixels: "1", bridge: "1", tauri: false, expected: 1 },
  { rustPixels: "0", bridge: "1", tauri: true, expected: 1 },
  { rustPixels: "0", bridge: "1", tauri: false, expected: 0 },
  { rustPixels: "0", bridge: "0", tauri: true, expected: 0 },
  { rustPixels: "0", bridge: "0", tauri: false, expected: 0 },
];

function applyRowFlags(flags: { rustPixels: "0" | "1"; bridge: "0" | "1" }): void {
  localStorage.setItem(RUST_PIXELS_KEY, flags.rustPixels);
  localStorage.setItem(GATE_KEY, flags.bridge);
}

describe("TILE/PIXEL undo/redo routing matrix (flag x bridge gate x runtime)", () => {
  beforeEach(() => {
    vi.spyOn(DialogProviderModule, "useDialog").mockReturnValue(
      {} as unknown as ReturnType<typeof DialogProviderModule.useDialog>,
    );
    vi.mocked(invoke).mockReset();
    vi.mocked(isTauriRuntime).mockReset();
    localStorage.clear();
  });

  afterEach(() => {
    vi.restoreAllMocks();
    localStorage.clear();
  });

  for (const row of ROWS) {
    const label =
      `rustPixels=${row.rustPixels} historyBridge=${row.bridge} ` +
      `runtime=${row.tauri ? "tauri" : "jsdom"}`;

    it(`${label}: exactly ${row.expected} cursor-sync invoke per direction`, async () => {
      applyRowFlags(row);
      vi.mocked(isTauriRuntime).mockReturnValue(row.tauri);
      const engine = makeEngine();
      mockUseEditor(makeEditorContext(makeHistory(makePatches()), engine));
      vi.mocked(invoke).mockResolvedValue({ tiles: [], epoch: 1, version: 1 });
      const censusBefore = (await flushPixelInvokeCensus()).entries.length;

      const commands = useEditorCommands(() => {});
      commands.undo();
      await flush();
      expect(countInvokes("rust_pixels_undo"), "undo fires only its own command").toBe(
        row.expected,
      );
      expect(countInvokes("rust_pixels_redo"), "undo must not fire redo").toBe(0);

      vi.mocked(invoke).mockClear();
      commands.redo();
      await flush();
      expect(countInvokes("rust_pixels_redo"), "redo fires only its own command").toBe(
        row.expected,
      );
      expect(countInvokes("rust_pixels_undo"), "redo must not fire undo").toBe(0);

      const census = await flushPixelInvokeCensus();
      const delta = census.entries
        .slice(censusBefore)
        .filter(
          (entry) =>
            entry.command === "rust_pixels_undo" || entry.command === "rust_pixels_redo",
        );
      expect(delta.map((entry) => entry.command)).toEqual(
        row.expected === 1 ? ["rust_pixels_undo", "rust_pixels_redo"] : [],
      );
      expect(delta.every((entry) => entry.phase === "resolved")).toBe(true);
    });
  }
});

describe("metadata-only steps fire no cursor-sync invoke from either site", () => {
  beforeEach(() => {
    vi.spyOn(DialogProviderModule, "useDialog").mockReturnValue(
      {} as unknown as ReturnType<typeof DialogProviderModule.useDialog>,
    );
    vi.mocked(invoke).mockReset();
    vi.mocked(isTauriRuntime).mockReset();
    localStorage.clear();
  });

  afterEach(() => {
    vi.restoreAllMocks();
    localStorage.clear();
  });

  const combos = [
    { rustPixels: "1", bridge: "1", tauri: true },
    { rustPixels: "0", bridge: "1", tauri: true },
    { rustPixels: "1", bridge: "0", tauri: false },
  ] as const;

  for (const combo of combos) {
    const label =
      `rustPixels=${combo.rustPixels} historyBridge=${combo.bridge} ` +
      `runtime=${combo.tauri ? "tauri" : "jsdom"}`;

    it(`${label}: zero invokes while the model restore still runs`, async () => {
      applyRowFlags(combo);
      vi.mocked(isTauriRuntime).mockReturnValue(combo.tauri);
      const engine = makeEngine();
      mockUseEditor(makeEditorContext(makeHistory(null), engine));
      vi.mocked(invoke).mockResolvedValue({ tiles: [], epoch: 1, version: 1 });
      const censusBefore = (await flushPixelInvokeCensus()).entries.length;

      const commands = useEditorCommands(() => {});
      commands.undo();
      await flush();
      commands.redo();
      await flush();

      // Non-vacuity: the step completed its model restore, so the zero below
      // means "no site fired", not "the step aborted before either site".
      expect(engine.restore).toHaveBeenCalledTimes(2);
      expect(countInvokes("rust_pixels_undo", "rust_pixels_redo")).toBe(0);

      const census = await flushPixelInvokeCensus();
      const delta = census.entries
        .slice(censusBefore)
        .filter(
          (entry) =>
            entry.command === "rust_pixels_undo" || entry.command === "rust_pixels_redo",
        );
      expect(delta).toEqual([]);
    });
  }
});

// -- Flip-ordering transition gates (both directions) ----------------------
// The two describes above set every flag BEFORE the action and never flip one
// mid-flight, so they cannot express the two directions the pixel flag needs:
//  - ON-to-OFF: a brush commit already waiting in the per-document queue must
//    cancel when photrez.rustPixels flips to OFF, so no state-changing pixel
//    invoke lands after the flip marker;
//  - OFF-to-ON: the legacy arm must enqueue nothing while OFF, and the next
//    pointer chain after the flip must produce the Rust region write.
// Both gates read their verdict from the six-command census (one monotonic
// order per state-changing pixel command) split at the marker, plus the store
// emulator below, which counts raw IPC writes independently of the census.

const SIX_COMMAND_ALLOWLIST: readonly string[] = [
  "rust_pixels_write_region",
  "rust_pixels_record_external",
  "rust_pixels_undo",
  "rust_pixels_redo",
  "apply_tile_patch",
  "rust_pixels_record_snapshot",
];

const FLIP_DOC = 256;

type StoredWrite = { x: number; y: number; w: number; h: number; rgba: Uint8Array };

/**
 * In-memory Rust pixel store behind the mocked `invoke`. It answers the
 * commands the brush commit issues AND counts every rust_pixels_write_region
 * that reaches the IPC boundary, so "no late invoke" has an oracle that does
 * not depend on the census recorder itself.
 */
function createPixelStore(width: number, height: number) {
  const buffer = new Uint8ClampedArray(width * height * 4);
  const writes: StoredWrite[] = [];
  const rejections: string[] = [];
  // Per-document bookkeeping, mirroring the real store: the unified pixel
  // history stream lives on the document (crates/core/src/pixel_store.rs:715-734)
  // and SURVIVES a layer removal, which drops storage only
  // (crates/core/src/pixel_store.rs:197-200). That asymmetry is what the
  // oracle counterexample test below reads.
  const docs = new Map<string, { history: string[]; removedLayers: Set<string> }>();
  const docOf = (id: string) => {
    const hit = docs.get(id);
    if (hit) return hit;
    const fresh = { history: [] as string[], removedLayers: new Set<string>() };
    docs.set(id, fresh);
    return fresh;
  };
  let epoch = 0;

  const readTile = (x: number, y: number, w: number, h: number) => {
    const data: number[] = [];
    for (let row = 0; row < h; row++) {
      const src = ((y + row) * width + x) * 4;
      for (let i = 0; i < w * 4; i++) data.push(buffer[src + i]);
    }
    return { x, y, w, h, data };
  };

  vi.mocked(invoke).mockImplementation(async (command: string, args?: unknown) => {
    const a = (args ?? {}) as Record<string, unknown>;
    if (command === "rust_pixels_open_document") {
      docOf(String(a.docId));
      return null;
    }
    if (command === "rust_pixels_get_epoch") {
      // A removed layer rejects with the bare string Tauri surfaces
      // (paint_parity_cmds.rs:310-316 -> pixel_store.rs:396-400). Layers the
      // harness never modeled keep the resolve path: the two flip gates seed
      // the store from the TS bitmap without an init call, and changing that
      // answer would move their rehydrate branch.
      if (docs.get(String(a.docId))?.removedLayers.has(String(a.layerId))) {
        return Promise.reject(`layer not initialized: ${String(a.layerId)}`);
      }
      return epoch;
    }
    if (command === "rust_pixels_remove_layer") {
      docOf(String(a.docId)).removedLayers.add(String(a.layerId));
      return null;
    }
    if (command === "rust_pixels_history_depth") {
      const doc = docs.get(String(a.docId));
      if (!doc) return Promise.reject(`document not open: ${String(a.docId)}`);
      return {
        total_depth: doc.history.length,
        undo_depth: doc.history.length,
        redo_depth: 0,
        affected_layer_ids: [...new Set(doc.history)].sort(),
      };
    }
    if (command === "protocol_apply_command_native") {
      // Mirrors protocol_native_cmds.rs:43-61: the doc must already be open,
      // and the envelope lands in the SAME unified history stream. This
      // command is deliberately NOT one of the six census'd ones
      // (pixelInvokeCensus.ts:30-37), which is the whole point below: history
      // can grow while the census reads zero.
      const storeDoc = docs.get(String(a.docId));
      if (!storeDoc) return Promise.reject(`document not open: ${String(a.docId)}`);
      let envelope: { command?: Record<string, unknown> };
      try {
        envelope = JSON.parse(String(a.envelopeJson)) as { command?: Record<string, unknown> };
      } catch (err) {
        return Promise.reject(`E_ENVELOPE_PARSE: ${String(err)}`);
      }
      // `Command` is internally tagged (crates/core/src/command.rs:58-60), so
      // the affected layer id rides at `command.id` - the same id the real
      // apply records on the history entry (document_core.rs begin_forward).
      const body = envelope.command;
      storeDoc.history.push(typeof body?.id === "string" ? body.id : "");
      return JSON.stringify({ documentVersion: storeDoc.history.length, delta: {} });
    }
    if (command === "rust_pixels_init") {
      buffer.set((a.bytes as Uint8Array).subarray(0, buffer.length));
      return null;
    }
    if (command === "rust_pixels_snapshot_layer") return [readTile(0, 0, width, height)];
    if (command === "rust_pixels_write_region") {
      const x = a.x as number;
      const y = a.y as number;
      const w = a.w as number;
      const h = a.h as number;
      const rgba = a.rgba as Uint8Array;
      if (!Number.isInteger(x) || !Number.isInteger(y) || !Number.isInteger(w) || !Number.isInteger(h) || w <= 0 || h <= 0) {
        rejections.push(`invalid dimensions ${x},${y},${w},${h}`);
        return Promise.reject("Invalid region dimensions");
      }
      if (x < 0 || y < 0 || x + w > width || y + h > height) {
        rejections.push(`region out of bounds ${x},${y},${w},${h}`);
        return Promise.reject("Region outside layer bounds");
      }
      if (rgba.length !== w * h * 4) {
        rejections.push(`length mismatch ${rgba.length} != ${w * h * 4}`);
        return Promise.reject("Invalid region length");
      }
      const storeDoc = docOf(String(a.docId));
      if (storeDoc.removedLayers.has(String(a.layerId))) {
        rejections.push(`layer not initialized: ${String(a.layerId)}`);
        return Promise.reject("layer not initialized or region out of bounds");
      }
      const before = readTile(x, y, w, h);
      for (let row = 0; row < h; row++) {
        const dst = ((y + row) * width + x) * 4;
        buffer.set(rgba.subarray(row * w, (row + 1) * w), dst);
      }
      const after = readTile(x, y, w, h);
      writes.push({ x, y, w, h, rgba });
      // One write_region opens one canonical Pixel history entry
      // (paint_parity_cmds.rs:318-322).
      storeDoc.history.push(String(a.layerId));
      epoch += 1;
      return { before: [before], after: [after], epoch, version: epoch };
    }
    // Every other allowlisted command (record_external, apply_tile_patch,
    // record_snapshot, undo, redo) only has to resolve so its census entry
    // reaches a terminal phase.
    return { version: epoch };
  });

  return {
    writes,
    rejections,
    seed: (pixels: Uint8ClampedArray) => buffer.set(pixels.subarray(0, buffer.length)),
    epoch: () => epoch,
  };
}

let flipSetTool: (tool: string) => void = () => {};
let flipSetFgColor: (color: string) => void = () => {};
let flipSetZoom: (zoom: number) => void = () => {};
let flipSetPan: (pan: { x: number; y: number }) => void = () => {};

const FlipConsumer = () => {
  const editor = useEditor();
  flipSetTool = editor.setActiveTool;
  flipSetFgColor = editor.setFgColor;
  flipSetZoom = editor.setZoom;
  flipSetPan = editor.setPan;
  return null;
};

let flipStore: ReturnType<typeof createPixelStore>;
let flipContainer: HTMLDivElement;
let flipDispose: (() => void) | undefined;
let flipRect: ReturnType<typeof vi.spyOn> | undefined;

function mountFlipViewport() {
  const container = document.createElement("div");
  document.body.appendChild(container);
  const workspace = new WorkspaceManager();
  // Partial renderer/scheduler doubles, typed permissively the same way
  // CanvasViewport.test.tsx does: EditorProvider's props require the full
  // backend surface, which these gates never touch.
  const renderer: Record<string, unknown> = {
    uploadImage: vi.fn(),
    destroyTexture: vi.fn(),
    uploadSurfaceTiles: vi.fn(),
  };
  const scheduler: Record<string, unknown> = { requestRender: vi.fn() };
  const session = WorkspaceManager.createBlankDocument("doc-flip", "Flip", FLIP_DOC, FLIP_DOC);
  workspace.addDocument(session);
  flipDispose = render(
    () => (
      <EditorProvider
        workspace={workspace}
        renderer={renderer as never}
        scheduler={scheduler as never}
      >
        <FlipConsumer />
        <CanvasViewport />
      </EditorProvider>
    ),
    container,
  );
  flipContainer = container;
  return { session, container };
}

function makeFlipCanvas(fill: string): HTMLCanvasElement {
  const canvas = document.createElement("canvas");
  canvas.width = FLIP_DOC;
  canvas.height = FLIP_DOC;
  const ctx = canvas.getContext("2d") as CanvasRenderingContext2D;
  ctx.fillStyle = fill;
  ctx.fillRect(0, 0, FLIP_DOC, FLIP_DOC);
  return canvas;
}

/** Paint layer + matching store seed, then install the cached paint surface. */
function prepareFlipLayer(session: ReturnType<typeof WorkspaceManager.createBlankDocument>) {
  const layerId = session.engine.getLayers()[0].id;
  const bitmap = makeFlipCanvas("#ffffff");
  session.engine.setLayerImageBitmap(layerId, bitmap as unknown as ImageBitmap);
  session.engine.setActiveLayer(layerId);
  const pixels = (bitmap.getContext("2d") as CanvasRenderingContext2D).getImageData(
    0,
    0,
    FLIP_DOC,
    FLIP_DOC,
  ).data;
  flipStore.seed(pixels);
  const surface = session.engine.getPaintSurface(layerId);
  if (!surface) throw new Error("paint surface not created (layer bitmap missing?)");
  return { layerId, surface };
}

function flipCanvas(): HTMLCanvasElement {
  const c = flipContainer.querySelector("canvas:not([data-overlay-canvas])") as HTMLCanvasElement | null;
  if (!c) throw new Error("viewport canvas not found");
  return c;
}

function fireFlip(type: string, el: Element, clientX: number, clientY: number, pointerId = 11) {
  el.dispatchEvent(
    new PointerEvent(type, {
      bubbles: true,
      cancelable: true,
      button: 0,
      pointerId,
      clientX,
      clientY,
    }),
  );
}

async function tick(ms = 0) {
  await new Promise((resolve) => setTimeout(resolve, ms));
}

/** One real brush pointer chain: down -> move -> up, ending in a c4 enqueue. */
function brushChain(el: Element, from: [number, number], to: [number, number]) {
  fireFlip("pointerdown", el, from[0], from[1]);
  fireFlip("pointermove", el, to[0], to[1]);
  fireFlip("pointerup", el, to[0], to[1]);
}

function setupFlipHarness() {
  vi.mocked(invoke).mockReset();
  vi.mocked(isTauriRuntime).mockReturnValue(true);
  // History bridge armed: whatever history fires shows up in the census, so a
  // zero count can never hide behind a gate that was closed the whole time.
  localStorage.clear();
  localStorage.setItem(GATE_KEY, "1");
  localStorage.removeItem("photrez.tileCommit");
  localStorage.removeItem("photrez.canonicalCommit");
  flipStore = createPixelStore(FLIP_DOC, FLIP_DOC);
  flipRect = vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
    left: 0, top: 0, right: FLIP_DOC, bottom: FLIP_DOC, width: FLIP_DOC, height: FLIP_DOC,
    x: 0, y: 0, toJSON: () => ({}),
  } as DOMRect);
  Element.prototype.setPointerCapture = vi.fn();
  Element.prototype.releasePointerCapture = vi.fn();
}

function teardownFlipHarness() {
  flipDispose?.();
  flipDispose = undefined;
  flipContainer?.parentNode?.removeChild(flipContainer);
  flipRect?.mockRestore();
  flipRect = undefined;
  localStorage.clear();
}

describe("ON-to-OFF flip gate (queued brush commit cancels before any pixel invoke)", () => {
  let cancelWarns: ReturnType<typeof vi.spyOn> | null = null;

  beforeEach(() => {
    setupFlipHarness();
    cancelWarns = vi.spyOn(console, "warn").mockImplementation(() => {});
  });
  afterEach(() => {
    cancelWarns?.mockRestore();
    cancelWarns = null;
    teardownFlipHarness();
  });

  it("arm guard sees the queued commit, the queue drains, and nothing fires after the marker", async () => {
    localStorage.setItem(RUST_PIXELS_KEY, "1");
    const { session } = mountFlipViewport();
    await tick();
    flipSetZoom(1);
    flipSetPan({ x: 0, y: 0 });
    flipSetTool("brush");
    flipSetFgColor("#ff0000");
    prepareFlipLayer(session);
    const canvas = flipCanvas();

    // Census entries are file-global: pin this test's own starting order so
    // every count below only sees what THIS chain recorded.
    const drainedBaseline = await flushPixelInvokeCensus();
    const startOrder = drainedBaseline.entries.length
      ? drainedBaseline.entries[drainedBaseline.entries.length - 1].order
      : 0;

    // Stroke 1 runs to completion, so a resolved state-changing entry exists
    // BEFORE the marker - a zero late count would otherwise be vacuous.
    brushChain(canvas, [40, 40], [70, 60]);
    await flushC4Commits();
    await tick();
    const drained = await flushPixelInvokeCensus();
    expect(
      drained.entries.filter(
        (e) => e.order > startOrder && e.command === "rust_pixels_write_region" && e.phase === "resolved",
      ).length,
      "stroke 1 wrote one region before the flip",
    ).toBeGreaterThanOrEqual(1);
    expect(flipStore.writes.length).toBe(1);

    // Stroke 2 enqueues and must still be waiting when the flip lands: from
    // pointerup to the marker there is no await, so the commit has not had a
    // single microtask yet.
    brushChain(canvas, [120, 120], [170, 160]);
    expect(c4PendingCommits(), "arm guard: a commit is queued before the marker").toBeGreaterThanOrEqual(1);

    // Sync census read is safe here only because nothing is in flight.
    const markerSnapshot = getPixelCensusSnapshot();
    expect(markerSnapshot.pending, "marker read needs zero in-flight invokes").toBe(0);
    const markerOrder = markerSnapshot.entries.length
      ? markerSnapshot.entries[markerSnapshot.entries.length - 1].order
      : 0;

    // THE FLIP: kill switch to OFF while stroke 2 waits for the queue.
    localStorage.setItem(RUST_PIXELS_KEY, "0");

    await flushC4Commits();
    expect(c4PendingCommits(), "the queue drained - otherwise zero late invokes proves nothing").toBe(0);
    const census = await flushPixelInvokeCensus();
    expect(census.pending).toBe(0);

    expect(
      census.entries.every((e) => SIX_COMMAND_ALLOWLIST.includes(e.command)),
      "census stays on the six-command allowlist",
    ).toBe(true);
    const orders = census.entries.map((e) => e.order);
    expect(new Set(orders).size, "orders are unique").toBe(orders.length);
    expect([...orders].sort((a, b) => a - b), "orders are monotonic").toEqual(orders);

    const before = census.entries.filter((e) => e.order > startOrder && e.order <= markerOrder);
    const late = census.entries.filter((e) => e.order > markerOrder);
    expect(
      before.some((e) => e.command === "rust_pixels_write_region" && e.phase === "resolved"),
      "a state-changing entry sits before the marker",
    ).toBe(true);
    expect(late.length, "lateInvokeCount after the flip marker").toBe(0);
    // Independent oracle: the store counts raw IPC writes, census or not.
    expect(flipStore.writes.length, "only stroke 1 reached rust_pixels_write_region").toBe(1);
    expect(flipStore.rejections).toEqual([]);

    // The cancel must be audible: a silently swallowed commit leaves the same
    // visible outcome as a queue that never held one.
    expect(
      cancelWarns?.mock.calls.some((c: unknown[]) => String(c[0]).includes("queued brush commit cancelled")),
      "the ON-to-OFF cancel path warned",
    ).toBe(true);
  });
});

describe("OFF-to-ON flip gate (legacy arm writes nothing, the flip arms the next chain)", () => {
  beforeEach(setupFlipHarness);
  afterEach(teardownFlipHarness);

  it("zero enqueues and zero writes while OFF, then post-flip write_region and no apply_tile_patch", async () => {
    localStorage.setItem(RUST_PIXELS_KEY, "0");
    const { session } = mountFlipViewport();
    await tick();
    flipSetZoom(1);
    flipSetPan({ x: 0, y: 0 });
    flipSetTool("brush");
    flipSetFgColor("#ff0000");
    prepareFlipLayer(session);
    const canvas = flipCanvas();

    // Census entries are file-global: pin this test's own starting order so
    // every count below only sees what THIS chain recorded.
    const baseline = await flushPixelInvokeCensus();
    const startOrder = baseline.entries.length
      ? baseline.entries[baseline.entries.length - 1].order
      : 0;

    brushChain(canvas, [40, 40], [70, 60]);
    expect(c4PendingCommits(), "flag OFF enqueues no deferred commit").toBe(0);
    await flushC4Commits();
    await tick();
    const pre = await flushPixelInvokeCensus();
    expect(
      pre.entries.filter((e) => e.order > startOrder && e.command === "rust_pixels_write_region").length,
      "preFlipWriteRegionCount",
    ).toBe(0);
    expect(flipStore.writes.length, "the legacy arm never reached the pixel store").toBe(0);

    // Census-alive proof, labeled harness-driven: the test itself drives one
    // bridge write, so every zero above and below cannot come from a recorder
    // that never records. It also pins that apply_tile_patch IS recordable in
    // this test, which is what makes the post-flip zero meaningful.
    await invokePixelCommand("apply_tile_patch", {
      docId: "doc-flip",
      layerId: "l1",
      before: [],
      after: [],
    });
    const alive = await flushPixelInvokeCensus();
    const last = alive.entries[alive.entries.length - 1];
    expect(last, "harness-driven bridge write is recorded").toMatchObject({
      command: "apply_tile_patch",
      phase: "resolved",
    });
    expect(alive.entries.every((e) => SIX_COMMAND_ALLOWLIST.includes(e.command))).toBe(true);
    const markerOrder = last.order;

    // THE FLIP: kill switch to ON.
    localStorage.setItem(RUST_PIXELS_KEY, "1");

    brushChain(canvas, [180, 180], [220, 210]);
    await flushC4Commits();
    expect(c4PendingCommits(), "the post-flip commit drained").toBe(0);
    const post = await flushPixelInvokeCensus();
    const postEntries = post.entries.filter((e) => e.order > markerOrder);
    expect(postEntries.every((e) => SIX_COMMAND_ALLOWLIST.includes(e.command))).toBe(true);
    expect(
      postEntries.filter((e) => e.command === "rust_pixels_write_region").length,
      "postFlipWriteRegionCount",
    ).toBeGreaterThanOrEqual(1);
    expect(
      postEntries.filter((e) => e.command === "apply_tile_patch").length,
      "postFlipApplyTilePatchCount",
    ).toBe(0);
    expect(flipStore.writes.length, "exactly the post-flip stroke reached the store").toBe(1);
    expect(flipStore.rejections).toEqual([]);
  });
});

describe("oracle counterexample: removed-layer history the six-command census never counts", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    vi.mocked(isTauriRuntime).mockReset();
    localStorage.clear();
  });
  afterEach(() => {
    localStorage.clear();
  });

  it("keeps the enablement event blocked: epoch rejects, depth reads 1 twice, census window reads 0", async () => {
    createPixelStore(FLIP_DOC, FLIP_DOC);
    const docId = "doc-oracle";
    const layerId = "layer-oracle";

    await invokePixelCommand("rust_pixels_open_document", { docId });

    // Census entries are file-global: pin this test's own starting order so
    // the zero below only sees what THIS window recorded.
    const baseline = await flushPixelInvokeCensus();
    const startOrder = baseline.entries.length
      ? baseline.entries[baseline.entries.length - 1].order
      : 0;

    // Seed one metadata history entry through the NON-census native apply -
    // the exact "census 0 + history PRESENT" counterexample shape
    // (pixelInvokeCensus.ts counts exactly six commands and
    // protocol_apply_command_native is not one of them).
    const seeded = await invokePixelCommand("protocol_apply_command_native", {
      docId,
      envelopeJson: JSON.stringify({
        contractVersion: 1,
        command: { type: "setBackgroundFlag", id: layerId },
      }),
    });
    expect(seeded, "the native apply resolved").toBeTruthy();

    await invokePixelCommand("rust_pixels_remove_layer", { docId, layerId });

    // The epoch probe rejects the removed layer with the bare string the Rust
    // command surfaces (paint_parity_cmds.rs:310-316 -> pixel_store.rs:396-400).
    await expect(
      invokePixelCommand("rust_pixels_get_epoch", { docId, layerId }),
    ).rejects.toBe(`layer not initialized: ${layerId}`);

    // Two identical read-only depth calls: same answer, still reporting the
    // entry that remove_layer left behind in the document-level stream
    // (pixel_store.rs:197-200 drops storage only; :715-734 is read-only).
    const depthA = await getPixelHistoryDepth(docId);
    const depthB = await getPixelHistoryDepth(docId);
    expect(depthA.total_depth, "total_depth > 0 after remove_layer").toBe(1);
    expect(depthB, "two identical read-only calls agree").toEqual(depthA);
    expect(depthA.affected_layer_ids, "the surviving entry still names the removed layer").toEqual([layerId]);

    const census = await flushPixelInvokeCensus();
    const censusWrites = census.entries.filter(
      (e) => e.order > startOrder && SIX_COMMAND_ALLOWLIST.includes(e.command),
    );
    expect(
      censusWrites.map((e) => e.command),
      "the six-command census reads zero over this window",
    ).toEqual([]);

    // The enablement-event precondition this counterexample defends: a zero
    // census must NEVER unblock the event on its own - it also needs an empty
    // Rust history, which the depth probe reports as 1 here.
    const eventUnblocked = censusWrites.length === 0 && depthA.total_depth === 0;
    expect(
      eventUnblocked,
      "census 0 does not unblock the event while total_depth > 0",
    ).toBe(false);

    // Recorder-aliveness after the zero: one harness-driven allowlisted command
    // lands, so the zero above came from "no census command ran", never from a
    // recorder that cannot record.
    const postOrder = census.entries.length ? census.entries[census.entries.length - 1].order : startOrder;
    await invokePixelCommand("apply_tile_patch", { docId, layerId, before: [], after: [] });
    const alive = await flushPixelInvokeCensus();
    expect(
      alive.entries.filter((e) => e.order > postOrder).map((e) => e.command),
      "the census recorder is alive",
    ).toEqual(["apply_tile_patch"]);
  });
});
