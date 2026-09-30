// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Cursor parity for a brush stroke on the photrez.rustPixels=OFF default path
// with the facade enabled (also a shipped default).
//
// Defect under test: the commit shim in lib/protocol/facadeRegistry.ts skips its
// External mirror whenever CommandHistory.commit is called with
// alreadyRecordedInRust=true. That is only sound when Rust actually owns the
// stroke's Pixel cursor entry, i.e. when rust_pixels_write_region ran - which
// only happens on the photrez.rustPixels=ON path. components/editor/
// useBrushOverlay.ts passes a literal true on the OTHER path too (the non-C4
// commit), so on the default flag state the stroke adds a TS history entry and
// NO Rust cursor entry. The unified cursor then sits one step behind the TS undo
// stack, and the first Ctrl+Z is spent walking the stale entry the previous
// projection left on the cursor instead of reverting the paint.
//
// The invariant both tests pin is the one useEditorCommands states: the TS
// cursor and the Rust cursor must move together.
//
// Cursor observable: getHistoryProjection(docId).cursor is ProtocolEngine's
// cursor, the exact number the rust_pixels_history_depth Tauri command reports
// as undo_depth (crates/core/src/pixel_store.rs:715-734 reads history.cursor()).
// The command itself needs a Tauri runtime jsdom has none of, so the same field
// is read through the protocol history query on the SAME engine - the real Rust
// wasm module, not a hand-written model (see src/test/wasmTestShim.ts).
//
// Non-vacuity: the document is seeded through the real facade projection
// (seedFacadeFromEngine + addLayer), which both marks layers facade-owned (the
// gate useEditorCommands consults) and leaves a non-empty Rust cursor. Without
// that pre-existing entry an empty Rust cursor would fall through to the TS
// stack on the first undo and the defect would hide.
import { describe, it, expect, vi, beforeAll, beforeEach, afterAll, afterEach } from "vitest";
import { render } from "solid-js/web";
import { ImageData as NodeImageData } from "canvas";
import { invoke } from "@tauri-apps/api/core";
import { WorkspaceManager } from "@/engine/workspace";
import { DocumentEngine, hasFacadeOwnedLayers, isFacadeOwnedLayer } from "@/engine/document";
import { getWasmExportModule } from "../wasmExport";
import { flushC4Commits } from "../useBrushOverlay";
import { useEditorCommands } from "../useEditorCommands";
import { EditorProvider, useEditor } from "../shell/EditorContext";
import { CanvasViewport } from "../canvas/CanvasViewport";
import {
  getFacade,
  getHistoryProjection,
  installFacadeCommitShim,
  seedFacadeFromEngine,
  __resetFacadeRegistryForTests,
} from "@/lib/protocol/facadeRegistry";
import * as DialogProviderModule from "../dialogs/DialogProvider";

// Tauri surface: with photrez.rustPixels=0 the document is never opened in the
// Rust pixel registry, so every rust_pixels_* read rejects with the bare string
// Tauri surfaces for an unknown document (pixel_history_depth.rs:39). The
// production callers all treat that rejection as "no Rust store", which is the
// state this file reproduces.
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(() => Promise.resolve(() => {})) }));
vi.mock("@tauri-apps/api/app", () => ({ getVersion: vi.fn(() => Promise.resolve("0.0.0")) }));

// Render/viewport layer only. The brush overlay, the pointer dispatcher and the
// command routing stay real, so the pointer chain below is a real one.
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
// The canvas keyboard arm does not own Ctrl+Z (it registers the shortcut as an
// intentional fallthrough conflict); useEditorCommands' own window listener does.
vi.mock("../canvas/useCanvasKeyboard", () => ({ useCanvasKeyboard: vi.fn() }));

// jsdom has no ImageData / OffscreenCanvas / createImageBitmap; node-canvas backs
// <canvas>. FakeImageData extends the node-canvas class because its 2D context
// rejects foreign ImageData instances, and widens the constructor to any
// ArrayLike<number> - which is how tile payloads arrive.
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

const DOC = "doc-flagoff";
const SIZE = 256;

let wasmModule: { protocol_reset: (docId: string) => void } | null = null;
let liveEngine: DocumentEngine | null = null;
let dispose: (() => void) | undefined;
let container: HTMLDivElement | undefined;
let rectSpy: ReturnType<typeof vi.spyOn> | undefined;

let setTool: (tool: string) => void = () => {};
let setFgColor: (color: string) => void = () => {};
let setZoom: (zoom: number) => void = () => {};
let setPan: (pan: { x: number; y: number }) => void = () => {};

const Consumer = () => {
  const editor = useEditor();
  setTool = editor.setActiveTool;
  setFgColor = editor.setFgColor;
  setZoom = editor.setZoom;
  setPan = editor.setPan;
  // Mounted so the production Ctrl+Z window listener exists: calling
  // commands.undo() directly would skip the user action under test.
  useEditorCommands(() => {});
  return null;
};

const tick = (ms = 0) => new Promise<void>((resolve) => setTimeout(resolve, ms));

/** Drain the deferred commit queue plus the shim's fire-and-forget mirror. */
async function settle(): Promise<void> {
  await flushC4Commits();
  for (let i = 0; i < 12; i++) await tick();
  await flushC4Commits();
  await tick(50);
}

/** FNV-1a over real bitmap bytes, order-sensitive. */
function hash(bitmap: unknown): string {
  const b = bitmap as {
    data?: Uint8ClampedArray;
    width?: number;
    height?: number;
    getContext?: (t: string) => CanvasRenderingContext2D | null;
  };
  let data: Uint8ClampedArray;
  if (b?.data) {
    data = b.data;
  } else {
    const ctx = b?.getContext?.("2d");
    if (!ctx) throw new Error("bitmap carries no readable bytes");
    data = ctx.getImageData(0, 0, b.width as number, b.height as number).data;
  }
  let h = 0x811c9dc5;
  for (let i = 0; i < data.length; i++) {
    h ^= data[i];
    h = Math.imul(h, 0x01000193) >>> 0;
  }
  return h.toString(16).padStart(8, "0");
}

function firePointer(type: string, el: Element, clientX: number, clientY: number) {
  el.dispatchEvent(
    new PointerEvent(type, {
      bubbles: true,
      cancelable: true,
      button: 0,
      pointerId: 7,
      clientX,
      clientY,
    }),
  );
}

/** One real brush stroke: pointerdown -> pointermove -> pointerup. */
function brushChain(el: Element) {
  firePointer("pointerdown", el, 40, 40);
  firePointer("pointermove", el, 70, 60);
  firePointer("pointerup", el, 70, 60);
}

function viewportCanvas(): HTMLCanvasElement {
  const c = container?.querySelector("canvas:not([data-overlay-canvas])") as HTMLCanvasElement | null;
  if (!c) throw new Error("viewport canvas not found");
  return c;
}

/**
 * Mount the real editor shell over a facade-seeded document: the projection
 * marks the layers facade-owned (the gate useEditorCommands consults) and leaves
 * the Rust cursor holding the entries earlier operations recorded.
 */
async function mount(opts: { rustPixels: "0" | "1"; tool: "brush" | "eraser" } = {
  rustPixels: "0",
  tool: "brush",
}) {
  localStorage.setItem("photrez.rustPixels", opts.rustPixels);
  const workspace = new WorkspaceManager();
  const session = WorkspaceManager.createBlankDocument(DOC, "FlagOff", SIZE, SIZE);
  workspace.addDocument(session);
  const engine = session.engine;
  liveEngine = engine;

  const facade = getFacade(DOC);
  await seedFacadeFromEngine(engine as never, facade);
  const added = (await facade.addLayer("Owned", 0, 0, 0)) as unknown as {
    layers: Array<{ id: string; name: string }>;
  };
  engine.applyFacadeSnapshot(added as never);
  // The paint target is added through the legacy path AFTER the projection, and
  // that is what makes the scenario reachable. applyFacadeSnapshot marks exactly
  // the ids its own projection carried, so a layer the legacy path adds later is
  // not owned - which is the only way the brush is paintable (Gate A,
  // useBrushOverlay.onPaintStroke, refuses a facade-owned layer) while
  // hasFacadeOwnedLayers() is still true, i.e. while the facade handoff owns the
  // undo route. The parent prompt's premise that the gate is per-layer is wrong
  // for a layer present in the projection; it holds only for one added after it.
  const paintId = engine.addLayer("Painted", SIZE, SIZE).id;
  engine.setActiveLayer(paintId);

  const bitmap = document.createElement("canvas");
  bitmap.width = SIZE;
  bitmap.height = SIZE;
  const bctx = bitmap.getContext("2d") as CanvasRenderingContext2D;
  bctx.fillStyle = "#ffffff";
  bctx.fillRect(0, 0, SIZE, SIZE);
  engine.setLayerImageBitmap(paintId, bitmap as unknown as ImageBitmap);
  if (!engine.getPaintSurface(paintId)) throw new Error("paint surface not created");

  container = document.createElement("div");
  document.body.appendChild(container);
  const renderer: Record<string, unknown> = {
    uploadImage: vi.fn(),
    destroyTexture: vi.fn(),
    uploadSurfaceTiles: vi.fn(),
  };
  const scheduler: Record<string, unknown> = { requestRender: vi.fn() };
  dispose = render(
    () => (
      <EditorProvider workspace={workspace} renderer={renderer as never} scheduler={scheduler as never}>
        <Consumer />
        <CanvasViewport />
      </EditorProvider>
    ),
    container,
  );
  await tick();
  setZoom(1);
  setPan({ x: 0, y: 0 });
  setTool(opts.tool);
  setFgColor("#ff0000");
  await tick();

  return { engine, history: session.history, paintId, canvas: viewportCanvas() };
}

beforeAll(async () => {
  wasmModule = await getWasmExportModule();
  vi.spyOn(DialogProviderModule, "useDialog").mockReturnValue({
    confirm: vi.fn(),
  } as unknown as ReturnType<typeof DialogProviderModule.useDialog>);
  // The production commit shim, installed once at EditorShell boot. The shim is
  // module-sticky, so install it before any test commits.
  installFacadeCommitShim({
    getEngine: () => liveEngine as never,
    getDocId: () => liveEngine?.getId() ?? DOC,
  });
});

afterAll(() => vi.restoreAllMocks());

beforeEach(() => {
  // The Rust pixel registry is never opened on this path: every rust_pixels_*
  // read rejects the way pixel_history_depth.rs:39 rejects an unknown document,
  // and a state-changing write rejects because the store is not initialized.
  vi.mocked(invoke).mockReset();
  vi.mocked(invoke).mockRejectedValue("pixel store not initialized");
  localStorage.clear();
  // The two shipped defaults, plus the pixel flag made explicit.
  localStorage.setItem("photrez.facade", "1");
  localStorage.setItem("photrez.facadeAuthority", "wasm");
  localStorage.setItem("photrez.rustPixels", "0");
  localStorage.removeItem("photrez.canonicalCommit");
  localStorage.removeItem("photrez.tileCommit");
  rectSpy = vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
    left: 0,
    top: 0,
    right: SIZE,
    bottom: SIZE,
    width: SIZE,
    height: SIZE,
    x: 0,
    y: 0,
    toJSON: () => ({}),
  } as DOMRect);
  Element.prototype.setPointerCapture = vi.fn();
  Element.prototype.releasePointerCapture = vi.fn();
});

afterEach(() => {
  dispose?.();
  dispose = undefined;
  container?.parentNode?.removeChild(container);
  container = undefined;
  rectSpy?.mockRestore();
  rectSpy = undefined;
  liveEngine = null;
  localStorage.clear();
  __resetFacadeRegistryForTests();
  wasmModule?.protocol_reset(DOC);
});

const rustCursor = async (): Promise<number> => (await getHistoryProjection(DOC)).cursor;

/** The two states the scenario needs, asserted so the test states its premise. */
function assertScenarioPremise(paintId: string) {
  expect(hasFacadeOwnedLayers(), "PROBE the projection marked layers facade-owned").toBe(true);
  expect(
    isFacadeOwnedLayer(paintId),
    "PROBE the painted layer is not facade-owned, so Gate A lets the brush run",
  ).toBe(false);
}

describe("rustPixels=OFF stroke keeps the Rust cursor in step with the TS undo stack (facade=1)", () => {
  it("records exactly one cursor entry for the stroke", async () => {
    const { history, paintId, canvas } = await mount();
    assertScenarioPremise(paintId);

    const tsBefore = history.getUndoCount();
    const rustBefore = await rustCursor();
    expect(rustBefore, "PROBE the projection left a non-empty Rust cursor").toBeGreaterThan(0);

    brushChain(canvas);
    await settle();

    const tsDelta = history.getUndoCount() - tsBefore;
    const rustDelta = (await rustCursor()) - rustBefore;
    // The stroke must be undoable, so the TS stack really grew.
    expect(tsDelta, "PROBE the stroke committed one TS history entry").toBe(1);
    // The invariant: TS cursor == Rust cursor. A skipped mirror shows up here as
    // a Rust cursor one entry short of the stack it must track.
    expect(
      rustDelta,
      `Rust cursor moved ${String(rustDelta)} for a stroke that moved the TS stack ${String(tsDelta)}`,
    ).toBe(tsDelta);
  });

  it("one Ctrl+Z reverts the stroke", async () => {
    const { engine, history, paintId, canvas } = await mount();
    assertScenarioPremise(paintId);

    const preHash = hash(engine.getLayer(paintId)!.imageBitmap);
    const tsBefore = history.getUndoCount();

    brushChain(canvas);
    await settle();
    const postHash = hash(engine.getLayer(paintId)!.imageBitmap);
    expect(postHash, "PROBE the stroke is visible in the model bitmap").not.toBe(preHash);
    expect(history.getUndoCount() - tsBefore, "PROBE the stroke committed one TS entry").toBe(1);

    // ONE real Ctrl+Z. The keydown listener under test is the one
    // useEditorCommands registers on window, so this is the user action.
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "z", ctrlKey: true, bubbles: true }));
    for (let i = 0; i < 20; i++) await tick();
    await tick(50);

    expect(
      hash(engine.getLayer(paintId)!.imageBitmap),
      "one Ctrl+Z must revert the stroke, not spend the step on a stale cursor entry",
    ).toBe(preHash);
  });
});

// The same hole with the pixel flag ON. The eraser is excluded from the deferred
// Rust write (useBrushOverlay enqueues it only for `!effectiveIsEraser`), so an
// eraser stroke at rustPixels=1 owns NO Pixel entry in Rust even though the flag
// is on. If the commit still claims Rust already recorded it, the mirror is
// skipped and the cursor falls one entry behind the TS undo stack. The existing
// eraser guard in brushStrokeCommitOrdering.test.ts cannot see this: it asserts
// TS history only, and a cursor is a Rust-side structure.
describe("rustPixels=ON eraser stroke keeps the Rust cursor in step with the TS undo stack (facade=1)", () => {
  it("records exactly one cursor entry, and no Rust pixel write was even attempted", async () => {
    const { history, paintId, canvas } = await mount({ rustPixels: "1", tool: "eraser" });
    assertScenarioPremise(paintId);

    const tsBefore = history.getUndoCount();
    const rustBefore = await rustCursor();
    expect(rustBefore, "PROBE the projection left a non-empty Rust cursor").toBeGreaterThan(0);

    brushChain(canvas);
    await settle();

    // The fact that makes the cursor assertion below mandatory: the eraser never
    // reaches rust_pixels_write_region, so nothing in Rust recorded this stroke
    // and the only thing that can move the cursor is the commit shim's mirror.
    expect(
      vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "rust_pixels_write_region").length,
      "PROBE the eraser sent no Rust pixel write, so Rust owns no entry for it",
    ).toBe(0);

    const tsDelta = history.getUndoCount() - tsBefore;
    const rustDelta = (await rustCursor()) - rustBefore;
    expect(tsDelta, "PROBE the eraser stroke committed one TS history entry").toBe(1);
    expect(
      rustDelta,
      `Rust cursor moved ${String(rustDelta)} for an eraser stroke that moved the TS stack ${String(tsDelta)}`,
    ).toBe(tsDelta);
  });
});
