// SPDX-License-Identifier: AGPL-3.0-or-later
//
// TextEditOverlay call-site wiring while the edited layer is owned by the native
// editor state. The protocol-level tests prove the funnel and the projection; this
// file proves the OVERLAY actually drives them, which is the half a pure-function
// test cannot see.
//
// Contracts pinned here:
//  1. Corner resize on an owned layer: ZERO protocol calls during the frames (the
//     gesture fires at frame rate), the box size travels with the preview so the
//     quad tracks the pointer instead of freezing at the model size, and exactly
//     one transform + one params command land at release.
//  2. Flag OFF is untouched: the per-frame model write and the single TS history
//     entry stay exactly where they were, and no facade call is made.
//  3. Abandon mid-resize leaves nothing behind: preview cleared, texture taken back
//     from the model bitmap, transform slot released so a later numeric edit lands.
//  4. Typing on an owned layer is routed at session close: the keystrokes do not
//     write the model (engine.updateTextData has no ownership guard, so that write
//     would silently diverge from the native arm), and the close sends ONE params
//     command whose projection puts both sides back in agreement.
//  5. A never-owned temp layer keeps the legacy path on both sides of the flag.
//
// Real component, real DocumentEngine, real facade projection, real protocol arm.
// Command counts are taken at the single boundary every protocol command crosses
// (bridge applyCommand).
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { createSignal } from "solid-js";
import { render } from "solid-js/web";
import { mockUseEditor } from "@/__tests__/mockUseEditor";
import { TextEditOverlay } from "../TextEditOverlay";
import { commitTextSession } from "../canvas/pointerTools/textTool";
import { DocumentEngine, isFacadeOwnedLayer } from "@/engine/document";
import { CommandHistory } from "@/engine/history";
import { DEFAULT_TEXT_DATA, type TextData } from "@/engine/textTypes";
import type { TextEditSession } from "../tools/editorState";
import {
  __resetFacadeRegistryForTests,
  applyFacadePreviews,
  facadeCommitNumericTransform,
  getFacade,
  peekFacade,
  setTransformPreview,
  transformPreview,
} from "@/lib/protocol/facadeRegistry";
import { getWasmExportModule } from "@/components/editor/wasmExport";
import { applyCommand, getSnapshot } from "@/lib/protocol/bridge";
import { CONTRACT_VERSION } from "@/lib/protocol/types";
import type { CommandEnvelope } from "@/lib/protocol/types";

vi.mock("../Toast", () => ({ showToast: vi.fn() }));

// applyCommand is the one function every protocol command crosses, under the wasm
// arm and the native arm alike, so its call list IS the dispatch count.
const { commandLog } = vi.hoisted(() => ({ commandLog: { types: [] as string[] } }));
vi.mock("@/lib/protocol/bridge", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/lib/protocol/bridge")>();
  return {
    ...actual,
    applyCommand: (envelope: CommandEnvelope) => {
      commandLog.types.push(envelope.command.type);
      return actual.applyCommand(envelope);
    },
  };
});

// jsdom has no OffscreenCanvas; stub the seam the text rasterizer uses so the
// live re-raster (both branches) produces a bitmap instead of a null context.
function stubOffscreenCanvas(): void {
  const Mock = function (this: any, w: number, h: number) {
    this.width = w;
    this.height = h;
    const ctx: any = {
      font: "",
      fillStyle: "",
      strokeStyle: "",
      lineWidth: 0,
      lineJoin: "miter",
      miterLimit: 10,
      globalAlpha: 1,
      globalCompositeOperation: "source-over",
      textBaseline: "alphabetic",
      letterSpacing: undefined,
      measureText: (s: string) => ({
        width: s.length * 10,
        actualBoundingBoxAscent: 80,
        actualBoundingBoxDescent: 24,
        fontBoundingBoxAscent: 80,
        fontBoundingBoxDescent: 24,
      }),
      fillText: () => {},
      strokeText: () => {},
      drawImage: () => {},
      save: () => {},
      restore: () => {},
      translate: () => {},
      scale: () => {},
      rotate: () => {},
      fillRect: () => {},
    };
    this.getContext = () => ctx;
    this.transferToImageBitmap = () => ({ width: this.width, height: this.height, close: () => {} });
  } as unknown as typeof OffscreenCanvas;
  vi.stubGlobal("OffscreenCanvas", Mock);
}

const DOC = "text-overlay-gestures";
const START = { x: 40, y: 30 };
const BOX = { w: 200, h: 100 };

let wasm: { protocol_reset: (docId: string) => void } | null = null;
const usedDocs: string[] = [];

beforeAll(async () => {
  wasm = await getWasmExportModule();
});

beforeEach(() => {
  localStorage.setItem("photrez.facade", "1");
  stubOffscreenCanvas();
});

afterEach(() => {
  for (const id of usedDocs) wasm?.protocol_reset(id);
  usedDocs.length = 0;
  localStorage.removeItem("photrez.facade");
  __resetFacadeRegistryForTests();
  (globalThis as unknown as Record<string, () => void>).__clearFacadeOwnedForTests?.();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
  document.body.innerHTML = "";
});

/** Microtask drain for the fire-and-forget command dispatches. */
async function settle(): Promise<void> {
  for (let i = 0; i < 8; i++) await new Promise((resolve) => setTimeout(resolve, 0));
}

interface Harness {
  engine: DocumentEngine;
  history: CommandHistory;
  layerId: string;
  uploadImage: ReturnType<typeof vi.fn>;
  requestRender: ReturnType<typeof vi.fn>;
  setSession: (session: TextEditSession | null) => void;
  readSession: () => TextEditSession | null;
  editor: Record<string, unknown>;
  container: HTMLElement;
  cleanup: () => void;
}

async function setup(opts: { owned: boolean; isNewLayer?: boolean }): Promise<Harness> {
  usedDocs.push(DOC);
  if (!opts.owned) localStorage.removeItem("photrez.facade");

  const engine = new DocumentEngine(DOC, "Doc", 800, 600);
  const data: TextData = {
    ...DEFAULT_TEXT_DATA,
    content: "hello",
    boxMode: "area",
    boxWidth: BOX.w,
    boxHeight: BOX.h,
  };
  const layer = engine.addTextLayer("Text", data);

  if (opts.owned) {
    // The native engine only learns a layer through a protocol arm, so the text
    // layer goes in through the typed-add arm under the id the model already
    // holds: the projection then keeps the model's text type and marks the layer
    // owned. (A layer created by the TS engine alone is invisible to the arm, and
    // a command against it restates nothing.)
    await applyCommand({
      contractVersion: CONTRACT_VERSION,
      expectedVersion: 0,
      docId: DOC,
      command: {
        type: "addLayer",
        id: layer.id,
        name: "Text",
        width: BOX.w,
        height: BOX.h,
        index: 0,
        layerType: "text",
        textData: data,
      } as never,
    });
    const snapshot = await getSnapshot(DOC);
    engine.applyFacadeSnapshot(snapshot as never);
    const facade = getFacade(DOC);
    facade.syncRenderedVersionTo(snapshot.version);
    const parked = await facadeCommitNumericTransform(engine, layer.id, { x: START.x, y: START.y });
    if (!parked) throw new Error("setup: could not park the owned layer");
  } else {
    engine.transformLayer(layer.id, { x: START.x, y: START.y });
  }

  const history = new CommandHistory();
  const uploadImage = vi.fn();
  const requestRender = vi.fn();
  const [session, setSession] = createSignal<TextEditSession | null>(null);
  const editor = {
    workspace: {
      getActiveEngine: () => engine,
      getActiveHistory: () => history,
      notifyVisualChange: () => {},
    },
    renderer: { uploadImage },
    scheduler: { requestRender },
    zoom: () => 1,
    pan: () => ({ x: 0, y: 0 }),
    layers: () => engine.getLayers(),
    textEditSession: session,
    setTextEditSession: setSession,
  };
  mockUseEditor(editor as never);

  const container = document.createElement("div");
  document.body.appendChild(container);
  const dispose = render(() => <TextEditOverlay />, container);

  // Open the session AFTER mount so the overlay registers its pending-content flush.
  setSession({
    layerId: layer.id,
    docX: START.x,
    docY: START.y,
    boxMode: "area",
    boxWidth: BOX.w,
    boxHeight: BOX.h,
    isNewLayer: opts.isNewLayer === true,
    preSnapshot: { layers: [{ id: layer.id, type: "text", textData: { ...layer.textData } }] } as never,
  });
  await settle();

  commandLog.types.length = 0;
  return {
    engine,
    history,
    layerId: layer.id,
    uploadImage,
    requestRender,
    setSession,
    readSession: session,
    editor,
    container,
    cleanup: () => {
      dispose();
      container.parentNode?.removeChild(container);
    },
  };
}

function handle(root: HTMLElement, corner: string): HTMLElement {
  const el = root.querySelector(`[data-text-handle="${corner}"]`) as HTMLElement | null;
  if (!el) throw new Error(`handle ${corner} not rendered`);
  return el;
}

function pointer(type: string, clientX: number, clientY: number): PointerEvent {
  return new PointerEvent(type, { bubbles: true, cancelable: true, clientX, clientY, pointerId: 1 });
}

const textDataOf = (h: Harness): TextData => h.engine.getLayer(h.layerId)!.textData!;

describe("TextEditOverlay corner resize", () => {
  it("owned layer: zero commands per frame, box size in the preview, one transform + one params command at release", async () => {
    const h = await setup({ owned: true });
    expect(isFacadeOwnedLayer(h.layerId)).toBe(true);

    // Top-left corner dragged outward by (-40, -20) at zoom 1: the box grows and
    // its ORIGIN moves, which is what makes the release produce a transform too.
    handle(h.container, "tl").dispatchEvent(pointer("pointerdown", 140, 100));
    for (const [x, y] of [[130, 90], [120, 85], [100, 80]]) {
      window.dispatchEvent(pointer("pointermove", x, y));
    }

    // Frames: nothing dispatched, nothing written to the model.
    expect(commandLog.types).toEqual([]);
    expect(textDataOf(h)).toMatchObject({ boxWidth: BOX.w, boxHeight: BOX.h });
    expect(h.engine.getLayer(h.layerId)!.transform).toMatchObject({ x: START.x, y: START.y });

    // The quad follows the pointer: the size has to ride the preview, because the
    // renderer draws the quad from RenderState width/height and the model is
    // deliberately left at its pre-gesture size until release. Asserted on the
    // merged RenderState the renderer consumes, not only on the signal.
    const preview = transformPreview();
    expect(preview).toHaveLength(1);
    expect(preview[0]).toMatchObject({ layerId: h.layerId, width: 240, height: 120 });
    expect(preview[0].transform).toMatchObject({ x: 0, y: 10 });
    const rendered = applyFacadePreviews(h.engine.getRenderState());
    const previewLayer = rendered.layers.find((l) => l.id === h.layerId)!;
    expect(previewLayer.width).toBe(240);
    expect(previewLayer.height).toBe(120);
    expect(previewLayer.transform).toMatchObject({ x: 0, y: 10 });

    window.dispatchEvent(pointer("pointerup", 100, 80));
    await settle();

    expect(commandLog.types).toEqual(["transformLayer", "setLayerParams"]);
    expect(h.engine.getLayer(h.layerId)!.transform).toMatchObject({ x: 0, y: 10 });
    expect(textDataOf(h)).toMatchObject({ boxMode: "area", boxWidth: 240, boxHeight: 120 });
    expect(transformPreview()).toEqual([]);
    expect(h.history.getUndoCount()).toBe(0);
    h.cleanup();
  });

  it("owned layer: a native history entry exists for the release, and the committed box survives the next metadata restatement", async () => {
    const h = await setup({ owned: true });

    handle(h.container, "tl").dispatchEvent(pointer("pointerdown", 140, 100));
    window.dispatchEvent(pointer("pointermove", 100, 80));
    window.dispatchEvent(pointer("pointerup", 100, 80));
    await settle();

    const committed = textDataOf(h);
    expect(committed.boxWidth).toBe(240);

    // A later arm that does not carry textData must not revert it: the projection
    // keeps the model value when the field is absent (see applyFacadeSnapshot).
    const layer = h.engine.getLayer(h.layerId)!;
    await facadeCommitNumericTransform(h.engine, h.layerId, { x: layer.transform.x + 5 });
    expect(textDataOf(h).boxWidth).toBe(240);
    expect(textDataOf(h).content).toBe(committed.content);
    h.cleanup();
  });

  it("flag OFF: per-frame model writes and bitmap uploads stay as they were, no facade call", async () => {
    const h = await setup({ owned: false });
    expect(isFacadeOwnedLayer(h.layerId)).toBe(false);

    handle(h.container, "tl").dispatchEvent(pointer("pointerdown", 140, 100));
    window.dispatchEvent(pointer("pointermove", 120, 85));
    window.dispatchEvent(pointer("pointermove", 100, 80));

    expect(commandLog.types).toEqual([]);

    // The legacy branch still writes the model every frame: origin here, box dims
    // through updateTextData, both re-uploaded to the renderer.
    expect(h.engine.getLayer(h.layerId)!.transform).toMatchObject({ x: 0, y: 10 });
    expect(textDataOf(h)).toMatchObject({ boxWidth: 240, boxHeight: 120 });
    expect(h.uploadImage).toHaveBeenCalledTimes(2);
    expect(transformPreview()).toEqual([]);

    window.dispatchEvent(pointer("pointerup", 100, 80));
    await settle();
    expect(commandLog.types).toEqual([]);

    commitTextSession({
      workspace: { getActiveEngine: () => h.engine, getActiveHistory: () => h.history },
      renderer: { uploadImage: h.uploadImage },
      scheduler: { requestRender: h.requestRender },
      textEditSession: h.readSession,
      setTextEditSession: h.setSession,
    } as never);

    expect(h.history.getUndoCount()).toBe(1);
    h.cleanup();
  });

  it("abandon mid-resize: preview cleared, texture taken back from the model, slot free for a later numeric edit", async () => {
    const h = await setup({ owned: true });
    const modelBitmap = h.engine.getLayer(h.layerId)!.imageBitmap;

    handle(h.container, "tl").dispatchEvent(pointer("pointerdown", 140, 100));
    window.dispatchEvent(pointer("pointermove", 100, 80));
    expect(transformPreview()).toHaveLength(1);

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));

    expect(transformPreview()).toEqual([]);
    expect(h.uploadImage.mock.calls.at(-1)?.[1]).toBe(modelBitmap);
    expect(peekFacade(DOC)?.transientTransformActive()).toBe(false);
    // The model never moved, so the session is back at the box it started from.
    expect(h.engine.getLayer(h.layerId)!.transform).toMatchObject({ x: START.x, y: START.y });
    expect(textDataOf(h)).toMatchObject({ boxWidth: BOX.w });

    const landed = await facadeCommitNumericTransform(h.engine, h.layerId, { x: 77 });
    expect(landed).toBe(true);
    expect(h.engine.getLayer(h.layerId)!.transform.x).toBe(77);
    h.cleanup();
  });

  it("never-owned temp layer: both sides of the flag keep the legacy per-frame path", async () => {
    const on = await setup({ owned: false, isNewLayer: true });
    const off = await setup({ owned: false, isNewLayer: true });

    for (const h of [on, off]) {
      handle(h.container, "tl").dispatchEvent(pointer("pointerdown", 140, 100));
      window.dispatchEvent(pointer("pointermove", 100, 80));
      expect(commandLog.types).toEqual([]);
      expect(h.engine.getLayer(h.layerId)!.transform).toMatchObject({ x: 0, y: 10 });
      expect(transformPreview()).toEqual([]);
      window.dispatchEvent(pointer("pointerup", 100, 80));
      await settle();
      h.cleanup();
    }
  });
});

describe("TextEditOverlay content path", () => {
  it("owned layer: typing does not write the model, and the session close sends exactly one params command", async () => {
    const h = await setup({ owned: true });
    const textarea = h.container.querySelector("[data-text-edit-overlay]") as HTMLTextAreaElement;

    textarea.value = "hello world";
    textarea.dispatchEvent(new InputEvent("input", { bubbles: true }));
    await settle();

    // The keystrokes are previewed host-side only: engine.updateTextData has no
    // ownership guard, so writing the model here would let the TS copy and the
    // native arm drift until some later projection clobbered one of them.
    expect(textDataOf(h).content).toBe("hello");
    expect(commandLog.types).toEqual([]);

    textarea.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", ctrlKey: true, bubbles: true }));
    await settle();

    expect(commandLog.types).toEqual(["setLayerParams"]);
    expect(textDataOf(h).content).toBe("hello world");
    // The native arm recorded the entry; a TS entry would strand an undo point
    // whose engine.restore() rejects with E_FACADE_OWNED.
    expect(h.history.getUndoCount()).toBe(0);
    h.cleanup();
  });

  it("owned layer: Escape discards the typing without a command and without a model write", async () => {
    const h = await setup({ owned: true });
    const textarea = h.container.querySelector("[data-text-edit-overlay]") as HTMLTextAreaElement;

    textarea.value = "throw me away";
    textarea.dispatchEvent(new InputEvent("input", { bubbles: true }));
    await settle();
    textarea.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    await settle();

    expect(commandLog.types).toEqual([]);
    expect(textDataOf(h).content).toBe("hello");
    expect(h.history.getUndoCount()).toBe(0);
    expect(h.container.querySelector("[data-text-edit-overlay]")).toBeNull();
    h.cleanup();
  });

  it("flag OFF: typing still writes the model through the debounced push", async () => {
    const h = await setup({ owned: false });
    const textarea = h.container.querySelector("[data-text-edit-overlay]") as HTMLTextAreaElement;

    textarea.value = "hello world";
    textarea.dispatchEvent(new InputEvent("input", { bubbles: true }));
    await new Promise((resolve) => setTimeout(resolve, 80));

    expect(textDataOf(h).content).toBe("hello world");
    expect(commandLog.types).toEqual([]);
    h.cleanup();
  });
});

describe("widened transform preview", () => {
  it("a transform-only preview leaves the layer size untouched, and an unrelated layer passes through", () => {
    setTransformPreview([
      { layerId: "a", transform: { x: 5, y: 6, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false } },
    ]);
    const rs = applyFacadePreviews({
      layers: [
        { id: "a", transform: {}, width: 111, height: 222 },
        { id: "b", transform: {}, width: 333, height: 444 },
      ],
    } as never);

    // Every other producer (canvas drag, group transform, transform bar, selection
    // overlay) sends { layerId, transform } only, so the missing-field spread must
    // be a no-op: the renderer keeps reading the model size.
    expect((rs.layers[0] as { width?: number }).width).toBe(111);
    expect((rs.layers[0] as { height?: number }).height).toBe(222);
    expect((rs.layers[1] as { width?: number }).width).toBe(333);
  });
});
