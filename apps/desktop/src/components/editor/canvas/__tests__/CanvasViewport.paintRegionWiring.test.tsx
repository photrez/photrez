import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import { ImageData as NodeImageData } from "canvas";
import { EditorProvider, useEditor } from "../../shell/EditorContext";
import { CanvasViewport } from "../CanvasViewport";
import { WorkspaceManager } from "@/engine/workspace";
import { flushC4Commits } from "../../useBrushOverlay";

// ── jsdom environment shims (jsdom has no ImageData / OffscreenCanvas /
//    createImageBitmap; node-canvas backs <canvas>.getContext("2d"), so 2D
//    drawing, getImageData and putImageData are REAL in this file) ──────────

// Local FakeImageData: jsdom provides no ImageData constructor, while the real
// node-canvas 2D context rejects any putImageData argument that is not an
// instance of its own ImageData class. So the local fake extends that class
// (instances stay accepted by real putImageData) and widens the constructor to
// any ArrayLike<number>, which is how the Rust tile payloads arrive.
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
  // Constructor returning a real <canvas> element: production drawImage() call
  // sites receive a source node-canvas accepts, and the element carries the
  // two OffscreenCanvas-only methods the commit path touches.
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

// ── Render/viewport boundaries (allowed mocks). useBrushOverlay,
//    useCanvasPointerTools, applyPaintBucketFill and regionProducer stay REAL.
//    Tauri invoke is replaced by an in-memory Rust pixel-store emulator below.
const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args: unknown) => invokeMock(cmd, args),
}));
vi.mock("../useViewportRenderer", () => ({
  useViewportRenderer: () => ({
    isFitTransition: () => false,
    fitToScreenAndRender: vi.fn(),
    resizeRenderer: vi.fn(),
  }),
}));
vi.mock("../usePanNavigation", () => ({
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
vi.mock("../useCanvasDerivedState", () => ({
  useCanvasDerivedState: () => ({ cropSnapTargets: () => [] }),
}));
vi.mock("../useCanvasKeyboard", () => ({
  useCanvasKeyboard: vi.fn(),
}));

const DOC = 256;

// ── In-memory Rust pixel store behind the invoke mock ──────────────────────
// Mirrors the real command contract: rust_pixels_write_region rejects a
// rgba length that does not equal w*h*4 and a region that leaves the layer
// (crates/core/src/pixel_store.rs), Tauri v2 surfaces those errors as bare
// string rejections, and every successful write bumps the store epoch.
type StoreTile = { x: number; y: number; w: number; h: number; data: number[] };
type StoredWrite = { x: number; y: number; w: number; h: number; rgba: Uint8Array };

function createStore(width: number, height: number) {
  const buffer = new Uint8ClampedArray(width * height * 4);
  const writes: StoredWrite[] = [];
  const rejections: string[] = [];
  let initCalls = 0;
  let epoch = 0;
  // Undo/redo stacks of whole-layer snapshots. Every successful region write is
  // ONE history entry, so the depth of this stack is the number of cursor steps
  // a user has to walk back - the same accounting the Rust pixel store keeps.
  const undoStack: Uint8ClampedArray[] = [];
  const redoStack: Uint8ClampedArray[] = [];
  // When set, rust_pixels_get_epoch rejects with this message instead of
  // returning the store epoch: Tauri v2 surfaces a Rust Err("layer not
  // initialized") as a bare-string rejection, which is the signal the
  // production seed path keys off (getRustEpoch -> null -> rust_pixels_init).
  let epochRejection: string | null = null;

  const readTile = (x: number, y: number, w: number, h: number): StoreTile => {
    const data = new Array<number>(w * h * 4);
    for (let row = 0; row < h; row++) {
      const src = ((y + row) * width + x) * 4;
      for (let i = 0; i < w * 4; i++) data[row * w * 4 + i] = buffer[src + i];
    }
    return { x, y, w, h, data };
  };

  invokeMock.mockImplementation(async (cmd: string, args: Record<string, unknown>) => {
    if (cmd === "rust_pixels_get_epoch") {
      if (epochRejection !== null) return Promise.reject(epochRejection);
      return epoch;
    }
    if (cmd === "rust_pixels_init") {
      initCalls += 1;
      const bytes = args.bytes as Uint8Array;
      buffer.set(bytes.subarray(0, buffer.length));
      return null;
    }
    if (cmd === "rust_pixels_snapshot_layer") return [readTile(0, 0, width, height)];
    if (cmd === "rust_pixels_write_region") {
      const x = args.x as number;
      const y = args.y as number;
      const w = args.w as number;
      const h = args.h as number;
      const rgba = args.rgba as Uint8Array;
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
      const before = readTile(x, y, w, h);
      undoStack.push(buffer.slice());
      redoStack.length = 0; // a new entry truncates redo, like the Rust store
      for (let row = 0; row < h; row++) {
        const dst = ((y + row) * width + x) * 4;
        buffer.set(rgba.subarray(row * w * 4, (row + 1) * w * 4), dst);
      }
      const after = readTile(x, y, w, h);
      writes.push({ x, y, w, h, rgba });
      epoch += 1;
      return { before: [before], after: [after], epoch, version: epoch };
    }
    if (cmd === "rust_pixels_undo" || cmd === "rust_pixels_redo") {
      const from = cmd === "rust_pixels_undo" ? undoStack : redoStack;
      const to = cmd === "rust_pixels_undo" ? redoStack : undoStack;
      const entry = from.pop();
      if (!entry) return { tiles: [], epoch, version: epoch, layerId: args.layerId };
      to.push(buffer.slice());
      buffer.set(entry);
      epoch += 1;
      return { tiles: [readTile(0, 0, width, height)], epoch, version: epoch, layerId: args.layerId };
    }
    return undefined;
  });

  return {
    writes,
    rejections,
    seed: (pixels: Uint8ClampedArray) => buffer.set(pixels.subarray(0, buffer.length)),
    initCalls: () => initCalls,
    epoch: () => epoch,
    rejectEpoch: (message: string | null) => { epochRejection = message; },
    /** Pixel-entry count: how many undo steps the cursor currently holds. */
    undoDepth: () => undoStack.length,
    /** Whole-layer bytes, for comparing pre/post states across an undo. */
    snapshot: () => buffer.slice(),
  };
}

function makeCanvas(w: number, h: number, draw: (ctx: CanvasRenderingContext2D) => void) {
  const canvas = document.createElement("canvas");
  canvas.width = w;
  canvas.height = h;
  const ctx = canvas.getContext("2d") as CanvasRenderingContext2D;
  draw(ctx);
  return canvas;
}

function containsPixel(rgba: ArrayLike<number>, match: (r: number, g: number, b: number, a: number) => boolean): boolean {
  for (let i = 0; i + 3 < rgba.length; i += 4) {
    if (match(rgba[i], rgba[i + 1], rgba[i + 2], rgba[i + 3])) return true;
  }
  return false;
}

let setTool: (tool: string) => void = () => {};
let setFgColor: (color: string) => void = () => {};
let setZoomState: (zoom: number) => void = () => {};
let setPanState: (pan: { x: number; y: number }) => void = () => {};

const TestConsumer = () => {
  const editor = useEditor();
  setTool = editor.setActiveTool;
  setFgColor = editor.setFgColor;
  setZoomState = editor.setZoom;
  setPanState = editor.setPan;
  return null;
};

describe("CanvasViewport paint-region wiring (real brush + paint-bucket pointer chains)", () => {
  let ws: WorkspaceManager;
  // Partial renderer/scheduler doubles, typed permissively exactly as
  // CanvasViewport.test.tsx does: EditorProvider's props require the full
  // WebGL2Backend/RenderScheduler surface, which these tests never touch.
  let renderer: any;
  let scheduler: any;
  let container: HTMLDivElement;
  let dispose: () => void;
  let rectStub: ReturnType<typeof vi.spyOn> | undefined;
  let store: ReturnType<typeof createStore>;

  beforeEach(() => {
    ws = new WorkspaceManager();
    renderer = { uploadImage: vi.fn(), destroyTexture: vi.fn(), uploadSurfaceTiles: vi.fn() };
    scheduler = { requestRender: vi.fn() };
    container = document.createElement("div");
    document.body.appendChild(container);
    invokeMock.mockReset();
    store = createStore(DOC, DOC);
    // jsdom has no layout: one fixed rect keeps client -> document coords 1:1
    // (getDocCoords = (client - rect.left - pan) / zoom).
    rectStub = vi.spyOn(HTMLElement.prototype, "getBoundingClientRect").mockReturnValue({
      left: 0, top: 0, right: DOC, bottom: DOC, width: DOC, height: DOC, x: 0, y: 0,
      toJSON: () => ({}),
    } as DOMRect);
    Element.prototype.setPointerCapture = vi.fn();
    Element.prototype.releasePointerCapture = vi.fn();
    localStorage.setItem("photrez.rustPixels", "1");
    localStorage.removeItem("photrez.canonicalCommit");
    localStorage.removeItem("photrez.tileCommit");
  });

  afterEach(() => {
    if (dispose) dispose();
    container.parentNode?.removeChild(container);
    rectStub?.mockRestore();
    localStorage.removeItem("photrez.rustPixels");
    vi.restoreAllMocks();
  });

  // Pixels the Rust store (and the layer bitmap) start with; assigned in each
  // test so the store seed and the derived surface agree byte for byte.
  function renderViewport() {
    const session = WorkspaceManager.createBlankDocument("doc-region", "Region", DOC, DOC);
    ws.addDocument(session);
    const result = render(
      () => (
        <EditorProvider workspace={ws} renderer={renderer} scheduler={scheduler}>
          <TestConsumer />
          <CanvasViewport />
        </EditorProvider>
      ),
      container,
    );
    dispose = result;
    return { session };
  }

  function getCanvas(): HTMLCanvasElement {
    const c = container.querySelector("canvas:not([data-overlay-canvas])") as HTMLCanvasElement | null;
    if (!c) throw new Error("viewport canvas not found");
    return c;
  }

  function fire(type: string, el: Element, clientX: number, clientY: number, pointerId = 10) {
    el.dispatchEvent(
      new PointerEvent(type, {
        bubbles: true, cancelable: true, button: 0, pointerId, clientX, clientY,
      }),
    );
  }

  async function tick(ms = 0) {
    await new Promise((resolve) => setTimeout(resolve, ms));
  }

  /** Paint layer + matching store seed, then install the cached paint surface. */
  function prepareLayer(
    session: ReturnType<typeof WorkspaceManager.createBlankDocument>,
    pixels: Uint8ClampedArray,
    draw: (ctx: CanvasRenderingContext2D) => void,
  ) {
    const layerId = session.engine.getLayers()[0].id;
    const bitmap = makeCanvas(DOC, DOC, draw);
    session.engine.setLayerImageBitmap(layerId, bitmap as unknown as ImageBitmap);
    session.engine.setActiveLayer(layerId);
    store.seed(pixels);
    const surface = session.engine.getPaintSurface(layerId);
    if (!surface) throw new Error("paint surface not created (layer bitmap missing?)");
    return { layerId, surface };
  }

  it("brush pointerup commits exactly one bounded region write through the shared producer", async () => {
    const { session } = renderViewport();
    await tick();
    setZoomState(1);
    setPanState({ x: 0, y: 0 });
    setTool("brush");
    setFgColor("#ff0000");

    const white = makeCanvas(DOC, DOC, (ctx) => {
      ctx.fillStyle = "#ffffff";
      ctx.fillRect(0, 0, DOC, DOC);
    });
    const pixels = white.getContext("2d")!.getImageData(0, 0, DOC, DOC).data;
    const { layerId, surface } = prepareLayer(session, pixels, (ctx) => {
      ctx.fillStyle = "#ffffff";
      ctx.fillRect(0, 0, DOC, DOC);
    });
    const readSpy = vi.spyOn(surface, "readRect");
    expect(session.engine.getPaintSurface(layerId)).toBe(surface);

    const canvas = getCanvas();
    fire("pointerdown", canvas, 40, 40);
    fire("pointermove", canvas, 70, 60);
    fire("pointermove", canvas, 100, 80);
    fire("pointerup", canvas, 100, 80);
    await flushC4Commits();
    await vi.waitFor(() => expect(store.writes.length).toBe(1), { timeout: 3000 });

    expect(store.rejections).toEqual([]);
    // No seed read: the store already has the layer, so the commit never does a
    // full-layer readRect(0,0,w,h) and never calls rust_pixels_init.
    expect(store.initCalls()).toBe(0);
    expect(readSpy).toHaveBeenCalledTimes(1);
    const [rx, ry, rw, rh] = readSpy.mock.calls[0];
    expect(store.writes[0]).toMatchObject({ x: rx, y: ry, w: rw, h: rh });
    // Bounded: the shared producer handed the stroke dirty rect, not the layer.
    expect(rw * rh).toBeLessThan(DOC * DOC);
    expect(rx).toBeGreaterThanOrEqual(0);
    expect(ry).toBeGreaterThanOrEqual(0);
    expect(rx + rw).toBeLessThanOrEqual(DOC);
    expect(ry + rh).toBeLessThanOrEqual(DOC);
    expect(readSpy.mock.calls.some(([x, y, w, h]) => x === 0 && y === 0 && w === DOC && h === DOC)).toBe(false);
    // Payload is real bytes AND carries the stroke (red over the white seed).
    const payload = store.writes[0].rgba;
    expect(payload.length).toBe(rw * rh * 4);
    expect(containsPixel(payload, (r, g, b, a) => a === 255 && (r !== 255 || g !== 255 || b !== 255))).toBe(true);
    expect(store.epoch()).toBe(1);
  });

  it("paint-bucket pointer chain writes exactly one bounded region (down+move+up = one fill)", async () => {
    const { session } = renderViewport();
    await tick();
    setZoomState(1);
    setPanState({ x: 0, y: 0 });
    setTool("paintBucket");
    setFgColor("#ff0000");

    // Left half black, right half white: a click inside the white half must
    // flood-fill only that half, so the shipped region stays sub-layer.
    const drawHalves = (ctx: CanvasRenderingContext2D) => {
      ctx.fillStyle = "#000000";
      ctx.fillRect(0, 0, DOC / 2, DOC);
      ctx.fillStyle = "#ffffff";
      ctx.fillRect(DOC / 2, 0, DOC / 2, DOC);
    };
    const halves = makeCanvas(DOC, DOC, drawHalves);
    const pixels = halves.getContext("2d")!.getImageData(0, 0, DOC, DOC).data;
    const { surface } = prepareLayer(session, pixels, drawHalves);
    const readSpy = vi.spyOn(surface, "readRect");

    const canvas = getCanvas();
    fire("pointerdown", canvas, 200, 128);
    fire("pointermove", canvas, 210, 132);
    fire("pointerup", canvas, 210, 132);
    await vi.waitFor(() => expect(store.writes.length).toBe(1), { timeout: 3000 });
    await tick(50);

    // down + move + up produced ONE fill entry, not one per event.
    expect(store.writes.length).toBe(1);
    expect(store.rejections).toEqual([]);
    expect(store.initCalls()).toBe(0);
    const w0 = store.writes[0];
    expect(w0.rgba.length).toBe(w0.w * w0.h * 4);
    expect(w0.w * w0.h).toBeLessThan(DOC * DOC);
    expect(w0.x).toBeGreaterThanOrEqual(0);
    expect(w0.y).toBeGreaterThanOrEqual(0);
    expect(w0.x + w0.w).toBeLessThanOrEqual(DOC);
    expect(w0.y + w0.h).toBeLessThanOrEqual(DOC);
    // The bucket sources pixels from the store snapshot, so it must not read
    // the derived surface rect at all (the brush is the only readRect caller).
    expect(readSpy).not.toHaveBeenCalled();
    // Nonzero payload: the red fill bytes reached the store.
    expect(containsPixel(w0.rgba, (r, g, b, a) => a === 255 && r === 255 && g === 0 && b === 0)).toBe(true);
    expect(store.epoch()).toBe(1);
  });

  it("brush commit seeds an absent store from one full-layer read when the epoch probe rejects", async () => {
    const { session } = renderViewport();
    await tick();
    setZoomState(1);
    setPanState({ x: 0, y: 0 });
    setTool("brush");
    setFgColor("#ff0000");

    const white = makeCanvas(DOC, DOC, (ctx) => {
      ctx.fillStyle = "#ffffff";
      ctx.fillRect(0, 0, DOC, DOC);
    });
    const pixels = white.getContext("2d")!.getImageData(0, 0, DOC, DOC).data;
    const { layerId, surface } = prepareLayer(session, pixels, (ctx) => {
      ctx.fillStyle = "#ffffff";
      ctx.fillRect(0, 0, DOC, DOC);
    });
    const readSpy = vi.spyOn(surface, "readRect");
    // The spy must sit on the surface production reads from, otherwise the
    // full-layer read assertion below would count nothing.
    expect(session.engine.getPaintSurface(layerId)).toBe(surface);
    // First raster op on this layer: Rust reports no entry for it, exactly as
    // `Err("layer not initialized")` arrives from Tauri v2 (bare-string reject).
    store.rejectEpoch("layer not initialized");

    const canvas = getCanvas();
    fire("pointerdown", canvas, 40, 40);
    fire("pointermove", canvas, 70, 60);
    fire("pointerup", canvas, 70, 60);
    await flushC4Commits();
    await vi.waitFor(() => expect(store.writes.length).toBe(1), { timeout: 3000 });

    expect(store.rejections).toEqual([]);
    // Seed path fired exactly once: the derived surface pixels were pushed into
    // Rust before the stroke write, instead of the store staying unseeded.
    expect(store.initCalls()).toBe(1);
    const fullLayerReads = readSpy.mock.calls.filter(
      ([x, y, w, h]) => x === 0 && y === 0 && w === DOC && h === DOC,
    );
    expect(fullLayerReads).toHaveLength(1);
    const dirtyReads = readSpy.mock.calls.filter(
      ([x, y, w, h]) => !(x === 0 && y === 0 && w === DOC && h === DOC),
    );
    expect(dirtyReads).toHaveLength(1);
    // The stroke write still ships the bounded dirty rect, not the layer.
    expect(dirtyReads[0][2] * dirtyReads[0][3]).toBeLessThan(DOC * DOC);
    const w0 = store.writes[0];
    expect(w0.rgba.length).toBe(w0.w * w0.h * 4);
    expect(containsPixel(w0.rgba, (r, g, b, a) => a === 255 && (r !== 255 || g !== 255 || b !== 255))).toBe(true);
    expect(store.epoch()).toBe(1);
  });

  it("paint-bucket fill under an active selection ships the selection-clipped region", async () => {
    const { session } = renderViewport();
    await tick();
    setZoomState(1);
    setPanState({ x: 0, y: 0 });
    setTool("paintBucket");
    setFgColor("#ff0000");

    // Uniform white layer: without the selection the contiguous fill would
    // change every pixel, so the unbounded fill bbox is the whole layer.
    const white = makeCanvas(DOC, DOC, (ctx) => {
      ctx.fillStyle = "#ffffff";
      ctx.fillRect(0, 0, DOC, DOC);
    });
    const pixels = white.getContext("2d")!.getImageData(0, 0, DOC, DOC).data;
    const { surface } = prepareLayer(session, pixels, (ctx) => {
      ctx.fillStyle = "#ffffff";
      ctx.fillRect(0, 0, DOC, DOC);
    });
    const readSpy = vi.spyOn(surface, "readRect");

    const SEL = { x: 64, y: 64, w: 128, h: 128 };
    session.engine.createSelection(SEL.x, SEL.y, SEL.w, SEL.h);

    const canvas = getCanvas();
    // Click inside the selection so the masked fill has work to do.
    fire("pointerdown", canvas, 100, 100);
    fire("pointermove", canvas, 104, 104);
    fire("pointerup", canvas, 104, 104);
    await vi.waitFor(() => expect(store.writes.length).toBe(1), { timeout: 3000 });
    await tick(50);

    expect(store.writes.length).toBe(1);
    expect(store.rejections).toEqual([]);
    const w0 = store.writes[0];
    // Region = intersection(unbounded fill bbox, selection rect). Shipping the
    // unbounded bbox instead would write outside the selection and carries a
    // payload whose length no longer matches the region, which the store rejects.
    expect(w0).toMatchObject({ x: SEL.x, y: SEL.y, w: SEL.w, h: SEL.h });
    expect(w0.w * w0.h).toBeLessThan(DOC * DOC);
    expect(w0.x).toBeGreaterThanOrEqual(SEL.x);
    expect(w0.y).toBeGreaterThanOrEqual(SEL.y);
    expect(w0.x + w0.w).toBeLessThanOrEqual(SEL.x + SEL.w);
    expect(w0.y + w0.h).toBeLessThanOrEqual(SEL.y + SEL.h);
    expect(w0.rgba.length).toBe(w0.w * w0.h * 4);
    expect(containsPixel(w0.rgba, (r, g, b, a) => a === 255 && r === 255 && g === 0 && b === 0)).toBe(true);
    expect(store.initCalls()).toBe(0);
    expect(readSpy).not.toHaveBeenCalled();
    expect(store.epoch()).toBe(1);
  });

  // An eraser stroke must reach Rust on the SAME single region commit the brush
  // uses. Before it did, the stroke committed to the TS history store only, so
  // the unified cursor held no entry for the erase at all: one Ctrl+Z stepped
  // past it onto the earlier brush entry and the erase stayed on screen.
  it("eraser pointerup ships exactly one region write, and ONE undo restores the painted stroke", async () => {
    const { session } = renderViewport();
    await tick();
    setZoomState(1);
    setPanState({ x: 0, y: 0 });

    const drawWhite = (ctx: CanvasRenderingContext2D) => {
      ctx.fillStyle = "#ffffff";
      ctx.fillRect(0, 0, DOC, DOC);
    };
    const white = makeCanvas(DOC, DOC, drawWhite);
    // A NON-background layer: on the document Background layer the eraser
    // paints the background swatch (resolveEraserFill -> isEraser:false), which
    // is the source-over path, not the transparent cut this test pins.
    const layerId = session.engine.addLayer("Paint").id;
    session.engine.setLayerImageBitmap(layerId, white as unknown as ImageBitmap);
    session.engine.setActiveLayer(layerId);
    store.seed(white.getContext("2d")!.getImageData(0, 0, DOC, DOC).data);
    if (!session.engine.getPaintSurface(layerId)) throw new Error("paint surface not created");
    const commitSpy = vi.spyOn(ws.getActiveHistory()!, "commit");

    const canvas = getCanvas();
    // down + 2 moves + up over one path; the eraser replays the SAME path so the
    // erase lands entirely on the stroke it has to undo.
    const path: [string, number, number][] = [
      ["pointerdown", 60, 60],
      ["pointermove", 100, 80],
      ["pointermove", 140, 100],
      ["pointerup", 140, 100],
    ];
    const replay = () => path.forEach(([type, x, y]) => fire(type, canvas, x, y));

    // 1) Paint. One region write, one cursor entry.
    setTool("brush");
    setFgColor("#ff0000");
    replay();
    await flushC4Commits();
    await vi.waitFor(() => expect(store.writes.length).toBe(1), { timeout: 3000 });
    const painted = store.snapshot();
    expect(containsPixel(painted, (r, g, b) => r === 255 && g === 0 && b === 0)).toBe(true);
    expect(store.undoDepth()).toBe(1);

    // 2) Erase the same path. The erase is ONE more region write - not zero
    //    (which strands the cursor before it) and not two (two owners).
    setTool("eraser");
    replay();
    await flushC4Commits();
    await vi.waitFor(() => expect(store.writes.length).toBe(2), { timeout: 3000 });
    await tick(50);

    expect(store.rejections).toEqual([]);
    const eraseWrite = store.writes[1];
    expect(eraseWrite.w * eraseWrite.h).toBeLessThan(DOC * DOC);
    expect(eraseWrite.rgba.length).toBe(eraseWrite.w * eraseWrite.h * 4);
    // The erase reached Rust as pixels: the stroke is punched transparent.
    expect(containsPixel(eraseWrite.rgba, (_r, _g, _b, a) => a < 255)).toBe(true);
    expect(containsPixel(store.snapshot(), (_r, _g, _b, a) => a < 255)).toBe(true);
    // The cursor advanced by exactly one entry for the erase.
    expect(store.undoDepth()).toBe(2);
    // One history commit per stroke, each flagged as already owned by Rust, so
    // the facade mirror never adds a second entry for the same stroke.
    expect(commitSpy).toHaveBeenCalledTimes(2);
    for (const call of commitSpy.mock.calls) expect(call[3]).toBe(true);
    // One owner: rust_pixels_write_region recorded the entries, so history
    // fired no second state-changing apply for either stroke.
    expect(invokeMock.mock.calls.filter((c) => c[0] === "apply_tile_patch")).toHaveLength(0);

    // 3) ONE undo. rust_pixels_undo is the command the history walker issues
    //    for a Pixel entry, so this is the pixel half of a single Ctrl+Z: it
    //    must land on the erase and bring the painted stroke back.
    await invokeMock("rust_pixels_undo", { docId: "doc-region", layerId });
    const afterOneUndo = store.snapshot();
    expect(Array.from(afterOneUndo)).toEqual(Array.from(painted));
    expect(store.undoDepth()).toBe(1);

    // The brush entry is still underneath, one step further back.
    await invokeMock("rust_pixels_undo", { docId: "doc-region", layerId });
    const afterTwoUndos = store.snapshot();
    expect(containsPixel(afterTwoUndos, (r, g, b, a) => a === 255 && r === 255 && g === 255 && b === 255)).toBe(true);
    expect(containsPixel(afterTwoUndos, (r, g) => r === 255 && g === 0)).toBe(false);
  });
});
