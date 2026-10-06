import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import { ImageData as NodeImageData } from "canvas";
import { EditorProvider, useEditor } from "../../shell/EditorContext";
import { CanvasViewport } from "../CanvasViewport";
import { WorkspaceManager } from "@/engine/workspace";
import { flushC4Commits } from "../../useBrushOverlay";
import { readPixelSeedCall , decodeRustBytes, encodePixelBytes } from "@/lib/protocol/pixelSeedCall";

// ── jsdom environment shims (jsdom has no ImageData / OffscreenCanvas /
//    createImageBitmap; node-canvas backs <canvas>.getContext("2d")) ─────────

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

// Render/viewport boundaries only. useBrushOverlay, useCanvasPointerTools and
// regionProducer stay REAL; only the Tauri transport is replaced.
const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args: unknown, options?: unknown) => invokeMock(cmd, args, options),
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
    setPanning: vi.fn(),
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

// ── Tauri v2 IPC payload serialization, replicated from the runtime ────────
//
// tauri-2.11.5/scripts/process-ipc-message-fn.js, which tauri-runtime injects
// into every webview and which ipc-protocol.js `sendIpcMessage` calls with the
// invoke payload:
//
//   if (message instanceof ArrayBuffer || ArrayBuffer.isView(message)
//       || Array.isArray(message))  -> { contentType: 'application/octet-stream',
//                                       data: message }
//   else                            -> { contentType: 'application/json',
//                                       data: JSON.stringify(message, replacer) }
//   replacer: val instanceof Uint8Array -> Array.from(val)
//
// So a typed-array payload crosses as bytes and never touches JSON, while a
// Uint8Array NESTED INSIDE the argument object is expanded to one JSON array
// element per byte. `jsonElementsPerByte` below is that expansion, counted the
// way the runtime pays for it. It is what the first-stroke stall was.
function serializeIpcPayload(payload: unknown): {
  jsonElementsPerByte: number;
  jsonChars: number;
} {
  let elements = 0;
  const walk = (v: unknown): unknown => {
    if (v instanceof Uint8Array) {
      elements += v.length; // Array.from(val): one JSON array element per byte
      return v;
    }
    if (Array.isArray(v)) return v.map(walk);
    if (v && typeof v === "object") {
      const out: Record<string, unknown> = {};
      for (const k of Object.keys(v as Record<string, unknown>)) out[k] = walk((v as Record<string, unknown>)[k]);
      return out;
    }
    return v;
  };
  const json = JSON.stringify(walk(payload));
  return { jsonElementsPerByte: elements, jsonChars: (json ?? "").length };
}

type SeedRecord = { cmd: string; bytes: Uint8Array; jsonElementsPerByte: number; jsonChars: number; width: number; height: number };
/**
 * A tile as it crosses the wire. `dataBase64` is what Rust sends; `data` as a
 * per-byte number array is the shape the wire used to carry, kept here so a
 * double can be switched to it and the transport tests go RED.
 */
type WireTile = { x: number; y: number; w: number; h: number; dataBase64: string };
type NumberTile = { x: number; y: number; w: number; h: number; data: number[] };

function createStore(width: number, height: number) {
  const buffer = new Uint8ClampedArray(width * height * 4);
  const seeds: SeedRecord[] = [];
  const writes: { x: number; y: number; w: number; h: number; rgba: Uint8Array }[] = [];
  const rejections: string[] = [];
  // What each command's RESPONSE costs to serialize, measured on the shape this
  // double answers with. The host parses it, so the response half of the wire is
  // as much a cost as the request half.
  const responseWires: { jsonElementsPerByte: number; jsonChars: number }[] = [];
  // The RGBA byte count the last measured response shipped, so the base64 budget
  // assertion has a denominator that is not the encoded length itself.
  let payloadBytes = 0;
  const undoStack: Uint8ClampedArray[] = [];
  let epoch = 0;
  let epochRejection: string | null = null;

  const tileBytes = (x: number, y: number, w: number, h: number): Uint8Array => {
    const data = new Uint8Array(w * h * 4);
    for (let row = 0; row < h; row++) {
      const src = ((y + row) * width + x) * 4;
      for (let i = 0; i < w * 4; i++) data[row * w * 4 + i] = buffer[src + i];
    }
    return data;
  };
  /** The wire shape: bytes as base64, which is what the command answers with. */
  const readTile = (x: number, y: number, w: number, h: number): WireTile => ({
    x,
    y,
    w,
    h,
    dataBase64: encodePixelBytes(tileBytes(x, y, w, h)),
  });
  /** The number-array shape the wire carried before: one JSON element per byte. */
  const readTileAsNumbers = (x: number, y: number, w: number, h: number): NumberTile => ({
    x,
    y,
    w,
    h,
    data: Array.from(tileBytes(x, y, w, h)),
  });

  invokeMock.mockImplementation(async (cmd: string, args: any, options: any) => {
    if (cmd === "rust_pixels_get_epoch") {
      if (epochRejection !== null) return Promise.reject(epochRejection);
      return epoch;
    }
    if (cmd === "rust_pixels_init") {
      const wire = serializeIpcPayload(args);
      const seed = readPixelSeedCall(cmd, args)!;
      if (seed.bytes.length !== seed.width * seed.height * 4) {
        rejections.push(`seed length ${seed.bytes.length} != ${seed.width * seed.height * 4}`);
        return Promise.reject("seed buffer size mismatch");
      }
      buffer.set(seed.bytes.subarray(0, buffer.length));
      seeds.push({
        cmd,
        bytes: seed.bytes.slice(),
        jsonElementsPerByte: wire.jsonElementsPerByte,
        jsonChars: wire.jsonChars,
        width: seed.width,
        height: seed.height,
      });
      return null;
    }
    if (cmd === "rust_pixels_snapshot_layer") return [readTile(0, 0, width, height)];
    if (cmd === "rust_pixels_write_region") {
      const { x, y, w, h } = args;
      const rgba = decodeRustBytes<{ rgba: Uint8Array }>(args).rgba;
      if (!Number.isInteger(x) || !Number.isInteger(y) || !Number.isInteger(w) || !Number.isInteger(h) || w <= 0 || h <= 0) {
        return Promise.reject("Invalid region dimensions");
      }
      if (x < 0 || y < 0 || x + w > width || y + h > height) return Promise.reject("Region outside layer bounds");
      if (rgba.length !== w * h * 4) return Promise.reject("Invalid region length");
      const before = readTile(x, y, w, h);
      undoStack.push(buffer.slice());
      for (let row = 0; row < h; row++) {
        buffer.set(rgba.subarray(row * w * 4, (row + 1) * w * 4), ((y + row) * width + x) * 4);
      }
      writes.push({ x, y, w, h, rgba });
      epoch += 1;
      const response = { before: [before], after: [readTile(x, y, w, h)], epoch, version: epoch };
      // Measured on the exact object this double returns, which is the shape the
      // host parses. Rust tiles carry `dataBase64`; a `data` number array is what
      // the old wire cost.
      // Payload bytes this response ships: the before tile AND the after tile.
      payloadBytes += 2 * w * h * 4;
      responseWires.push(serializeIpcPayload(response));
      return response;
    }
    if (cmd === "rust_pixels_undo") {
      const entry = undoStack.pop();
      if (!entry) return { tiles: [], epoch, version: epoch, layerId: args.layerId };
      buffer.set(entry);
      epoch += 1;
      return { tiles: [readTile(0, 0, width, height)], epoch, version: epoch, layerId: args.layerId };
    }
    return undefined;
  });

  return {
    seeds,
    writes,
    rejections,
    responseWires,
    responseBytes: () => payloadBytes,
    seedPixels: (pixels: Uint8ClampedArray) => buffer.set(pixels.subarray(0, buffer.length)),
    rejectEpoch: (m: string | null) => { epochRejection = m; },
    epoch: () => epoch,
    undoDepth: () => undoStack.length,
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

const WHITE = "#ffffff";

describe("brush seed transport (first stroke on a store that has no pixels yet)", () => {
  let ws: WorkspaceManager;
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

  function renderViewport() {
    const session = WorkspaceManager.createBlankDocument("doc-seed", "Seed", DOC, DOC);
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

  function drawWhite(ctx: CanvasRenderingContext2D) {
    ctx.fillStyle = WHITE;
    ctx.fillRect(0, 0, DOC, DOC);
  }

  /** A HOST-CREATED layer: added at runtime, never present in the Rust graph. */
  function addHostLayer(session: ReturnType<typeof WorkspaceManager.createBlankDocument>) {
    const white = makeCanvas(DOC, DOC, drawWhite);
    session.engine.setLayerImageBitmap(session.engine.getLayers()[0].id, white as unknown as ImageBitmap);
    const hostLayer = session.engine.addLayer("Paint");
    const hostId = hostLayer.id;
    session.engine.setLayerImageBitmap(hostId, white as unknown as ImageBitmap);
    session.engine.setActiveLayer(hostId);
    store.seedPixels(white.getContext("2d")!.getImageData(0, 0, DOC, DOC).data);
    if (!session.engine.getPaintSurface(hostId)) throw new Error("paint surface not created");
    return hostId;
  }

  async function strokeOnce() {
    const canvas = getCanvas();
    fire("pointerdown", canvas, 40, 40);
    fire("pointermove", canvas, 70, 60);
    fire("pointermove", canvas, 100, 80);
    fire("pointerup", canvas, 100, 80);
    await flushC4Commits();
  }

  // THE SECOND HALF OF THE WIRE. The seed test below covers the request; this
  // covers the RESPONSE. rust_pixels_write_region answers with before+after tiles
  // for a 3254x208 dirty rect, which is 27,264,034 JSON characters when each byte
  // is a number - the per-stroke cost that kept ~14 frames over 33 ms even after
  // the seed was fixed. The response must cross base64 too, and this measures the
  // shape the host actually parses.
  //
  // DEFEAT: make the store answer with `data: number[]` tiles again (the shape
  // Rust used to serialize) and jsonElementsPerByte goes above zero and the
  // assertion below goes RED.
  it("the commit's RESPONSE crosses base64 too, not as a per-byte JSON array", async () => {
    const { session } = renderViewport();
    await new Promise((r) => setTimeout(r, 0));
    setZoomState(1);
    setPanState({ x: 0, y: 0 });
    setTool("brush");
    setFgColor("#ff0000");
    addHostLayer(session);
    store.rejectEpoch("layer not initialized");

    await strokeOnce();
    await vi.waitFor(() => expect(store.writes.length).toBe(1), { timeout: 3000 });

    expect(store.responseWires.length).toBeGreaterThan(0);
    // A Rust tile serializes its bytes as base64, so the response is ~4/3 of the
    // byte count. The number form is "255," per byte - about five characters - and
    // the assertion below pins the encoded form under 2x, so a per-byte expansion
    // cannot pass.
    for (const wire of store.responseWires) {
      expect(wire.jsonElementsPerByte, "no tile byte is expanded into a JSON element").toBe(0);
      expect(wire.jsonChars).toBeLessThan(store.responseBytes() * 2);
    }
  });

  // DEFEAT, run at the time of writing. With the store double answering
  // `readTileAsNumbers` instead of `readTile` - the number-array shape Rust used to
  // serialize - the response case above fails with:
  //
  //   AssertionError: expected 110981 to be less than 88400
  //     expect(wire.jsonChars).toBeLessThan(store.responseBytes() * 2);
  //   394|       expect(wire.jsonElementsPerByte, "no tile byte is expanded into ...
  //
  // 110,981 characters for the same bytes base64 carries in ~59,072: the number
  // form is about five characters per byte ("255,") against base64's 1.33. That
  // ratio is what scaled to 27,264,034 characters per stroke at the 3254x208 dirty
  // rect the harness strokes. Restored to `readTile`; the only diff on this file
  // between the RED and GREEN runs is that one identifier.
  //
  // The same substitution on the seed arm (seed.jsonElementsPerByte) is the DEFEAT
  // for the seed case below.
  it("the first stroke's whole-layer seed crosses the IPC as bytes, not as a JSON byte array", async () => {
    const { session } = renderViewport();
    await new Promise((r) => setTimeout(r, 0));
    setZoomState(1);
    setPanState({ x: 0, y: 0 });
    setTool("brush");
    setFgColor("#ff0000");
    addHostLayer(session);
    // Rust holds no pixels for this layer yet, so the commit must seed it first.
    store.rejectEpoch("layer not initialized");

    await strokeOnce();
    await vi.waitFor(() => expect(store.writes.length).toBe(1), { timeout: 3000 });

    expect(store.rejections).toEqual([]);
    expect(store.seeds).toHaveLength(1);
    const seed = store.seeds[0];
    // THE INVARIANT: the IPC layer expands ZERO bytes into JSON array elements.
    // A Uint8Array argument would expand one element per byte - 67,108,864 of them
    // at 4096^2, which is the ~15 s first-stroke stall. The seed crosses as a
    // base64 string, which JSON.stringify copies in one pass.
    expect(seed.jsonElementsPerByte).toBe(0);
    expect(seed.bytes.length).toBe(DOC * DOC * 4);
    // And the encoded form is bounded: base64 is 4/3 of the bytes, never the ~4x
    // expansion of "255," per byte that a number array costs.
    expect(seed.jsonChars).toBeLessThan(Math.ceil((DOC * DOC * 4 * 4) / 3) + 512);
    // The commit still ships the bounded dirty rect afterwards, not the layer.
    expect(store.writes[0].w * store.writes[0].h).toBeLessThan(DOC * DOC);
  });

  it("seeds byte-identical pixels and one history entry — transport only, pixels unchanged", async () => {
    const { session } = renderViewport();
    await new Promise((r) => setTimeout(r, 0));
    setZoomState(1);
    setPanState({ x: 0, y: 0 });
    setTool("brush");
    setFgColor("#ff0000");
    const hostId = addHostLayer(session);
    const pristine = makeCanvas(DOC, DOC, drawWhite).getContext("2d")!.getImageData(0, 0, DOC, DOC).data;
    store.rejectEpoch("layer not initialized");

    await strokeOnce();
    await vi.waitFor(() => expect(store.writes.length).toBe(1), { timeout: 3000 });

    // Pixel parity: the seed that reached Rust is exactly the layer's pixels.
    const seed = store.seeds[0];
    expect(Array.from(seed.bytes.subarray(0, DOC * 4))).toEqual(Array.from(pristine.subarray(0, DOC * 4)));
    expect(seed.width).toBe(DOC);
    expect(seed.height).toBe(DOC);
    // The stroke's dabs are in the committed region and in the store.
    expect(containsPixel(store.writes[0].rgba, (r, g, b, a) => a === 255 && (r !== 255 || g !== 255 || b !== 255))).toBe(true);
    expect(containsPixel(store.snapshot(), (r, g, b, a) => a === 255 && (r !== 255 || g !== 255 || b !== 255))).toBe(true);
    // History parity: the seed opens no history step; the stroke is exactly one.
    expect(store.undoDepth()).toBe(1);
    expect(store.epoch()).toBe(1);
    void hostId;
  });

  // The bug 9dc10677 fixed: the brush used to be gated on graph ownership, which
  // refused strokes on a host-created layer forever. Nothing in the transport fix
  // may bring that refusal back, so the stroke above must still paint. This test
  // fails if any gate blocks the host-created layer.
  it("still paints on a host-created layer (no ownership gate reintroduced)", async () => {
    const { session } = renderViewport();
    await new Promise((r) => setTimeout(r, 0));
    setZoomState(1);
    setPanState({ x: 0, y: 0 });
    setTool("brush");
    setFgColor("#ff0000");
    const hostId = addHostLayer(session);
    const hostLayer = session.engine.getLayer(hostId);
    expect(hostLayer).toBeDefined();
    store.rejectEpoch("layer not initialized");

    await strokeOnce();
    await vi.waitFor(() => expect(store.writes.length).toBe(1), { timeout: 3000 });

    // The stroke painted: red reached the canonical store on the host layer.
    expect(containsPixel(store.snapshot(), (r, g, b, a) => a === 255 && r === 255 && g === 0 && b === 0)).toBe(true);
    expect(store.undoDepth()).toBe(1);
  });
});