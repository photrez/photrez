// SPDX-License-Identifier: AGPL-3.0-or-later

/**
 * A PARAMETRIC layer is not a pixel owner, so a paint op onto one is REFUSED.
 *
 * WHY. A shape layer's `shapeParams` and a text layer's `textData` are the
 * document state; `imageBitmap` is a CACHE re-derived from them on every edit
 * (`DocumentEngine.updateShapeParams` and `updateTextData`, which each call their
 * rasterizer and hand the result to `replaceLayerBitmap`). A parametric layer
 * therefore cannot absorb accumulated pixels: the next param edit re-derives the
 * raster from unchanged params and silently discards whatever was written. The
 * canvas pointer dispatcher already refuses this for brush, eraser, bucket and
 * gradient behind a "Convert to Pixels" confirm
 * (`canvas/useCanvasPointerTools.ts`, the "Parametric-layer pixel guard").
 *
 * THE GAP THESE CASES CLOSE. Two producers sit OUTSIDE that dispatcher and
 * carried no such check:
 *   - `fillActiveLayerWithColor`, reached by Alt+Delete / Ctrl+Delete through
 *     `handleLayerFillKey` (`canvas/keyboardShortcuts/layerFill.ts`), which guards
 *     only a transform session and the crop tool;
 *   - `SelectionOperations.deleteSelection`, reached by the selection tool's
 *     Delete key and the "Delete Selection Pixels" button, which is not in the
 *     pointer dispatcher's tool list at all.
 * Both reached the Rust single-owner write path, so a fill or a byte-clear landed
 * in the canonical store on a layer whose next param edit would erase it - and
 * because the write is canonical, the erasure is not merely visual: the store
 * and the projection then agree on the re-derived raster, so no convergence
 * check would ever report the lost edit.
 *
 * THE THREE ARMS, driven the way a user drives them: a real
 * `handleLayerFillKey` keydown and a real `SelectionOperations.deleteSelection`,
 * both over a real `CommandHistory` and a transport-faithful pixel store.
 */
import { describe, it, expect, vi, beforeEach, afterEach, type Mock } from "vitest";
import { pixelSeedDispatch } from "@/lib/protocol/pixelSeedCall";
import { invoke } from "@tauri-apps/api/core";
import { handleLayerFillKey } from "@/components/editor/canvas/keyboardShortcuts/layerFill";
import { fillActiveLayerWithColor } from "@/components/editor/layers/layerOperations";
import { SelectionOperations } from "@/features/selection/SelectionOperations";
import { showToast } from "@/components/editor/Toast";
import { CommandHistory } from "@/engine/history";
import { createRustStoreEmulator, type RustStoreEmulator } from "@/lib/paint/__tests__/rustStoreEmulator";
import { toIpcBytes } from "@/lib/paint/storeCurrency";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const invokeMock = invoke as unknown as Mock<(cmd: string, args: Record<string, unknown>) => Promise<unknown>>;

vi.mock("@/components/editor/Toast", () => ({ showToast: vi.fn() }));

const DOC = "parametricFill";
const SIZE = 8;
type ParametricKind = "shape" | "text" | "raster";

function issued(): string[] {
  return invokeMock.mock.calls.map((c) => c[0]);
}

/** Commands that can put bytes into the Rust pixel store. */
function pixelStoreCommands(): string[] {
  return issued().filter((c) => c.startsWith("rust_pixels_"));
}

async function flushAsync(rounds = 4): Promise<void> {
  for (let i = 0; i < rounds; i++) {
    for (let j = 0; j < 12; j++) await Promise.resolve();
    await new Promise((resolve) => setTimeout(resolve, 0));
  }
}

function makeLayer(kind: ParametricKind) {
  return {
    id: "L1",
    type: kind === "raster" ? "raster" : kind,
    name: kind,
    width: SIZE,
    height: SIZE,
    locked: false,
    visible: true,
    lockTransparency: false,
    opacity: 1,
    blendMode: "normal",
    isBackground: false,
    imageBitmap: { width: SIZE, height: SIZE, close: () => {} },
    shapeParams: kind === "shape" ? { kind: "rect" } : null,
    textData: kind === "text" ? { content: "hi" } : null,
    basicAdjustment: null,
    transform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
  };
}

/**
 * The engine surface both producers touch. `getPaintSurface` answers a REAL
 * surface (never null) on purpose: the guard has to refuse BEFORE the surface is
 * consulted, so a double that returned null would let the arm take its
 * "Rust pixel surface not ready" branch instead and pass for the wrong reason.
 */
function makeEngine(kind: ParametricKind) {
  const layer = makeLayer(kind);
  const surface = {
    context: {
      // NON-ZERO and uniform: a zero surface makes every fill a byte-for-byte
      // no-op against a zero store, and the producer correctly writes nothing -
      // which would make a positive control pass or fail for the wrong reason.
      getImageData: (_x: number, _y: number, w: number, h: number) => ({
        data: new Uint8ClampedArray(w * h * 4).fill(10), width: w, height: h, colorSpace: "srgb",
      }),
      putImageData: vi.fn(),
    },
    pixelEpoch: 0,
    pixelVersion: 0,
  };
  const engine = {
    getId: () => DOC,
    getActiveLayerId: () => layer.id,
    getLayer: (id: string) => (id === layer.id ? layer : null),
    getSelection: () => null,
    getPaintSurface: vi.fn(() => surface),
    getLayerImageBitmap: () => layer.imageBitmap,
    setLayerImageBitmap: vi.fn(),
    snapshot: () => ({ id: DOC, layers: [layer] }),
    restore: vi.fn(),
    clearBasicAdjustments: vi.fn(),
    invalidatePaintSurface: vi.fn(),
    notifyVisualChange: vi.fn(),
    clearSelection: vi.fn(),
    setRenderHiddenLayerId: vi.fn(),
    // THE GUARD'S INPUT. Both producers ask the engine, not the layer, so the
    // double answers from the layer's own type - the same question the real
    // engine answers via `applyIsShapeLayer` / `applyIsTextLayer`.
    isShapeLayer: (id: string) => engine.getLayer(id)?.type === "shape",
    isTextLayer: (id: string) => engine.getLayer(id)?.type === "text",
  };
  return { engine, layer, surface };
}

function makeRenderer(): unknown {
  return { uploadImage: vi.fn(), uploadSurfaceTiles: vi.fn(), destroyTexture: vi.fn() };
}

/** The editor surface `handleLayerFillKey` reads. */
function makeKeyboardCtx(): unknown {
  return {
    editor: {
      renderer: makeRenderer(),
      scheduler: { requestRender: vi.fn() },
      layerTransformSession: () => null,
      activeTool: () => "brush",
      fgColor: () => "#ff0000",
      bgColor: () => "#0000ff",
      setRenamingLayerId: vi.fn(),
      setRenameLayerName: vi.fn(),
    },
    options: {},
    layerActions: {},
  };
}

/** A real Alt+Delete keydown - the shortcut the app binds to Fill Layer. */
function pressAltDelete(): boolean {
  return handleLayerFillKey(
    makeKeyboardCtx() as never,
    { key: "Delete", altKey: true, ctrlKey: false, metaKey: false, preventDefault: vi.fn(), stopPropagation: vi.fn() } as never,
    undefined as never,
    undefined as never,
  );
}

describe("Alt+Delete refuses a parametric layer instead of filling it", () => {
  let store: RustStoreEmulator;

  beforeEach(() => {
    localStorage.clear();
    localStorage.setItem("photrez.facade", "0");
    invokeMock.mockReset();
    (showToast as unknown as Mock).mockClear();
    store = createRustStoreEmulator();
    invokeMock.mockImplementation(((cmd: string, args: Record<string, unknown>) =>
      store.invoke(cmd, args)) as never);
    (globalThis as Record<string, unknown>).__TAURI_INTERNALS__ = undefined;
  });

  afterEach(() => {
    store.dispose();
    localStorage.clear();
    vi.restoreAllMocks();
  });

  for (const kind of ["shape", "text"] as const) {
    it(`issues ZERO pixel-store commands and no history entry on a ${kind} layer`, async () => {
      const { engine } = makeEngine(kind);
      const history = new CommandHistory(8);
      const commitSpy = vi.spyOn(history, "commit");

      // Direct call, so the assertion is about the producer's own refusal rather
      // than about the keydown's routing. The keydown path is covered below.
      const ok = fillActiveLayerWithColor(engine as never, history, makeRenderer() as never, "#ff0000");
      await flushAsync();

      expect(ok, `premise: the ${kind} fill was refused, not performed`).toBe(false);
      expect(commitSpy, "a refused fill records no history entry").not.toHaveBeenCalled();
      expect(
        pixelStoreCommands(),
        `a parametric layer must gain no store entry: the next param edit re-derives the raster and would erase it`,
      ).toEqual([]);
    });
  }

  // THE POSITIVE CONTROL, and the case that would catch an over-broad guard. A
  // raster layer is the ONLY kind this op is for. The assertion is the BOUNDARY
  // the guard controls - the producer PROCEEDS into the canonical path for a
  // raster layer and stops dead for a parametric one - rather than the bytes the
  // write carries, which depend on fill-versus-store colour equality this
  // fixture deliberately does not model. A check that were wrong in the
  // permissive direction would show zero canonical commands here; one wrong in
  // the restrictive direction would show the parametric rows passing for free.
  it("still PROCEEDS into the canonical path on a RASTER layer", async () => {
    const { engine } = makeEngine("raster");
    // The fill reads the selection, so the positive control needs one.
    const withSel = { ...engine, getSelection: () => ({ shape: "rect", active: false, inverted: false, x: 0, y: 0, w: SIZE, h: SIZE }) };
    const history = new CommandHistory(8);
    await store.invoke("rust_pixels_init", pixelSeedDispatch(DOC, "L1", SIZE, SIZE, toIpcBytes(new Uint8ClampedArray(SIZE * SIZE * 4))));
    invokeMock.mockClear();

    const ok = fillActiveLayerWithColor(withSel as never, history, makeRenderer() as never, "#ff0000");
    expect(ok, "a raster layer is what this op is for").toBe(true);
    await flushAsync(8);

    expect(
      store.calls.filter((c) => c.cmd === "rust_pixels_get_epoch").length,
      "the raster fill reached the canonical store - the guard did not swallow it",
    ).toBeGreaterThan(0);
  });

  // THE REAL KEYDOWN, so the refusal is proven at the surface a user reaches and
  // not only at the producer. A guard in the producer covers this automatically;
  // a guard placed in the wrong one of the two would not.
  it("the Alt+Delete keydown on a text layer writes nothing and says why", async () => {
    const { engine } = makeEngine("text");
    const history = new CommandHistory(8);
    const commitSpy = vi.spyOn(history, "commit");

    const handled = handleLayerFillKey(
      makeKeyboardCtx() as never,
      { key: "Delete", altKey: true, ctrlKey: false, metaKey: false, preventDefault: vi.fn(), stopPropagation: vi.fn() } as never,
      engine as never,
      history,
    );
    await flushAsync();

    expect(handled, "the shortcut was consumed").toBe(true);
    expect(commitSpy, "no history entry for a refused parametric fill").not.toHaveBeenCalled();
    expect(pixelStoreCommands(), "the keydown issued no pixel-store command").toEqual([]);
    expect(
      (showToast as unknown as Mock).mock.calls,
      "the user is told the fill did not happen - the caller's existing toast",
    ).toEqual([["Could not fill layer", "warn"]]);
  });
});

describe("Delete Selection Pixels refuses a parametric layer instead of clearing it", () => {
  let store: RustStoreEmulator;

  const SEL = { shape: "rect", active: true, inverted: false, x: 0, y: 0, w: SIZE, h: SIZE };

  beforeEach(() => {
    localStorage.clear();
    localStorage.setItem("photrez.facade", "0");
    invokeMock.mockReset();
    (showToast as unknown as Mock).mockClear();
    store = createRustStoreEmulator();
    invokeMock.mockImplementation(((cmd: string, args: Record<string, unknown>) =>
      store.invoke(cmd, args)) as never);
    (globalThis as Record<string, unknown>).__TAURI_INTERNALS__ = undefined;
  });

  afterEach(() => {
    store.dispose();
    localStorage.clear();
    vi.restoreAllMocks();
  });

  for (const kind of ["shape", "text"] as const) {
    it(`writes nothing and leaves the selection standing on a ${kind} layer`, async () => {
      const { engine } = makeEngine(kind);
      const engineSel = { ...engine, getSelection: () => SEL } as never as typeof engine;
      const history = new CommandHistory(8);
      const commitSpy = vi.spyOn(history, "commit");

      SelectionOperations.deleteSelection(engineSel as never, history, makeRenderer() as never);
      await flushAsync();

      expect(
        pixelStoreCommands(),
        `a parametric layer must gain no store entry: the next param edit would re-derive the raster and restore the deleted pixels`,
      ).toEqual([]);
      expect(commitSpy, "a refused delete-pixels records no history entry").not.toHaveBeenCalled();
      expect(
        (engineSel as unknown as { clearSelection: Mock }).clearSelection,
        "the selection is left standing, so the user can change their mind",
      ).not.toHaveBeenCalled();
      expect(
        (showToast as unknown as Mock).mock.calls,
        "the user is told why nothing happened",
      ).toEqual([["Cannot delete pixels from a shape or text layer", "warn"]]);
    });
  }

  // THE POSITIVE CONTROL, as above: the guard must not swallow a legitimate
  // delete on the layer kind this op exists for. The boundary asserted is that
  // the producer PROCEEDS to the canonical store for a raster layer.
  it("still PROCEEDS into the canonical path on a RASTER layer", async () => {
    const { engine } = makeEngine("raster");
    const engineSel = { ...engine, getSelection: () => SEL } as never as typeof engine;
    const history = new CommandHistory(8);
    await store.invoke("rust_pixels_init", pixelSeedDispatch(DOC, "L1", SIZE, SIZE, toIpcBytes(new Uint8ClampedArray(SIZE * SIZE * 4).fill(90))));
    invokeMock.mockClear();

    SelectionOperations.deleteSelection(engineSel as never, history, makeRenderer() as never);
    await flushAsync(8);

    expect(
      store.calls.filter((c) => c.cmd === "rust_pixels_get_epoch").length,
      "the raster delete reached the canonical store - the guard did not swallow it",
    ).toBeGreaterThan(0);
    expect(
      (engineSel as unknown as { clearSelection: Mock }).clearSelection,
      "and the selection was cleared, which a refusal would have left standing",
    ).toHaveBeenCalled();
  });
});
