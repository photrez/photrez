// apps/desktop/src/components/editor/layers/__tests__/layerOperations.test.ts
//
// Contract tests for layerOperations.ts — pure functions operating on a
// real DocumentEngine.  If these break, layer merge/flatten silently fail
// (user clicks "Merge Down" → nothing happens).

import { describe, it, expect, vi, beforeEach, beforeAll, afterAll, afterEach } from "vitest";
import { DocumentEngine } from "@/engine/document";
import { CommandHistory } from "@/engine/history";
import { mergeActiveLayerDown, flattenAllLayers, fillActiveLayerWithColor } from "../layerOperations";
import type { WebGL2Backend } from "@/renderer/webgl2";
import * as Toast from "../../Toast";

// Polyfill OffscreenCanvas for jsdom — DocumentEngine.mergeDown and
// flattenLayers use OffscreenCanvas internally for pixel compositing.
// The 2D context mock must support all operations used by drawLayerToContext:
// save, restore, translate, rotate, scale, globalAlpha, globalCompositeOperation, drawImage.
const OriginalOffscreenCanvas = (globalThis as any).OffscreenCanvas;
beforeAll(() => {
  if (typeof OffscreenCanvas === "undefined") {
    (globalThis as any).OffscreenCanvas = class {
      width: number;
      height: number;
      constructor(w: number, h: number) { this.width = w; this.height = h; }
      getContext() {
        return {
          save: vi.fn(),
          restore: vi.fn(),
          translate: vi.fn(),
          rotate: vi.fn(),
          scale: vi.fn(),
          drawImage: vi.fn(),
          globalAlpha: 1,
          globalCompositeOperation: "source-over",
          canvas: this,
        } as any;
      }
      transferToImageBitmap() {
        return { width: this.width, height: this.height, close: vi.fn() } as unknown as ImageBitmap;
      }
    };
  }
});
afterAll(() => {
  if (OriginalOffscreenCanvas) {
    (globalThis as any).OffscreenCanvas = OriginalOffscreenCanvas;
  }
});

function makeMockRenderer(): WebGL2Backend {
  return {
    uploadImage: vi.fn(),
    destroyTexture: vi.fn(),
    render: vi.fn(),
    resizeToViewport: vi.fn(),
    getWebGLContext: vi.fn(),
  } as unknown as WebGL2Backend;
}

function makeBitmap(width = 100, height = 100): ImageBitmap {
  return { width, height, close: vi.fn() } as unknown as ImageBitmap;
}

describe("mergeActiveLayerDown", () => {
  let engine: DocumentEngine;
  let history: CommandHistory;
  let renderer: WebGL2Backend;

  beforeEach(() => {
    engine = new DocumentEngine("doc-1", "Test", 200, 200);
    history = new CommandHistory();
    renderer = makeMockRenderer();
  });

  it("merges active layer into the layer below it", () => {
    // addLayer inserts BEFORE the active layer and auto-sets active.
    // Bottom first, then Top → layers = [Top, Bottom], active = Top.
    const bottom = engine.addLayer("Bottom", 100, 100);
    const top = engine.addLayer("Top", 100, 100);
    const topBitmap = makeBitmap();
    const bottomBitmap = makeBitmap();
    engine.setLayerImageBitmap(top.id, topBitmap);
    engine.setLayerImageBitmap(bottom.id, bottomBitmap);

    const result = mergeActiveLayerDown(engine, history, renderer, top.id);

    expect(result).toBe(true);
    // After merge, only the bottom layer remains (top was merged into it)
    const layers = engine.getLayers();
    expect(layers).toHaveLength(1);
    expect(history.getUndoCount()).toBe(1);
    // Top layer bitmap was destroyed in renderer
    expect(renderer.destroyTexture).toHaveBeenCalledWith(top.id);
    expect(renderer.destroyTexture).toHaveBeenCalledWith(bottom.id);
    // Merged layer bitmap was uploaded
    expect(renderer.uploadImage).toHaveBeenCalled();
  });

  it("returns false when active layer is the bottom-most layer (no layer below)", () => {
    engine.addLayer("Only", 100, 100);
    const onlyId = engine.getLayers()[0].id;
    engine.setActiveLayer(onlyId);

    const result = mergeActiveLayerDown(engine, history, renderer, onlyId);

    expect(result).toBe(false);
    expect(history.getUndoCount()).toBe(0);
    expect(renderer.destroyTexture).not.toHaveBeenCalled();
  });

  it("does nothing when activeId is missing from engine", () => {
    const result = mergeActiveLayerDown(engine, history, renderer, "non-existent");

    expect(result).toBe(false);
    expect(history.getUndoCount()).toBe(0);
  });
});

// ─── fillActiveLayerWithColor: selection-aware scoping ──────────────────────
// Mirrors how similar editors fill only the selected region. The OffscreenCanvas
// mock here tracks real pixels so we can assert which areas were painted.
describe("fillActiveLayerWithColor (selection-aware)", () => {
  let prevOffscreenCanvas: any;

  function installPixelMock() {
    prevOffscreenCanvas = (globalThis as any).OffscreenCanvas;
    (globalThis as any).OffscreenCanvas = class {
      width: number;
      height: number;
      _buffer: Uint8ClampedArray;
      constructor(w: number, h: number) {
        this.width = w;
        this.height = h;
        this._buffer = new Uint8ClampedArray(w * h * 4);
      }
      getContext() {
        const self = this;
        return {
          _fs: "" as string,
          get fillStyle() { return (this as any)._fs; },
          set fillStyle(v: string) { (this as any)._fs = v; },
          fillRect: function (this: any, x: number, y: number, w: number, h: number) {
            const hex = (this._fs as string).replace("#", "");
            const r = parseInt(hex.slice(0, 2), 16);
            const g = parseInt(hex.slice(2, 4), 16);
            const b = parseInt(hex.slice(4, 6), 16);
            for (let row = y; row < y + h; row++) {
              for (let col = x; col < x + w; col++) {
                if (row < 0 || row >= self.height || col < 0 || col >= self.width) continue;
                const idx = (row * self.width + col) * 4;
                self._buffer[idx] = r;
                self._buffer[idx + 1] = g;
                self._buffer[idx + 2] = b;
                self._buffer[idx + 3] = 255;
              }
            }
          },
          drawImage: vi.fn(),
          getImageData: function(x: number, y: number, w: number, h: number) {
            return {
              data: self._buffer,
              width: self.width, height: self.height,
              colorSpace: "srgb",
            };
          },
          putImageData: vi.fn(),
          save: vi.fn(),
          restore: vi.fn(),
          translate: vi.fn(),
          rotate: vi.fn(),
          scale: vi.fn(),
          globalAlpha: 1,
          globalCompositeOperation: "source-over",
        };
      }
      transferToImageBitmap() {
        const buf = this._buffer;
        return {
          width: this.width,
          height: this.height,
          getImageData: (_x: number, _y: number, _w: number, _h: number) => ({
            data: buf, width: this.width, height: this.height, colorSpace: "srgb",
          }),
          close: vi.fn(),
        } as unknown as ImageBitmap;
      }
    };
  }

  afterEach(() => {
    if (prevOffscreenCanvas !== undefined) {
      (globalThis as any).OffscreenCanvas = prevOffscreenCanvas;
    }
    vi.restoreAllMocks();
  });

  function setup() {
    installPixelMock();
    const engine = new DocumentEngine("fill-doc", "Fill", 100, 100);
    const layer = engine.addLayer("Target", 100, 100);
    engine.setActiveLayer(layer.id);
    const history = new CommandHistory();
    const renderer = makeMockRenderer();
    return { engine, layer, history, renderer };
  }

  function pixel(bitmap: any, x: number, y: number) {
    const d = (bitmap.getImageData as any)(0, 0, 100, 100).data;
    const idx = (y * 100 + x) * 4;
    return [d[idx], d[idx + 1], d[idx + 2], d[idx + 3]];
  }

  it("fills the entire layer when no selection is active", () => {
    const { engine, layer, history, renderer } = setup();
    const ok = fillActiveLayerWithColor(engine, history, renderer, "#ff0000");
    expect(ok).toBe(true);
    // Top-left and bottom-right corners both painted.
    expect(pixel(layer.imageBitmap, 0, 0)).toEqual([255, 0, 0, 255]);
    expect(pixel(layer.imageBitmap, 99, 99)).toEqual([255, 0, 0, 255]);
  });

  it("fills only the selection bounds when a selection is active", () => {
    const { engine, layer, history, renderer } = setup();
    engine.createSelection(10, 10, 20, 20, 0);

    const ok = fillActiveLayerWithColor(engine, history, renderer, "#00ff00");
    expect(ok).toBe(true);

    // Inside selection → green
    expect(pixel(layer.imageBitmap, 15, 15)).toEqual([0, 255, 0, 255]);
    // Outside selection → untouched (transparent)
    expect(pixel(layer.imageBitmap, 0, 0)).toEqual([0, 0, 0, 0]);
    expect(pixel(layer.imageBitmap, 99, 99)).toEqual([0, 0, 0, 0]);
  });

  it("fills everything EXCEPT the bounds when the selection is inverted", () => {
    const { engine, layer, history, renderer } = setup();
    engine.createSelection(10, 10, 20, 20, 0);
    engine.invertSelection();

    const ok = fillActiveLayerWithColor(engine, history, renderer, "#0000ff");
    expect(ok).toBe(true);

    // Inside excluded rect → untouched
    expect(pixel(layer.imageBitmap, 15, 15)).toEqual([0, 0, 0, 0]);
    // Outside → blue
    expect(pixel(layer.imageBitmap, 0, 0)).toEqual([0, 0, 255, 255]);
    expect(pixel(layer.imageBitmap, 99, 99)).toEqual([0, 0, 255, 255]);
  });

  it("fills the layer-local rect under a (translated) marquee, not the doc-space rect", () => {
    const { engine, layer, history, renderer } = setup();
    // Translate the layer by (+50,+50); the marquee in doc space (60,60,20,20)
    // maps to layer-local (10,10,20,20).
    engine.transformLayer(layer.id, { x: 50, y: 50, scaleX: 1, scaleY: 1, rotation: 0 });
    engine.createSelection(60, 60, 20, 20, 0);

    const ok = fillActiveLayerWithColor(engine, history, renderer, "#00ff00");
    expect(ok).toBe(true);

    // Inside the mapped layer-local rect → green
    expect(pixel(layer.imageBitmap, 15, 15)).toEqual([0, 255, 0, 255]);
    expect(pixel(layer.imageBitmap, 25, 25)).toEqual([0, 255, 0, 255]);
    // Outside it (the un-translated part of the layer) → untouched
    expect(pixel(layer.imageBitmap, 5, 5)).toEqual([0, 0, 0, 0]);
    expect(pixel(layer.imageBitmap, 95, 95)).toEqual([0, 0, 0, 0]);
  });

  it("fills the layer-local rect under a (scaled) marquee", () => {
    const { engine, layer, history, renderer } = setup();
    // Scale 2x, no translate. Doc-space (60,60,20,20) → layer-local (30,30,10,10).
    engine.transformLayer(layer.id, { x: 0, y: 0, scaleX: 2, scaleY: 2, rotation: 0 });
    engine.createSelection(60, 60, 20, 20, 0);

    const ok = fillActiveLayerWithColor(engine, history, renderer, "#ffff00");
    expect(ok).toBe(true);

    expect(pixel(layer.imageBitmap, 35, 35)).toEqual([255, 255, 0, 255]); // inside
    expect(pixel(layer.imageBitmap, 5, 5)).toEqual([0, 0, 0, 0]);          // outside
  });

  it("fills only INSIDE the ellipse when selection shape is ellipse", () => {
    const { engine, layer, history, renderer } = setup();
    engine.createSelection(20, 20, 60, 60, 0, "ellipse");

    const ok = fillActiveLayerWithColor(engine, history, renderer, "#ff0000");
    expect(ok).toBe(true);

    // Center of ellipse → inside → filled red.
    expect(pixel(layer.imageBitmap, 50, 50)).toEqual([255, 0, 0, 255]);
    // Corner of AABB (outside ellipse) → untouched (transparent).
    expect(pixel(layer.imageBitmap, 21, 21)).toEqual([0, 0, 0, 0]);
    expect(pixel(layer.imageBitmap, 78, 78)).toEqual([0, 0, 0, 0]);
  });

  it("fills everything OUTSIDE the ellipse when selection is inverted + ellipse", () => {
    const { engine, layer, history, renderer } = setup();
    engine.createSelection(20, 20, 60, 60, 0, "ellipse");
    engine.invertSelection();

    const ok = fillActiveLayerWithColor(engine, history, renderer, "#00ff00");
    expect(ok).toBe(true);

    // Center of ellipse → inside → untouched (transparent).
    expect(pixel(layer.imageBitmap, 50, 50)).toEqual([0, 0, 0, 0]);
    // Corner of AABB (outside ellipse) → filled green.
    expect(pixel(layer.imageBitmap, 21, 21)).toEqual([0, 255, 0, 255]);
    // Far corner (outside AABB entirely) → filled green.
    expect(pixel(layer.imageBitmap, 1, 1)).toEqual([0, 255, 0, 255]);
    expect(pixel(layer.imageBitmap, 95, 95)).toEqual([0, 255, 0, 255]);
  });
});

describe("flattenAllLayers", () => {
  let engine: DocumentEngine;
  let history: CommandHistory;
  let renderer: WebGL2Backend;

  beforeEach(() => {
    engine = new DocumentEngine("doc-1", "Test", 200, 200);
    history = new CommandHistory();
    renderer = makeMockRenderer();
  });

  it("flattens multiple layers into one", () => {
    const l1 = engine.addLayer("Layer 1", 100, 100);
    const l2 = engine.addLayer("Layer 2", 100, 100);
    const l3 = engine.addLayer("Layer 3", 100, 100);
    engine.setLayerImageBitmap(l1.id, makeBitmap());
    engine.setLayerImageBitmap(l2.id, makeBitmap());
    engine.setLayerImageBitmap(l3.id, makeBitmap());

    const result = flattenAllLayers(engine, history, renderer);

    expect(result).toBe(true);
    // All layers flattened into one (no separate background layer)
    const layers = engine.getLayers();
    expect(layers).toHaveLength(1);
    expect(history.getUndoCount()).toBe(1);
    // Each old layer's texture was destroyed
    expect(renderer.destroyTexture).toHaveBeenCalledWith(l1.id);
    expect(renderer.destroyTexture).toHaveBeenCalledWith(l2.id);
    expect(renderer.destroyTexture).toHaveBeenCalledWith(l3.id);
    // Flattened bitmap uploaded
    expect(renderer.uploadImage).toHaveBeenCalledTimes(1);
  });

  it("returns false when there is only the background layer (nothing to flatten)", () => {
    const result = flattenAllLayers(engine, history, renderer);

    expect(result).toBe(false);
    expect(history.getUndoCount()).toBe(0);
    expect(renderer.destroyTexture).not.toHaveBeenCalled();
    expect(renderer.uploadImage).not.toHaveBeenCalled();
  });

  it("commits history before flattening (undo restores pre-flatten state)", () => {
    engine.addLayer("L1", 100, 100);
    engine.addLayer("L2", 100, 100);
    const preFlattenSnapshot = engine.snapshot();

    flattenAllLayers(engine, history, renderer);

    // Undo should restore exactly the pre-flatten state
    engine.restore(history.undo(engine.snapshot())!);
    const restoredLayers = engine.getLayers();
    expect(restoredLayers).toHaveLength(preFlattenSnapshot.layers.length);
  });
});

// ─────────────────────────────────────────────────────────────
//  useLayerActions wiring test — mount the hook inside a
//  mock EditorProvider and verify each action produces the
//  expected engine + history + renderer side effects.
// ─────────────────────────────────────────────────────────────

// (OffscreenCanvas polyfill is at the top of the file — covers all tests)

import { renderHook } from "@solidjs/testing-library";
import { EditorProvider, useEditor } from "../../shell/EditorContext";
import { useLayerActions } from "../useLayerActions";
import { WorkspaceManager } from "@/engine/workspace";
import { DialogProvider } from "../../dialogs/DialogProvider";
import type { TextData } from "@/engine/textTypes";
import type { TextEditSession } from "../../tools/editorState";

describe("useLayerActions wiring", () => {
  function createWrapper() {
    const ws = new WorkspaceManager();
    ws.addDocument(WorkspaceManager.createBlankDocument("doc-a", "DocA", 800, 600));
    ws.switchDocument("doc-a");
    const engine = ws.getEngine("doc-a")!;
    const history = ws.getHistory("doc-a")!;
    const renderer = makeMockRenderer();
    const scheduler = { requestRender: vi.fn() };

    const wrapper = (props: { children: any }) => (
      <DialogProvider>
        <EditorProvider workspace={ws} renderer={renderer as any} scheduler={scheduler as any}>
          {props.children}
        </EditorProvider>
      </DialogProvider>
    );

    return { ws, engine, history, renderer, scheduler, wrapper };
  }

  it("handleAddLayer creates a new layer and commits history", () => {
    const { engine, history, wrapper } = createWrapper();
    const { result } = renderHook(() => useLayerActions(), { wrapper });

    const beforeCount = engine.getLayers().length;
    result.handleAddLayer();

    expect(engine.getLayers()).toHaveLength(beforeCount + 1);
    expect(history.getUndoCount()).toBe(1);
  });

  it("handleDuplicateActiveLayer duplicates the active layer", () => {
    const { engine, history, renderer, wrapper } = createWrapper();
    engine.addLayer("Source", 100, 100);
    const src = engine.getLayers().find(l => l.name === "Source")!;
    engine.setActiveLayer(src.id);
    engine.setLayerImageBitmap(src.id, makeBitmap());

    const { result } = renderHook(() => useLayerActions(), { wrapper });

    result.handleDuplicateActiveLayer();

    const layers = engine.getLayers();
    const dup = layers.find(l => l.name === "Source 2");
    expect(dup).toBeDefined();
    expect(history.getUndoCount()).toBe(1);
    // Duplicated layer bitmap was uploaded
    expect(renderer.uploadImage).toHaveBeenCalledWith(dup!.id, dup!.imageBitmap);
  });

  it("handleSelectLayer calls setActiveLayer", () => {
    const { engine, wrapper } = createWrapper();
    engine.addLayer("Target", 100, 100);
    const target = engine.getLayers().find(l => l.name === "Target")!;

    const { result } = renderHook(() => useLayerActions(), { wrapper });

    result.handleSelectLayer(target.id);

    expect(engine.getActiveLayerId()).toBe(target.id);
  });

  it("handleMoveUp and handleMoveDown reorder layers", () => {
    const { engine, history, scheduler, wrapper } = createWrapper();
    // addLayer inserts BEFORE the active layer. Add Top first, then Bottom
    // so Top ends up at index 1 (below Bottom), making handleMoveUp valid.
    engine.addLayer("Top", 100, 100);
    engine.addLayer("Bottom", 100, 100);
    const layers = engine.getLayers();
    const topIdx = layers.findIndex(l => l.name === "Top");

    const { result } = renderHook(() => useLayerActions(), { wrapper });

    result.handleMoveUp({ stopPropagation: vi.fn() } as any, topIdx);

    // The top layer should now be above the bottom layer
    const afterUp = engine.getLayers();
    expect(afterUp[0].name).toBe("Top");
    expect(history.getUndoCount()).toBe(1);
    expect(scheduler.requestRender).toHaveBeenCalled();
  });

  it("handleMergeActiveLayerDown calls the underlying merge function", () => {
    const { engine, history, renderer, scheduler, wrapper } = createWrapper();
    const top = engine.addLayer("Top", 100, 100);
    const bottom = engine.addLayer("Bottom", 100, 100);
    engine.setActiveLayer(top.id);
    engine.setLayerImageBitmap(top.id, makeBitmap());
    engine.setLayerImageBitmap(bottom.id, makeBitmap());

    const { result } = renderHook(() => useLayerActions(), { wrapper });

    result.handleMergeActiveLayerDown();

    expect(history.getUndoCount()).toBe(1);
    expect(scheduler.requestRender).toHaveBeenCalled();
  });

  it("handleDuplicateActiveLayer with no active layer shows a warning toast (no silent no-op)", () => {
    const { engine, wrapper } = createWrapper();
    engine.setActiveLayer(null);
    const toastSpy = vi.spyOn(Toast, "showToast");

    const { result } = renderHook(() => useLayerActions(), { wrapper });
    result.handleDuplicateActiveLayer();

    expect(toastSpy).toHaveBeenCalledWith("No layer selected", "warn");
  });

  it("handleFlattenAllLayers with a single layer shows a warning toast (no silent no-op)", () => {
    const { engine, wrapper } = createWrapper();
    const toastSpy = vi.spyOn(Toast, "showToast");

    const { result } = renderHook(() => useLayerActions(), { wrapper });
    result.handleFlattenAllLayers();

    expect(toastSpy).toHaveBeenCalledWith("Could not flatten layers", "warn");
  });

  // ── Text session lifecycle (B6/B9) ───────────────────────────────────────

  function createTextProbe() {
    const ctx = createWrapper();
    const { result } = renderHook(() => {
      const editor = useEditor();
      const actions = useLayerActions();
      return {
        ...actions,
        openSession: (s: TextEditSession) => editor.setTextEditSession(s),
        session: () => editor.textEditSession(),
      };
    }, { wrapper: ctx.wrapper });
    return { ...ctx, result };
  }

  const textData = (content: string): TextData => ({
    content, fontFamily: "Arial", fontSize: 48, fontWeight: 400, fontStyle: "normal",
    color: "#000000", align: "left", lineHeight: 1.4, letterSpacing: 0,
    boxMode: "point", boxWidth: 0, stroke: { width: 0, color: "#000000" },
  });

  // Build a text layer WITHOUT the rasterizer — this file's OffscreenCanvas
  // polyfill lacks measureText/fillText, and the wiring test only cares about
  // the session lifecycle, not glyph rasterization.
  const makeTextLayer = (engine: DocumentEngine, name: string) => {
    const layer = engine.addLayer(name, 100, 100);
    (layer as unknown as { type: string }).type = "text";
    (layer as unknown as { textData: TextData }).textData = textData("Hello");
    return layer;
  };

  it("delete while a re-edit session is open closes the session — one 'Delete Layer' step, no resurrection (B6)", () => {
    const { engine, history, result } = createTextProbe();
    const layer = engine.addLayer("Text 1", 100, 100);
    engine.setActiveLayer(layer.id);
    result.openSession({ layerId: layer.id, docX: 0, docY: 0, boxMode: "point", boxWidth: 0, isNewLayer: false, preSnapshot: engine.snapshot() });

    result.handleDeleteActiveLayer();

    expect(result.session()).toBeNull();
    expect(engine.getLayer(layer.id)).toBeUndefined();
    expect(history.getUndoCount()).toBe(1); // exactly "Delete Layer" — no ghost "Edit Text"
  });

  it("delete while a TEMP text session is open just cancels it — no history entry (B6)", () => {
    const { engine, history, result } = createTextProbe();
    const temp = engine.addLayer("Text", 100, 100);
    engine.setActiveLayer(temp.id);
    result.openSession({ layerId: temp.id, docX: 0, docY: 0, boxMode: "point", boxWidth: 0, isNewLayer: true, preSnapshot: engine.snapshot() });

    result.handleDeleteActiveLayer();

    expect(result.session()).toBeNull();
    expect(engine.getLayer(temp.id)).toBeUndefined();
    expect(history.getUndoCount()).toBe(0); // no ghost "Delete Layer"
  });

  it("selecting a DIFFERENT layer commits the open session (click-away pattern) (B9)", () => {
    const { engine, history, result } = createTextProbe();
    const text = makeTextLayer(engine, "Text");
    const other = engine.addLayer("Other", 100, 100);
    engine.setActiveLayer(text.id);
    result.openSession({ layerId: text.id, docX: 0, docY: 0, boxMode: "point", boxWidth: 0, isNewLayer: false, preSnapshot: engine.snapshot() });
    // Simulate typing: the live textData now differs from the session snapshot.
    engine.getLayer(text.id)!.textData!.content = "Hello world";

    result.handleSelectLayer(other.id);

    expect(history.getUndoCount()).toBe(1); // "Edit Text"
    expect(result.session()).toBeNull();
    expect(engine.getActiveLayerId()).toBe(other.id);
  });

  it("selecting the session's OWN layer keeps the session open (re-edit flow) (B9)", () => {
    const { engine, history, result } = createTextProbe();
    const text = makeTextLayer(engine, "Text");
    engine.setActiveLayer(text.id);
    result.openSession({ layerId: text.id, docX: 0, docY: 0, boxMode: "point", boxWidth: 0, isNewLayer: false, preSnapshot: engine.snapshot() });

    result.handleSelectLayer(text.id);

    expect(history.getUndoCount()).toBe(0);
    expect(result.session()).not.toBeNull();
  });

  // ── Merge / flatten vs. an open text session (B6-adjacent) ───────────────

  it("handleMergeActiveLayerDown commits an open session on the active layer first (typed text survives) (B6-adjacent)", () => {
    const { engine, history, result } = createTextProbe();
    const below = engine.addLayer("Below", 100, 100);
    const text = makeTextLayer(engine, "Text"); // inserted above Below
    engine.setActiveLayer(text.id);
    result.openSession({ layerId: text.id, docX: 0, docY: 0, boxMode: "point", boxWidth: 0, isNewLayer: false, preSnapshot: engine.snapshot() });
    engine.getLayer(text.id)!.textData!.content = "Hello world"; // simulate typing

    result.handleMergeActiveLayerDown();

    // Session closed, the layer was merged away (mergeDown consumes BOTH the
    // top and the bottom into a NEW merged id), and both the edit and the
    // merge are undoable — the typed text is not silently dropped.
    expect(result.session()).toBeNull();
    expect(engine.getLayer(text.id)).toBeUndefined();
    expect(engine.getLayer(below.id)).toBeUndefined();
    expect(engine.getLayers()).toHaveLength(2); // [merged, bg]
    expect(history.getUndoCount()).toBe(2); // "Edit Text" + "Merge Down"
  });

  it("handleMergeActiveLayerDown with an empty TEMP session bails — no unintended merge (B6-adjacent)", () => {
    const { engine, history, result } = createTextProbe();
    const below = engine.addLayer("Below", 100, 100);
    const temp = engine.addLayer("Temp", 100, 100);
    (temp as unknown as { type: string }).type = "text";
    (temp as unknown as { textData: TextData }).textData = textData(""); // empty temp
    engine.setActiveLayer(temp.id);
    result.openSession({ layerId: temp.id, docX: 0, docY: 0, boxMode: "point", boxWidth: 0, isNewLayer: true, preSnapshot: engine.snapshot() });

    result.handleMergeActiveLayerDown();

    // The empty temp is removed by the session commit's empty-commit cleanup;
    // the merge is moot and must NOT merge the layer that moved up into the
    // active slot.
    expect(result.session()).toBeNull();
    expect(engine.getLayer(temp.id)).toBeUndefined();
    expect(engine.getLayer(below.id)).toBeDefined();
    expect(history.getUndoCount()).toBe(0);
  });

  it("handleFlattenAllLayers commits an open text session first (typed text survives flatten) (B6-adjacent)", () => {
    const { engine, history, result } = createTextProbe();
    const text = makeTextLayer(engine, "Text"); // [Text, bg]
    engine.setActiveLayer(text.id);
    result.openSession({ layerId: text.id, docX: 0, docY: 0, boxMode: "point", boxWidth: 0, isNewLayer: false, preSnapshot: engine.snapshot() });
    engine.getLayer(text.id)!.textData!.content = "Hello world"; // simulate typing

    result.handleFlattenAllLayers();

    expect(result.session()).toBeNull();
    expect(engine.getLayers()).toHaveLength(1);
    expect(history.getUndoCount()).toBe(2); // "Edit Text" + "Flatten Image"
  });

  it("handleMergeActiveLayerDown commits a session on the merge TARGET (the layer below the active) too (B6-adjacent)", () => {
    const { engine, history, result } = createTextProbe();
    const target = makeTextLayer(engine, "Target"); // [Target, bg]
    const active = engine.addLayer("Active", 100, 100); // [Active, Target, bg]
    engine.setActiveLayer(active.id);
    result.openSession({ layerId: target.id, docX: 0, docY: 0, boxMode: "point", boxWidth: 0, isNewLayer: false, preSnapshot: engine.snapshot() });
    engine.getLayer(target.id)!.textData!.content = "Hello world"; // simulate typing

    result.handleMergeActiveLayerDown();

    // mergeDown consumes BOTH Active and Target — the session on Target must
    // be committed, not left dangling over a consumed layer.
    expect(result.session()).toBeNull();
    expect(engine.getLayer(target.id)).toBeUndefined();
    expect(engine.getLayer(active.id)).toBeUndefined();
    expect(engine.getLayers()).toHaveLength(2); // [merged, bg]
    expect(history.getUndoCount()).toBe(2); // "Edit Text" + "Merge Down"
  });

  it("handleMergeActiveLayerDown leaves a session on an UNRELATED layer open (guard no-op) (B6-adjacent)", () => {
    const { engine, history, result } = createTextProbe();
    const below = engine.addLayer("Below", 100, 100);
    const target = engine.addLayer("Target", 100, 100);
    const active = engine.addLayer("Active", 100, 100);
    const text = makeTextLayer(engine, "Text"); // [Text, Active, Target, Below, bg]
    engine.setActiveLayer(active.id);
    result.openSession({ layerId: text.id, docX: 0, docY: 0, boxMode: "point", boxWidth: 0, isNewLayer: false, preSnapshot: engine.snapshot() });
    engine.getLayer(text.id)!.textData!.content = "Hello world";

    result.handleMergeActiveLayerDown();

    // The session layer is neither the active layer nor the merge target —
    // the merge proceeds and the session stays open untouched.
    expect(result.session()).not.toBeNull();
    expect(engine.getLayer(text.id)).toBeDefined();
    expect(history.getUndoCount()).toBe(1); // only "Merge Down"
  });
});
