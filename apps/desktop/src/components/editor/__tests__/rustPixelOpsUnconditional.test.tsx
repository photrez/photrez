import { decodeRustBytes } from "@/lib/protocol/pixelSeedCall";

/**
 * Paint Bucket Fill, Fill Layer, Adjustment Bake and Gradient Fill are
 * Rust-canonical at the DEFAULT state of photrez.rustPixels (the key a normal
 * user never sets).
 *
 * Each test drives the real production entry, then asserts the four properties
 * that make one op exactly one undoable step:
 *   1. exactly one rust_pixels_write_region reached the canonical store,
 *   2. exactly one Pixel entry exists in that store's history stream,
 *   3. one real Ctrl+Z reverts the canonical bytes,
 *   4. one real Ctrl+Shift+Z restores them.
 *
 * The store fake implements rust_pixels_undo/redo the way paint_parity_cmds.rs
 * does (:173-230): undo pops the document stream and restores the entry's
 * `before` tiles, redo re-applies `after`, and an empty stream resolves with an
 * empty tile list rather than rejecting.
 *
 * Counting is done at the IPC boundary (mockInvoke calls) and against the
 * store's own history array, never through a spy on the routing guard - a guard
 * spy would report "did not take the rust arm" without proving a write happened.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import { invoke } from "@tauri-apps/api/core";
import { WorkspaceManager } from "@/engine/workspace";
import { EditorProvider, useEditor } from "../shell/EditorContext";
import { DialogProvider } from "../dialogs/DialogProvider";
import { useEditorCommands } from "../useEditorCommands";
import { useLayerActions } from "../layers/useLayerActions";
import { applyPaintBucketFill } from "../canvas/pointerTools/paintBucket";
import { applyGradientFill } from "../canvas/pointerTools/gradientTool";
import { fillActiveLayerWithColor } from "../layers/layerOperations";
import { installFaithfulCanvas } from "@/__tests__/faithfulOffscreenCanvas";
import { gradientFillTs } from "@/features/fill/fillOperations";

const DOC = "doc-ops";
const SIZE = 16;

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}));
vi.mock("@tauri-apps/api/app", () => ({
  getVersion: vi.fn(() => Promise.resolve("0.0.0")),
}));

const toasts: unknown[][] = [];
vi.mock("@/components/editor/Toast", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/components/editor/Toast")>();
  return {
    ...actual,
    showToast: (...a: unknown[]) => {
      toasts.push(a);
      return actual.showToast(...(a as Parameters<typeof actual.showToast>));
    },
  };
});

// The undo/redo dispatch asks whether any layer is facade-owned; keep the
// predicate off so the TS tile path is the executor under test.
vi.mock("@/engine/document", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/engine/document")>();
  return {
    ...actual,
    hasFacadeOwnedLayers: vi.fn(() => false),
    isFacadeOwnedLayer: vi.fn(() => false),
  };
});

// ---- jsdom shims ----------------------------------------------------------

// Faithful canvas: transferToImageBitmap() throws InvalidStateError when no 2d
// context was ever obtained, exactly like the real WebView2 canvas. A lenient
// stub is what let the empty-raster crash ship with every suite green.
let restoreCanvas: (() => void) | undefined;


// ---- Rust pixel store fake ------------------------------------------------

type StoredTile = { x: number; y: number; w: number; h: number; data: Uint8Array };
type Entry = { kind: "pixel" | "native"; layerId: string; before?: StoredTile[]; after?: StoredTile[] };

/**
 * In-memory stand-in for the Rust pixel store, faithful to
 * crates/core pixel_store semantics as surfaced by paint_parity_cmds.rs:
 * one write_region == one Pixel entry; undo/redo walk ONE document-level
 * cursor that also carries native entries.
 */
function createStore(width: number, height: number) {
  const buffer = new Uint8ClampedArray(width * height * 4);
  const stream: Entry[] = [];
  let cursor = 0;
  let epoch = 0;
  let writes = 0;
  const rejections: string[] = [];

  const readTile = (x: number, y: number, w: number, h: number): StoredTile => {
    const data = new Uint8Array(w * h * 4);
    for (let row = 0; row < h; row++) {
      data.set(buffer.subarray(((y + row) * width + x) * 4, ((y + row) * width + x + w) * 4), row * w * 4);
    }
    return { x, y, w, h, data };
  };
  const writeTile = (t: StoredTile) => {
    for (let row = 0; row < t.h; row++) {
      buffer.set(t.data.subarray(row * t.w * 4, (row + 1) * t.w * 4), ((t.y + row) * width + t.x) * 4);
    }
  };
  /** Every redo target past the cursor is dropped, as a real forward step does. */
  const dropRedo = () => { stream.length = cursor; };
  const wireTiles = (tiles: StoredTile[]) => tiles.map((t) => ({ x: t.x, y: t.y, w: t.w, h: t.h, data: Array.from(t.data) }));

  vi.mocked(invoke).mockImplementation(async (command: string, args?: unknown) => {
    const a = (args ?? {}) as Record<string, unknown>;
    if (command === "rust_pixels_open_document") return null;
    if (command === "rust_pixels_get_epoch") return epoch;
    if (command === "rust_pixels_init") {
      buffer.set((a.bytes as Uint8Array).subarray(0, buffer.length));
      return null;
    }
    if (command === "rust_pixels_snapshot_layer") return [wireTiles([readTile(0, 0, width, height)])[0]];
    if (command === "rust_pixels_write_region") {
      const x = a.x as number, y = a.y as number, w = a.w as number, h = a.h as number;
      const rgba = decodeRustBytes<{ rgba: Uint8Array }>(a).rgba;
      if (!Number.isInteger(w) || !Number.isInteger(h) || w <= 0 || h <= 0) {
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
      const after: StoredTile = { x, y, w, h, data: rgba.slice() };
      writeTile(after);
      writes += 1;
      dropRedo();
      stream.push({ kind: "pixel", layerId: String(a.layerId), before: [before], after: [after] });
      cursor = stream.length;
      epoch += 1;
      return { before: wireTiles([before]), after: wireTiles([after]), epoch, version: epoch };
    }
    if (command === "rust_pixels_undo" || command === "rust_pixels_redo") {
      const forward = command === "rust_pixels_redo";
      const entry = forward ? stream[cursor] : stream[cursor - 1];
      if (!entry || entry.kind !== "pixel") {
        // paint_parity_cmds.rs:186-197 resolves with an empty tile list.
        return { layerId: String(a.layerId), tiles: [], epoch, version: epoch };
      }
      const tiles = (forward ? entry.after : entry.before)!;
      for (const t of tiles) writeTile(t);
      cursor += forward ? 1 : -1;
      epoch += 1;
      return { layerId: entry.layerId, tiles: wireTiles(tiles), epoch, version: epoch };
    }
    if (command === "rust_pixels_history_depth") {
      return { total_depth: stream.length, undo_depth: cursor, redo_depth: stream.length - cursor, affected_layer_ids: [] };
    }
    if (command === "protocol_apply_command_native") {
      dropRedo();
      stream.push({ kind: "native", layerId: "" });
      cursor = stream.length;
      return JSON.stringify({ documentVersion: cursor, delta: {} });
    }
    if (command === "rust_pixels_record_external" || command === "rust_pixels_record_snapshot") {
      dropRedo();
      stream.push({ kind: "native", layerId: "" });
      cursor = stream.length;
      return { version: epoch };
    }
    return { version: epoch };
  });

  return {
    writes: () => writes,
    rejections,
    pixelEntries: () => stream.filter((e) => e.kind === "pixel").length,
    /** FNV-1a over the canonical buffer: a committed op must move it. */
    hash: () => {
      let h = 0x811c9dc5;
      for (let i = 0; i < buffer.length; i++) {
        h ^= buffer[i];
        h = Math.imul(h, 0x01000193) >>> 0;
      }
      return h.toString(16).padStart(8, "0");
    },
    seed: (pixels: Uint8ClampedArray) => buffer.set(pixels.subarray(0, buffer.length)),
  };
}

// ---- harness --------------------------------------------------------------

let setTool: (t: string) => void = () => {};
let setFgColor: (c: string) => void = () => {};
let commands: ReturnType<typeof useEditorCommands> | null = null;
let layerActions: ReturnType<typeof useLayerActions> | null = null;

const Harness = () => {
  const editor = useEditor();
  setTool = editor.setActiveTool;
  setFgColor = editor.setFgColor;
  commands = useEditorCommands(() => {});
  layerActions = useLayerActions();
  return null;
};

let store: ReturnType<typeof createStore>;
let dispose: (() => void) | undefined;
let container: HTMLDivElement | undefined;

function mount() {
  container = document.createElement("div");
  document.body.appendChild(container);
  const workspace = new WorkspaceManager();
  workspace.addDocument(WorkspaceManager.createBlankDocument(DOC, "Ops", SIZE, SIZE));
  workspace.switchDocument(DOC);
  const renderer: Record<string, unknown> = {
    uploadImage: vi.fn(),
    uploadSurfaceTiles: vi.fn(),
    destroyTexture: vi.fn(),
  };
  const scheduler: Record<string, unknown> = { requestRender: vi.fn() };
  dispose = render(
    () => (
      <DialogProvider>
        <EditorProvider workspace={workspace} renderer={renderer as never} scheduler={scheduler as never}>
          <Harness />
        </EditorProvider>
      </DialogProvider>
    ),
    container,
  );
  const engine = workspace.getActiveEngine()!;
  const history = workspace.getActiveHistory()!;
  return { workspace, engine, history, renderer, scheduler };
}

/** Give the layer a real raster so getPaintSurface can build a surface. */
function primeLayer(engine: ReturnType<WorkspaceManager["getActiveEngine"]>) {
  const layerId = engine!.getActiveLayerId()!;
  const canvas = document.createElement("canvas");
  canvas.width = SIZE;
  canvas.height = SIZE;
  const ctx = canvas.getContext("2d") as CanvasRenderingContext2D;
  ctx.fillStyle = "#ffffff";
  ctx.fillRect(0, 0, SIZE, SIZE);
  engine!.setLayerImageBitmap(layerId, canvas as unknown as ImageBitmap);
  const pixels = ctx.getImageData(0, 0, SIZE, SIZE).data;
  store.seed(pixels);
  return layerId;
}

const tick = async (ms = 0) => new Promise<void>((r) => setTimeout(r, ms));

/**
 * The gradient raster these tests drive: a radial gradient of radius 2 over a
 * white layer, red at the centre fading to the white it already is. Only the
 * disc actually changes, so a correct producer sends a PARTIAL region - a
 * full-layer write would mean the region was assumed instead of measured, and
 * the untouched white around the disc would be re-shipped as if it were edited.
 *
 * The drag points are document-space, so they are offset by the layer position
 * to land on the layer-local points named above. The premise is asserted so this
 * helper can never quietly stop mapping to the intended local coordinates.
 */
const GRADIENT_LOCAL = { start: { x: 8, y: 8 }, end: { x: 8, y: 10 } };
function driveGradient(
  editorCtx: { workspace: unknown; renderer: unknown; scheduler: unknown },
  engine: NonNullable<ReturnType<WorkspaceManager["getActiveEngine"]>>,
  opts: { gradientType?: "linear" | "radial"; fgColor?: string; bgColor?: string } = {},
) {
  const layerId = engine!.getActiveLayerId()!;
  const layer = engine!.getLayer(layerId)!;
  expect(layer.transform.rotation, "the drag->layer mapping assumes no rotation").toBe(0);
  expect(layer.transform.scaleX, "the drag->layer mapping assumes unit scale").toBe(1);
  expect(layer.transform.scaleY, "the drag->layer mapping assumes unit scale").toBe(1);
  const toDoc = (p: { x: number; y: number }) => ({ x: p.x + layer.transform.x, y: p.y + layer.transform.y });
  const editor: any = {
    ...editorCtx,
    activeTool: () => "gradient",
    fgColor: () => opts.fgColor ?? "#ff0000",
    bgColor: () => opts.bgColor ?? "#ffffff",
    gradientPreset: () => "fg-bg",
    gradientType: () => opts.gradientType ?? "radial",
    setGradientDragLine: vi.fn(),
    setStatusLoadingMessage: vi.fn(),
  };
  const state: any = {
    start: toDoc(GRADIENT_LOCAL.start),
    end: toDoc(GRADIENT_LOCAL.end),
    isDragging: true,
    reset: vi.fn(),
  };
  return applyGradientFill({ editor, getDocCoords: () => state.start, getCanvasRef: () => ({ current: null }) } as any, state);
}

const writeRegionArgs = () =>
  (vi.mocked(invoke).mock.calls as [string, Record<string, unknown>][])
    .filter(([c]) => c === "rust_pixels_write_region")
    .map(([, a]) => decodeRustBytes<{ x: number; y: number; w: number; h: number; rgba: Uint8Array }>(a));

/** Same FNV-1a the store fake hashes its canonical buffer with. */
function fnv1a(bytes: ArrayLike<number>): string {
  let h = 0x811c9dc5;
  for (let i = 0; i < bytes.length; i++) {
    h ^= bytes[i];
    h = Math.imul(h, 0x01000193) >>> 0;
  }
  return h.toString(16).padStart(8, "0");
}

/** Real Ctrl+Z / Ctrl+Shift+Z on window, the keys useEditorCommands binds. */
async function pressUndo() {
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "z", ctrlKey: true, bubbles: true, cancelable: true }));
  await tick(10);
}
async function pressRedo() {
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "z", ctrlKey: true, shiftKey: true, bubbles: true, cancelable: true }));
  await tick(10);
}

const countWrites = () =>
  (vi.mocked(invoke).mock.calls as [string][]).filter(([c]) => c === "rust_pixels_write_region").length;

/**
 * The shared post-condition: one write, one Pixel entry, and one Ctrl+Z /
 * Ctrl+Shift+Z round trip through the canonical bytes.
 */
async function expectOneUndoableRustStep(label: string, beforeHash: string) {
  await tick(20);
  expect(countWrites(), `${label}: exactly one rust_pixels_write_region`).toBe(1);
  expect(store.pixelEntries(), `${label}: exactly one Pixel entry in the Rust stream`).toBe(1);
  expect(store.rejections, `${label}: no rejected region write`).toEqual([]);

  const afterHash = store.hash();
  expect(afterHash, `${label}: the canonical buffer moved`).not.toBe(beforeHash);

  await pressUndo();
  expect(store.hash(), `${label}: one Ctrl+Z reverts the canonical bytes`).toBe(beforeHash);
  expect(countWrites(), `${label}: undo is not a second state-changing apply`).toBe(1);

  await pressRedo();
  expect(store.hash(), `${label}: one redo restores the canonical bytes`).toBe(afterHash);
  expect(countWrites(), `${label}: redo is not a second state-changing apply`).toBe(1);
}

beforeEach(() => {
  vi.mocked(invoke).mockReset();
  localStorage.clear();
  // The facade cursor must not own the step under test.
  localStorage.setItem("photrez.facade", "0");
  restoreCanvas = installFaithfulCanvas();
  store = createStore(SIZE, SIZE);
});

afterEach(() => {
  dispose?.();
  dispose = undefined;
  container?.parentNode?.removeChild(container);
  container = undefined;
  restoreCanvas?.();
  restoreCanvas = undefined;
  localStorage.clear();
  vi.restoreAllMocks();
});

describe("bucket, fill and bake are Rust-canonical with photrez.rustPixels at its default", () => {
  it("Paint Bucket Fill: one write, one Pixel entry, Ctrl+Z reverts, redo restores", async () => {
    // DEFAULT state: the key was never set by this user.
    expect(localStorage.getItem("photrez.rustPixels")).toBeNull();
    const { workspace, engine, history, renderer } = mount();
    await tick();
    setTool("paintBucket");
    setFgColor("#ff0000");
    primeLayer(engine);
    const beforeHash = store.hash();

    const editor: any = {
      activeTool: () => "paintBucket",
      workspace,
      renderer,
      scheduler: { requestRender: vi.fn() },
      fgColor: () => "#ff0000",
      fillTolerance: () => 0,
      fillContiguous: () => true,
    };
    const ctx: any = {
      editor,
      getDocCoords: () => ({ x: 4, y: 4 }),
      getCanvasRef: () => ({ current: null }),
    };
    expect(applyPaintBucketFill(ctx, { pointerId: 1 } as any), "the bucket handled the click").toBe(true);

    await expectOneUndoableRustStep("bucket", beforeHash);
    // The TS twin must be a cursor token, not a second copy of the pixels.
    expect(history.getHistoryStack().map((i) => i.label)).toContain("Paint Bucket Fill");
  });

  it("Fill Layer: one write, one Pixel entry, Ctrl+Z reverts, redo restores", async () => {
    expect(localStorage.getItem("photrez.rustPixels")).toBeNull();
    const { engine, history, renderer } = mount();
    await tick();
    primeLayer(engine);
    const beforeHash = store.hash();

    expect(fillActiveLayerWithColor(engine, history, renderer as never, "#00ff00")).toBe(true);

    await expectOneUndoableRustStep("fill", beforeHash);
    expect(history.getHistoryStack().map((i) => i.label)).toContain("Fill Layer");
  });

  it("Adjustment Bake: one write, one Pixel entry, Ctrl+Z reverts, redo restores", async () => {
    expect(localStorage.getItem("photrez.rustPixels")).toBeNull();
    const { engine } = mount();
    await tick();
    const layerId = primeLayer(engine);
    engine.getLayer(layerId)!.basicAdjustment = { brightness: 20, contrast: 0, saturation: 0 };
    // Bake is CPU in jsdom: give commitBasicAdjustment the bitmap swap the
    // real engine performs, so the Rust write has baked bytes to ship.
    vi.spyOn(engine, "commitBasicAdjustment").mockImplementation(async (id: string) => {
      const canvas = document.createElement("canvas");
      canvas.width = SIZE;
      canvas.height = SIZE;
      const ctx = canvas.getContext("2d") as CanvasRenderingContext2D;
      ctx.fillStyle = "#808080";
      ctx.fillRect(0, 0, SIZE, SIZE);
      engine.setLayerImageBitmap(id, canvas as unknown as ImageBitmap);
      return "cpu" as const;
    });
    const beforeHash = store.hash();

    await layerActions!.handleApplyAdjustment();

    await expectOneUndoableRustStep("bake", beforeHash);
  });

  it("Gradient Fill: one write, one Pixel entry, Ctrl+Z reverts, redo restores", async () => {
    expect(localStorage.getItem("photrez.rustPixels")).toBeNull();
    const { workspace, engine, history, renderer } = mount();
    await tick();
    primeLayer(engine);
    const commitSpy = vi.spyOn(history, "commit");
    const beforeHash = store.hash();

    expect(await driveGradient({ workspace, renderer, scheduler: { requestRender: vi.fn() } }, engine)).toBe(true);
    await tick(50);

    await expectOneUndoableRustStep("gradient", beforeHash);
    // The TS twin must be a cursor token, not a second copy of the pixels.
    expect(history.getHistoryStack().map((i) => i.label)).toContain("Gradient Fill");
    expect(commitSpy, "the gradient committed exactly once").toHaveBeenCalledTimes(1);
    expect(commitSpy.mock.calls[0][1], "the entry is labelled for the history UI").toBe("Gradient Fill");
    const twin = commitSpy.mock.calls[0][2] as { rustOwned?: boolean } | undefined;
    expect(twin?.rustOwned, "the twin is a cursor token, not a pixel source").toBe(true);
    expect(commitSpy.mock.calls[0][3], "Rust already owns the Pixel entry").toBe(true);
    expect(store.rejections, "the region write was accepted").toEqual([]);
  });

  it("Gradient Fill sends the region it actually touched, not the whole layer", async () => {
    const { workspace, engine, renderer } = mount();
    await tick();
    primeLayer(engine);
    // A selection bounds the raster, so the changed box is the selection rect
    // while the layer is 4x larger. A producer that assumed "a gradient writes
    // the full layer" would ship all 16x16 pixels and re-write the untouched
    // white around the selection as if the op had edited it.
    engine.createSelection(2, 2, 4, 4);
    const beforeHash = store.hash();

    expect(await driveGradient({ workspace, renderer, scheduler: { requestRender: vi.fn() } }, engine, { bgColor: "#0000ff" })).toBe(true);
    await tick(50);

    const wr = writeRegionArgs();
    expect(wr.length, "exactly one region write").toBe(1);
    expect({ x: wr[0].x, y: wr[0].y, w: wr[0].w, h: wr[0].h }, "the write is the selection, not the layer").toEqual({ x: 2, y: 2, w: 4, h: 4 });
    expect(wr[0].rgba.byteLength, "the payload covers exactly the region").toBe(4 * 4 * 4);

    // The whole canonical buffer, not just the written region: white everywhere
    // the gradient was not allowed to paint, blue inside the selection. This is
    // what proves the pixels outside the region were left alone.
    const expected = new Uint8ClampedArray(SIZE * SIZE * 4);
    for (let i = 0; i < SIZE * SIZE; i++) {
      expected[i * 4] = 255; expected[i * 4 + 1] = 255;
      expected[i * 4 + 2] = 255; expected[i * 4 + 3] = 255;
    }
    for (let row = 0; row < 4; row++) {
      for (let col = 0; col < 4; col++) {
        const i = ((2 + row) * SIZE + (2 + col)) * 4;
        expected[i] = 0; expected[i + 1] = 0; expected[i + 2] = 255; expected[i + 3] = 255;
      }
    }
    expect(store.hash(), "only the selected pixels changed in the canonical buffer").toBe(fnv1a(expected));
    expect(beforeHash, "premise: the gradient moved the buffer").not.toBe(store.hash());
  });

  it("Gradient Fill on a layer with no raster refuses visibly instead of committing", async () => {
    const { workspace, engine, history, renderer } = mount();
    await tick();
    const layerId = engine.getActiveLayerId()!;
    expect(bitmapHash(engine, layerId), "premise: the layer has no raster to seed from").toBe("no-bitmap");

    expect(await driveGradient({ workspace, renderer, scheduler: { requestRender: vi.fn() } }, engine)).toBe(true);
    await tick(30);

    expect(countWrites(), "nothing was written without a canonical surface").toBe(0);
    expect(history.getHistoryStack().map((i) => i.label)).not.toContain("Gradient Fill");
    // The refusal must be visible AND must name the missing surface. A silent
    // no-op, or one that fails later with a different message, is the failure
    // this pins: the user would see the gradient do nothing with no reason.
    expect(
      toasts.some(([msg, kind]) => kind === "warn" && String(msg).includes("surface")),
      `a visible refusal naming the missing surface, got ${JSON.stringify(toasts)}`,
    ).toBe(true);
  });
});

/**
 * The layer bitmap hash is what the visible canvas is built from - the viewport
 * renderer uploads `layer.imageBitmap` (useViewportRenderer calls uploadImage with
 * it). Hashing it is how a fill is proven VISIBLE, not merely recorded.
 *
 * Reads both bitmap shapes the engine holds: a transferred ImageBitmap-like
 * (getImageData) and a live canvas element (getContext).
 */
function bitmapHash(engine: ReturnType<WorkspaceManager["getActiveEngine"]>, layerId: string): string {
  const bitmap = engine!.getLayerImageBitmap(layerId) as any;
  if (!bitmap) return "no-bitmap";
  let bytes: ArrayLike<number> | null = null;
  if (typeof bitmap.getImageData === "function") {
    bytes = bitmap.getImageData(0, 0, SIZE, SIZE).data;
  } else if (typeof bitmap.getContext === "function") {
    const c = bitmap.getContext("2d");
    if (c) bytes = c.getImageData(0, 0, SIZE, SIZE).data;
  }
  if (!bytes) return "unreadable-bitmap";
  let h = 0x811c9dc5;
  for (let i = 0; i < bytes.length; i++) {
    h ^= bytes[i];
    h = Math.imul(h, 0x01000193) >>> 0;
  }
  return h.toString(16).padStart(8, "0");
}

describe("a fill is visible at op time, not only after an undo/redo round trip", () => {
  it("Paint Bucket Fill refreshes the layer bitmap and the visible texture", async () => {
    const { workspace, engine, history, renderer } = mount();
    await tick();
    setTool("paintBucket");
    setFgColor("#ff0000");
    const layerId = primeLayer(engine);
    const beforeHash = bitmapHash(engine, layerId);

    const editor: any = {
      activeTool: () => "paintBucket",
      workspace,
      renderer,
      scheduler: { requestRender: vi.fn() },
      fgColor: () => "#ff0000",
      fillTolerance: () => 0,
      fillContiguous: () => true,
    };
    const ctx: any = { editor, getDocCoords: () => ({ x: 4, y: 4 }), getCanvasRef: () => ({ current: null }) };
    applyPaintBucketFill(ctx, { pointerId: 1 } as any);
    await tick(30);

    // The canonical write is not the visible surface: the viewport renderer draws
    // from the layer bitmap, so an op that only writes Rust leaves the canvas
    // showing the pre-fill pixels until something else rebuilds that bitmap.
    expect(history.getHistoryStack().map((i) => i.label)).toContain("Paint Bucket Fill");
    expect(bitmapHash(engine, layerId), "the layer bitmap carries the fill").not.toBe(beforeHash);
    expect((renderer.uploadImage as any).mock.calls.length, "the visible texture was refreshed").toBeGreaterThan(0);
  });

  it("Fill Layer refreshes the layer bitmap and the visible texture", async () => {
    const { engine, history, renderer } = mount();
    await tick();
    const layerId = primeLayer(engine);
    const beforeHash = bitmapHash(engine, layerId);

    expect(fillActiveLayerWithColor(engine, history, renderer as never, "#00ff00")).toBe(true);
    await tick(30);

    expect(bitmapHash(engine, layerId), "the layer bitmap carries the fill").not.toBe(beforeHash);
    expect((renderer.uploadImage as any).mock.calls.length, "the visible texture was refreshed").toBeGreaterThan(0);
  });

  it("Gradient Fill refreshes the layer bitmap and the visible texture", async () => {
    const { workspace, engine, history, renderer } = mount();
    await tick();
    const layerId = primeLayer(engine);
    const beforeBitmap = engine.getLayerImageBitmap(layerId);

    expect(await driveGradient({ workspace, renderer, scheduler: { requestRender: vi.fn() } }, engine)).toBe(true);
    await tick(50);

    // The viewport renderer draws from layer.imageBitmap, so an op that only
    // writes the canonical store leaves the canvas showing the pre-gradient
    // raster until an undo/redo happens to rebuild it. Identity is the honest
    // check here: the raster the texture was built from must be a NEW object.
    expect(history.getHistoryStack().map((i) => i.label)).toContain("Gradient Fill");
    const afterBitmap = engine.getLayerImageBitmap(layerId);
    expect(afterBitmap, "the layer bitmap was rebuilt").not.toBe(beforeBitmap);
    expect((renderer.uploadImage as any).mock.calls.length, "the visible texture was refreshed").toBeGreaterThan(0);
    expect(
      (renderer.uploadImage as any).mock.calls.some((c: unknown[]) => c[0] === layerId && c[1] === afterBitmap),
      "the visible texture received the layer's current raster",
    ).toBe(true);
  });
});

describe("Alt+Delete on a layer with no raster", () => {
  it("materialises the raster and records one canonical write without throwing", async () => {
    const { engine, history, renderer } = mount();
    await tick();
    const layerId = engine.getActiveLayerId()!;
    // Premise, asserted so this can never silently stop testing the real flow.
    expect(bitmapHash(engine, layerId), "the layer really has no raster to start from").toBe("no-bitmap");

    let threw: unknown = null;
    try {
      expect(fillActiveLayerWithColor(engine, history, renderer as never, "#ff0000")).toBe(true);
    } catch (err) {
      threw = err;
    }
    await tick(30);

    // The shipped crash: transferToImageBitmap() on a canvas whose 2d context was
    // never obtained throws InvalidStateError and takes the canvas down with it.
    expect(threw, "no exception escaped the fill").toBeNull();
    expect(bitmapHash(engine, layerId), "the empty raster materialised").not.toBe("no-bitmap");
    expect(countWrites(), "the fill recorded through Rust").toBe(1);
  });

  it("refuses a zero-size layer visibly instead of crashing on the transfer", async () => {
    const { engine, history, renderer } = mount();
    await tick();
    const layerId = engine.getActiveLayerId()!;
    engine.getLayer(layerId)!.width = 0;
    engine.getLayer(layerId)!.height = 0;

    let threw: unknown = null;
    try {
      fillActiveLayerWithColor(engine, history, renderer as never, "#ff0000");
      await tick(20);
    } catch (err) {
      threw = err;
    }

    expect(threw, "a zero-size layer is refused, not crashed on").toBeNull();
    expect(countWrites(), "nothing was written for a zero-size layer").toBe(0);
  });

  it("refuses a layer with negative dimensions visibly instead of crashing", async () => {
    const { engine, history, renderer } = mount();
    await tick();
    const layerId = engine.getActiveLayerId()!;
    engine.getLayer(layerId)!.width = -8;
    engine.getLayer(layerId)!.height = 16;

    let threw: unknown = null;
    try {
      fillActiveLayerWithColor(engine, history, renderer as never, "#ff0000");
      await tick(20);
    } catch (err) {
      threw = err;
    }

    expect(threw, "negative dimensions are refused, not crashed on").toBeNull();
    expect(countWrites(), "nothing was written for a negative-size layer").toBe(0);
  });
});

/**
 * The GPU boundary. A WebGPU device IS available here and a wasm module that
 * would hand the gradient a working GPU rasteriser IS installed, painting a
 * value the CPU kernel can never produce. The gradient must ignore both: the
 * canonical bytes have to come from the CPU read of the canonical surface.
 *
 * The poison exists so this test cannot pass vacuously. Without it, a GPU path
 * that silently fell back to the CPU would produce byte-identical output and
 * these assertions would hold whether or not the GPU was consulted.
 */
const gpuRasterCalls: string[] = [];
vi.mock("@/components/editor/wasmExport", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/components/editor/wasmExport")>();
  return {
    ...actual,
    // Keep the reference CPU kernel: the expected bytes below are computed with
    // it, so shipping anything else is a mismatch rather than a silent pass.
    gradientFillWithWasm: () => null,
    getWasmExportModule: async () => ({
      WebGpuAdjustRenderer: {
        create: async (w: number, h: number) => {
          gpuRasterCalls.push(`create ${w}x${h}`);
          return {
            gradient: async (_data: Uint8Array, ...rest: number[]) => {
              gpuRasterCalls.push(`gradient ${rest.length} args`);
              return new Uint8Array(SIZE * SIZE * 4).fill(0xab);
            },
          };
        },
      },
    }),
  };
});

describe("the gradient's canonical bytes never come from the GPU", () => {
  const installGpu = () => {
    const nav = navigator as unknown as { gpu?: unknown };
    const previous = nav.gpu;
    nav.gpu = { requestAdapter: async () => ({}) };
    return () => { nav.gpu = previous; };
  };

  it("records the CPU raster even with a working WebGPU gradient kernel installed", async () => {
    const restoreGpu = installGpu();
    try {
      const { workspace, engine, renderer } = mount();
      await tick();
      primeLayer(engine);

      expect(await driveGradient({ workspace, renderer, scheduler: { requestRender: vi.fn() } }, engine)).toBe(true);
      await tick(50);

      expect(gpuRasterCalls, "the GPU gradient kernel was never the canonical rasteriser").toEqual([]);

      const wr = writeRegionArgs();
      expect(wr.length, "one canonical region write").toBe(1);
      // Independently recompute the expected payload with the reference CPU
      // kernel over a white layer, then compare the shipped bytes exactly.
      const expected = { data: new Uint8ClampedArray(SIZE * SIZE * 4), width: SIZE, height: SIZE };
      for (let i = 0; i < SIZE * SIZE; i++) {
        expected.data[i * 4] = 255; expected.data[i * 4 + 1] = 255;
        expected.data[i * 4 + 2] = 255; expected.data[i * 4 + 3] = 255;
      }
      gradientFillTs(
        expected as unknown as ImageData,
        "radial",
        GRADIENT_LOCAL.start.x, GRADIENT_LOCAL.start.y,
        GRADIENT_LOCAL.end.x, GRADIENT_LOCAL.end.y,
        [{ offset: 0, r: 255, g: 0, b: 0, a: 255 }, { offset: 1, r: 255, g: 255, b: 255, a: 255 }],
        null,
      );
      const { x, y, w, h, rgba } = wr[0];
      const want = new Uint8ClampedArray(w * h * 4);
      for (let row = 0; row < h; row++) {
        const src = ((y + row) * SIZE + x) * 4;
        want.set(expected.data.subarray(src, src + w * 4), row * w * 4);
      }
      expect(Array.from(rgba), "the shipped bytes are the CPU gradient, not GPU output").toEqual(Array.from(want));
      expect(Array.from(rgba).includes(0xab), "no GPU-poison byte reached the canonical store").toBe(false);
    } finally {
      restoreGpu();
      gpuRasterCalls.length = 0;
    }
  });
});
