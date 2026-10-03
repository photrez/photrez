import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import { EditorProvider, useEditor } from "../shell/EditorContext";
import { useCanvasKeyboard } from "../canvas/useCanvasKeyboard";
import { clearRegistry } from "../keyboardRegistry";
import { WorkspaceManager } from "@/engine/workspace";
import { settlePixelOps, createRustStoreEmulator, installCreateImageBitmapMock } from "@/lib/paint/__tests__/rustStoreEmulator";
import type { ToolType } from "@/viewport/input-handler";

// Delete pixels and Cut are recorded by Rust as one canonical pixel write, so
// their history commit and bitmap projection land asynchronously. With no Tauri
// runtime in jsdom there is no store to write into, so the op reports a visible
// failure and never commits - these tests therefore drive a faithful store
// emulator and await the write.
const hoist = vi.hoisted(() => ({ invoke: null as null | ((c: string, a: any) => Promise<any>) }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: (c: string, a: any) => hoist.invoke!(c, a) }));

let store: ReturnType<typeof createRustStoreEmulator>;

/**
 * OffscreenCanvas mock for jsdom (which has no OffscreenCanvas).
 * Tracks drawImage + clearRect so we can verify pixel changes happened,
 * and provides a working getImageData/putImageData/transferToImageBitmap
 * roundtrip on a Uint8ClampedArray buffer.
 */
function setupOffscreenCanvasMock() {
  const MockOffscreenCanvas = function (this: any, w: number, h: number) {
    this.width = w;
    this.height = h;
    this._buffer = new Uint8ClampedArray(w * h * 4);
    const ctx = {
      width: w,
      height: h,
      _buffer: this._buffer,
      drawImage: vi.fn(function (this: any, src: any, ...rest: any[]) {
        if (src && src._buffer) {
          // Simplified: just copy the entire source buffer
          const len = Math.min(src._buffer.length, this._buffer.length);
          for (let i = 0; i < len; i++) this._buffer[i] = src._buffer[i];
        }
      }),
      clearRect: vi.fn(function (this: any, x: number, y: number, cw: number, ch: number) {
        for (let row = y; row < y + ch; row++) {
          for (let col = x; col < x + cw; col++) {
            if (row < 0 || row >= this.height || col < 0 || col >= this.width) continue;
            const idx = (row * this.width + col) * 4;
            this._buffer[idx] = 0;
            this._buffer[idx + 1] = 0;
            this._buffer[idx + 2] = 0;
            this._buffer[idx + 3] = 0;
          }
        }
      }),
      putImageData: vi.fn(function (this: any, imageData: ImageData, x: number, y: number) {
        for (let row = 0; row < imageData.height; row++) {
          for (let col = 0; col < imageData.width; col++) {
            const srcIdx = (row * imageData.width + col) * 4;
            const dstCol = x + col;
            const dstRow = y + row;
            if (dstRow < 0 || dstRow >= this.height || dstCol < 0 || dstCol >= this.width) continue;
            const dstIdx = (dstRow * this.width + dstCol) * 4;
            this._buffer[dstIdx] = imageData.data[srcIdx];
            this._buffer[dstIdx + 1] = imageData.data[srcIdx + 1];
            this._buffer[dstIdx + 2] = imageData.data[srcIdx + 2];
            this._buffer[dstIdx + 3] = imageData.data[srcIdx + 3];
          }
        }
      }),
      getImageData: vi.fn(function (this: any, x: number, y: number, gw: number, gh: number) {
        const data = new Uint8ClampedArray(gw * gh * 4);
        for (let row = 0; row < gh; row++) {
          for (let col = 0; col < gw; col++) {
            const srcCol = x + col;
            const srcRow = y + row;
            if (srcRow < 0 || srcRow >= this.height || srcCol < 0 || srcCol >= this.width) continue;
            const srcIdx = (srcRow * this.width + srcCol) * 4;
            const dstIdx = (row * gw + col) * 4;
            data[dstIdx] = this._buffer[srcIdx];
            data[dstIdx + 1] = this._buffer[srcIdx + 1];
            data[dstIdx + 2] = this._buffer[srcIdx + 2];
            data[dstIdx + 3] = this._buffer[srcIdx + 3];
          }
        }
        return { data, width: gw, height: gh, colorSpace: "srgb" } as ImageData;
      }),
    };
    this.getContext = vi.fn(() => ctx);
    this.transferToImageBitmap = vi.fn(function (this: any) {
      return {
        width: this.width,
        height: this.height,
        _buffer: this._buffer,
      } as unknown as ImageBitmap;
    });
  };

  vi.stubGlobal("OffscreenCanvas", MockOffscreenCanvas as unknown as typeof OffscreenCanvas);
}

function makeFilledBitmap(w: number, h: number, r: number, g: number, b: number, a = 255) {
  const buffer = new Uint8ClampedArray(w * h * 4);
  for (let i = 0; i < w * h; i++) {
    buffer[i * 4] = r;
    buffer[i * 4 + 1] = g;
    buffer[i * 4 + 2] = b;
    buffer[i * 4 + 3] = a;
  }
  return { width: w, height: h, _buffer: buffer } as any;
}

function SelectionKeyboardHarness(props: {
  captureEditor: (editor: ReturnType<typeof useEditor>) => void;
  activeTool: ToolType;
}) {
  const editor = useEditor();
  props.captureEditor(editor);
  useCanvasKeyboard({
    isSpacePressed: () => false,
    setIsSpacePressed: vi.fn(),
    isAltPressed: () => false,
    setIsAltPressed: vi.fn(),
    isPanning: () => false,
    setIsPanning: vi.fn(),
    stopMomentum: vi.fn(),
    fitToScreenAndRender: vi.fn(),
    syncViewport: vi.fn(),
    getCanvasContainerRef: () => undefined,
  });

  return null;
}

function fireKey(target: EventTarget, init: KeyboardEventInit) {
  const ev = new KeyboardEvent("keydown", { bubbles: true, cancelable: true, ...init });
  target.dispatchEvent(ev);
  return ev;
}

function setupHarness() {
  const session = WorkspaceManager.createBlankDocument("sel-kbd-history", "Sel Kbd History", 400, 300);
  const ws = new WorkspaceManager();
  const renderer = { uploadImage: vi.fn(), destroyTexture: vi.fn() };
  const scheduler = { requestRender: vi.fn() };
  const container = document.createElement("div");
  document.body.appendChild(container);

  let editorRef: any = null;

  const dispose = render(
    () => (
      <EditorProvider workspace={ws} renderer={renderer as any} scheduler={scheduler as any}>
        <SelectionKeyboardHarness
          captureEditor={(e) => (editorRef = e)}
          activeTool={"selection"}
        />
      </EditorProvider>
    ),
    container,
  );

  ws.addDocument(session);
  const engine = ws.getActiveEngine()!;

  // Add a real layer with a filled bitmap so delete/cut have something to mutate
  const layer = engine.addLayer("Test", 100, 100);
  const originalBitmap = makeFilledBitmap(100, 100, 255, 0, 0);
  engine.setLayerImageBitmap(layer.id, originalBitmap);
  engine.setActiveLayer(layer.id);

  return {
    ws,
    engine,
    session,
    layer,
    originalBitmap,
    editorRef: editorRef as any,
    renderer: renderer as any,
    scheduler: scheduler as any,
    dispose: () => {
      dispose();
      container.parentNode?.removeChild(container);
    },
  };
}

/**
 * Regression tests for the two bugs the user reported (2026-06-14):
 *  1. Selection edits (cut/paste/delete) cannot be redone — root cause: the
 *     keyboard handler and toolbar mutate engine state WITHOUT first committing
 *     a snapshot to history, so the post-action state is never on the undo
 *     stack and redo has nothing to replay.
 *  2. Canvas not visually updated after a selection edit — root cause: the
 *     call sites update `engine.layer.imageBitmap` but never call
 *     `renderer.uploadImage(layerId, bitmap)` to refresh the GPU texture. The
 *     user has to switch tools to trigger a re-upload from another code path.
 */
describe("selection tool — history + renderer integration on edit (regression: 2026-06-14)", () => {
  beforeEach(() => {
    setupOffscreenCanvasMock();
    installCreateImageBitmapMock();
    store = createRustStoreEmulator();
    hoist.invoke = store.invoke;
  });

  afterEach(() => {
    store.dispose();
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
    clearRegistry();
  });

  it("NON-VACUITY: the jsdom createImageBitmap mock carries this suite's canvas pixels", async () => {
    // Every other case here asserts history entries, renderer uploads and bitmap
    // IDENTITY (`lastCall[1] === layer.imageBitmap`), so none of them can see whether
    // a bitmap has pixels at all - `toBe` on an opaque object passes just as well for
    // an empty one. The mock is what `PaintTileSurface.toImageBitmap` routes through,
    // so a bitmap reporting a valid size with no bytes would make every draw of it a
    // silent no-op across this file. This suite's canvas mock has no fillRect, so the
    // canvas is filled through its putImageData.
    const canvas = new OffscreenCanvas(8, 8);
    const ctx = canvas.getContext("2d") as any;
    ctx.putImageData({ data: new Uint8ClampedArray(8 * 8 * 4).fill(255), width: 8, height: 8 }, 0, 0);

    const bitmap = (await (globalThis as any).createImageBitmap(canvas)) as {
      width: number;
      _buffer: Uint8ClampedArray;
    };
    expect(bitmap.width, "the bitmap keeps the canvas geometry").toBe(8);
    expect(bitmap._buffer.length, "and a full-size buffer, not a short one").toBe(8 * 8 * 4);
    let painted = 0;
    for (let i = 0; i + 3 < bitmap._buffer.length; i += 4) {
      if (
        bitmap._buffer[i] === 255 &&
        bitmap._buffer[i + 1] === 255 &&
        bitmap._buffer[i + 2] === 255 &&
        bitmap._buffer[i + 3] === 255
      ) {
        painted += 1;
      }
    }
    expect(
      painted,
      "every filled pixel must survive the mock, or a draw of this bitmap paints nothing " +
        "and the identity assertions above still pass",
    ).toBe(8 * 8);
  });

  // ── Bug 1: redo doesn't work for selection edits ──

  it("Delete commits pre-action snapshot to history (regression: redo broken)", async () => {
    const { engine, editorRef, ws, dispose } = setupHarness();
    const history = ws.getActiveHistory()!;
    const commitSpy = vi.spyOn(history, "commit");

    engine.createSelection(10, 10, 30, 30);
    editorRef.setActiveTool("selection");

    fireKey(window, { key: "Delete" });
    await settlePixelOps();

    // The bug was: no commit was called, so the pre-action state was never
    // recorded and undo+redo had nothing to restore.
    expect(commitSpy).toHaveBeenCalled();
    // The commit should be the engine's pre-action snapshot.
    const committed = commitSpy.mock.calls[0][0] as any;
    expect(committed).toBeDefined();
    expect(committed.layers).toBeDefined();
    dispose();
  });

  it("Ctrl+X (cut) commits pre-action snapshot to history (regression: redo broken)", async () => {
    const { engine, editorRef, ws, dispose } = setupHarness();
    const history = ws.getActiveHistory()!;
    const commitSpy = vi.spyOn(history, "commit");

    engine.createSelection(10, 10, 30, 30);
    editorRef.setActiveTool("selection");

    fireKey(window, { key: "x", ctrlKey: true });
    await settlePixelOps();

    expect(commitSpy).toHaveBeenCalled();
    dispose();
  });

  it("Ctrl+V (paste) commits pre-action snapshot to history (regression: redo broken)", async () => {
    const { engine, editorRef, ws, dispose } = setupHarness();
    const history = ws.getActiveHistory()!;
    const commitSpy = vi.spyOn(history, "commit");

    // Pre-populate the clipboard by copying an existing selection
    engine.createSelection(10, 10, 30, 30);
    editorRef.setActiveTool("selection");
    fireKey(window, { key: "c", ctrlKey: true });

    // Now paste
    commitSpy.mockClear();
    fireKey(window, { key: "v", ctrlKey: true });

    expect(commitSpy).toHaveBeenCalled();
    dispose();
  });

  // ── Bug 2: canvas not updated after selection edit ──

  it("Delete uploads new bitmap to renderer (regression: canvas stale)", async () => {
    const { engine, editorRef, renderer, dispose } = setupHarness();

    engine.createSelection(10, 10, 30, 30);
    editorRef.setActiveTool("selection");

    fireKey(window, { key: "Delete" });
    await settlePixelOps();

    // The bug was: renderer's GPU texture still held the old bitmap because
    // no uploadImage call was made. The user had to switch tools to trigger
    // a re-upload from another code path.
    const calls = (renderer.uploadImage as any).mock.calls as Array<[string, any]>;
    expect(calls.length).toBeGreaterThan(0);
    // The uploaded bitmap should be the post-delete (new) bitmap of the
    // active layer, not the original.
    const activeId = engine.getActiveLayerId();
    const activeLayer = engine.getLayer(activeId!);
    const lastCall = calls[calls.length - 1];
    expect(lastCall[0]).toBe(activeId);
    expect(lastCall[1]).toBe(activeLayer!.imageBitmap);
    dispose();
  });

  it("Ctrl+X (cut) uploads new bitmap to renderer (regression: canvas stale)", async () => {
    const { engine, editorRef, renderer, dispose } = setupHarness();

    engine.createSelection(10, 10, 30, 30);
    editorRef.setActiveTool("selection");

    fireKey(window, { key: "x", ctrlKey: true });
    await settlePixelOps();

    const calls = (renderer.uploadImage as any).mock.calls as Array<[string, any]>;
    expect(calls.length).toBeGreaterThan(0);
    dispose();
  });

  it("Ctrl+V (paste) uploads new bitmap to renderer for the new layer (regression: canvas stale)", async () => {
    const { engine, editorRef, renderer, dispose } = setupHarness();

    // Copy first to populate clipboard
    engine.createSelection(10, 10, 30, 30);
    editorRef.setActiveTool("selection");
    fireKey(window, { key: "c", ctrlKey: true });

    // Paste creates a new layer — the new layer's bitmap must be uploaded
    fireKey(window, { key: "v", ctrlKey: true });

    const calls = (renderer.uploadImage as any).mock.calls as Array<[string, any]>;
    expect(calls.length).toBeGreaterThan(0);
    // The last uploaded layer should be the new "Pasted Layer"
    const lastCall = calls[calls.length - 1];
    const lastLayer = engine.getLayer(lastCall[0]);
    expect(lastLayer!.name).toContain("Pasted");
    dispose();
  });

  // ── End-to-end: undo + redo roundtrip actually restores the bitmap ──

  it("Delete -> Undo -> Redo roundtrip restores bitmap (regression: full flow)", async () => {
    const { engine, editorRef, ws, layer, originalBitmap, dispose } = setupHarness();
    const history = ws.getActiveHistory()!;

    engine.createSelection(10, 10, 30, 30);
    editorRef.setActiveTool("selection");

    // 1. Action: Delete — bitmap should be replaced
    fireKey(window, { key: "Delete" });
    await settlePixelOps();
    const afterDeleteBitmap = engine.getLayer(layer.id)!.imageBitmap;
    expect(afterDeleteBitmap).not.toBeNull();
    expect(afterDeleteBitmap).not.toBe(originalBitmap);

    // 2. Undo — bitmap should be the original (red)
    const prev = history.undo(engine.snapshot());
    expect(prev).not.toBeNull();
    if (prev) engine.restore(prev);
    const afterUndoBitmap = engine.getLayer(layer.id)!.imageBitmap;
    expect(afterUndoBitmap).toBe(originalBitmap);

    // 3. Redo — bitmap should be the post-delete bitmap
    const next = history.redo(engine.snapshot());
    expect(next).not.toBeNull();
    if (next) engine.restore(next);
    const afterRedoBitmap = engine.getLayer(layer.id)!.imageBitmap;
    expect(afterRedoBitmap).toBe(afterDeleteBitmap);

    dispose();
  });
});
