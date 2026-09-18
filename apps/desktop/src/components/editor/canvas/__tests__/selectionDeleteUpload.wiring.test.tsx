import { describe, it, expect, vi, afterEach } from "vitest";
import { render } from "solid-js/web";
import { DocumentEngine } from "@/engine/document";
import { CommandHistory } from "@/engine/history";
import { SelectionOperations } from "@/features/selection/SelectionOperations";
import { handleSelectionToolKey } from "../keyboardShortcuts/selectionTool";
import type { KeyboardShortcutContext } from "../keyboardShortcuts/context";
import { mockUseEditor } from "@/__tests__/mockUseEditor";
import { SelectionOptionBar } from "../../SelectionOptionBar";
import { useEditorCommands } from "../../useEditorCommands";
import * as DialogProviderModule from "../../dialogs/DialogProvider";

/**
 * Wiring test for the delete-path texture upload granularity.
 *
 * Cut/Delete replace only the selected pixels, so the re-upload after the
 * real handleSelectionToolKey path must carry the selection AABB as the
 * upload dirtyRect (PATCH) instead of re-uploading the whole layer (FULL).
 * Inverted selections change the whole layer, so they keep the 2-arg FULL
 * call. Paste creates a brand-new layer id with no existing texture, so it
 * also keeps the 2-arg FULL call.
 */

function stubCanvas() {
  vi.stubGlobal(
    "OffscreenCanvas",
    class {
      width: number;
      height: number;
      constructor(w: number, h: number) {
        this.width = w;
        this.height = h;
      }
      getContext() {
        return {
          drawImage: () => {},
          clearRect: () => {},
          getImageData: () => ({ data: new Uint8ClampedArray(4) }),
          putImageData: () => {},
        };
      }
      transferToImageBitmap() {
        return { width: this.width, height: this.height, close: () => {} };
      }
    },
  );
}

function setup(selX: number, selY: number, selW: number, selH: number) {
  stubCanvas();
  const engine = new DocumentEngine("del-upload", "D", 100, 100);
  const layer = engine.addLayer("L", 100, 100);
  engine.setActiveLayer(layer.id);
  engine.setLayerImageBitmap(layer.id, { width: 100, height: 100 } as ImageBitmap);
  engine.createSelection(selX, selY, selW, selH);
  const history = new CommandHistory();
  const uploadImage = vi.fn();
  const ctx = {
    editor: {
      activeTool: () => "selection",
      scheduler: { requestRender: vi.fn() },
      renderer: { uploadImage },
      setSelectionEditMode: vi.fn(),
      selectionEditMode: () => false,
    },
    options: { onSelectionChange: vi.fn() },
  } as unknown as KeyboardShortcutContext;
  return { engine, history, ctx, uploadImage, layerId: layer.id };
}

describe("selection delete upload granularity", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
  });

  it("Delete passes the selection AABB as the upload dirtyRect", () => {
    const { engine, history, ctx, uploadImage, layerId } = setup(10, 10, 20, 20);

    const handled = handleSelectionToolKey(
      ctx,
      new KeyboardEvent("keydown", { key: "Delete", bubbles: true }),
      engine,
      history,
    );

    expect(handled).toBe(true);
    expect(uploadImage).toHaveBeenCalledTimes(1);
    expect(uploadImage).toHaveBeenCalledWith(layerId, expect.anything(), {
      x: 10,
      y: 10,
      width: 20,
      height: 20,
    });
  });

  it("Cut passes the selection AABB as the upload dirtyRect", () => {
    const { engine, history, ctx, uploadImage, layerId } = setup(10, 10, 20, 20);

    const handled = handleSelectionToolKey(
      ctx,
      new KeyboardEvent("keydown", { key: "x", ctrlKey: true, bubbles: true }),
      engine,
      history,
    );

    expect(handled).toBe(true);
    expect(uploadImage).toHaveBeenCalledTimes(1);
    expect(uploadImage).toHaveBeenCalledWith(layerId, expect.anything(), {
      x: 10,
      y: 10,
      width: 20,
      height: 20,
    });
  });

  it("Delete clamps a selection hanging off the layer edge", () => {
    const { engine, history, ctx, uploadImage, layerId } = setup(80, 80, 50, 50);

    handleSelectionToolKey(
      ctx,
      new KeyboardEvent("keydown", { key: "Delete", bubbles: true }),
      engine,
      history,
    );

    expect(uploadImage).toHaveBeenCalledWith(layerId, expect.anything(), {
      x: 80,
      y: 80,
      width: 20,
      height: 20,
    });
  });

  it("Delete of an inverted selection keeps the FULL 2-arg upload", () => {
    const { engine, history, ctx, uploadImage, layerId } = setup(10, 10, 20, 20);
    engine.invertSelection();

    handleSelectionToolKey(
      ctx,
      new KeyboardEvent("keydown", { key: "Delete", bubbles: true }),
      engine,
      history,
    );

    expect(uploadImage).toHaveBeenCalledTimes(1);
    expect(uploadImage).toHaveBeenCalledWith(layerId, expect.anything());
    expect(uploadImage.mock.calls[0]).toHaveLength(2);
  });

  it("Delete with a rotated selection covers the full writer-cleared range", () => {
    // A rotated marquee lands between pixels. Sweep real rotations for one
    // whose fractional AABB splits the two roundings, so this case cannot
    // pass by float luck on an integer-equivalent input.
    const identity = {
      x: 0,
      y: 0,
      scaleX: 1,
      scaleY: 1,
      rotation: 0,
      flipH: false,
      flipV: false,
    };
    let angle = 0;
    for (let a = 1; a < 90; a++) {
      const t = SelectionOperations.selectionToLayerAabb(
        { x: 10, y: 10, width: 20, height: 20, angle: a },
        identity,
        100,
        100,
      );
      const splitX = Math.round(t.x + t.width) !== Math.round(t.x) + Math.round(t.width);
      const splitY = Math.round(t.y + t.height) !== Math.round(t.y) + Math.round(t.height);
      if (splitX || splitY) {
        angle = a;
        break;
      }
    }
    expect(angle).toBeGreaterThan(0);
    const { engine, history, ctx, uploadImage } = setup(10, 10, 20, 20);
    engine.createSelection(10, 10, 20, 20, angle);
    const sel = engine.getSelection();
    const layer = engine.getLayer(engine.getActiveLayerId()!);
    const aabb = SelectionOperations.selectionToLayerAabb(
      sel!,
      layer!.transform,
      layer!.width,
      layer!.height,
    );
    // A rotated marquee lands between pixels; without a fractional input
    // this test cannot prove the rounding contract.
    expect(aabb.x % 1 !== 0 || aabb.width % 1 !== 0).toBe(true);
    // The pixel writer clears [round(x), round(x) + round(w)).
    const wx = Math.round(aabb.x);
    const wy = Math.round(aabb.y);
    const ww = Math.round(aabb.width);
    const wh = Math.round(aabb.height);

    handleSelectionToolKey(
      ctx,
      new KeyboardEvent("keydown", { key: "Delete", bubbles: true }),
      engine,
      history,
    );

    expect(uploadImage).toHaveBeenCalledTimes(1);
    const rect = uploadImage.mock.calls[0][2] as {
      x: number;
      y: number;
      width: number;
      height: number;
    };
    expect(rect.x).toBeLessThanOrEqual(wx);
    expect(rect.y).toBeLessThanOrEqual(wy);
    expect(rect.x + rect.width).toBeGreaterThanOrEqual(wx + ww);
    expect(rect.y + rect.height).toBeGreaterThanOrEqual(wy + wh);
  });

  it("menu Cut passes the selection AABB as the upload dirtyRect", () => {
    stubCanvas();
    vi.spyOn(DialogProviderModule, "useDialog").mockReturnValue(
      {} as unknown as ReturnType<typeof DialogProviderModule.useDialog>,
    );
    const engine = new DocumentEngine("menu-cut", "D", 100, 100);
    const layer = engine.addLayer("L", 100, 100);
    engine.setActiveLayer(layer.id);
    engine.setLayerImageBitmap(layer.id, { width: 100, height: 100 } as ImageBitmap);
    engine.createSelection(10, 10, 20, 20);
    const history = new CommandHistory();
    const uploadImage = vi.fn();
    mockUseEditor({
      workspace: {
        getActiveEngine: () => engine,
        getActiveHistory: () => history,
        getActiveDocumentId: () => "menu-cut",
        notifyVisualChange: () => {},
      },
      renderer: { uploadImage },
      scheduler: { requestRender: vi.fn() },
      activeDocumentId: () => "menu-cut",
      layerTransformSession: () => null,
      setLayerTransformSession: vi.fn(),
      activeTool: () => "selection",
    });

    const commands = useEditorCommands(() => {});
    commands.execute("edit.cut");

    expect(uploadImage).toHaveBeenCalledTimes(1);
    expect(uploadImage).toHaveBeenCalledWith(layer.id, expect.anything(), {
      x: 10,
      y: 10,
      width: 20,
      height: 20,
    });
  });

  it("option-bar Cut button passes the selection AABB as the upload dirtyRect", () => {
    stubCanvas();
    const engine = new DocumentEngine("bar-cut", "D", 100, 100);
    const layer = engine.addLayer("L", 100, 100);
    engine.setActiveLayer(layer.id);
    engine.setLayerImageBitmap(layer.id, { width: 100, height: 100 } as ImageBitmap);
    engine.createSelection(10, 10, 20, 20);
    const history = new CommandHistory();
    const uploadImage = vi.fn();
    mockUseEditor({
      workspace: {
        getActiveEngine: () => engine,
        getActiveHistory: () => history,
      },
      renderer: { uploadImage },
      scheduler: { requestRender: vi.fn() },
      selection: () => engine.getSelection(),
      activeTool: () => "selection",
      selectionEditMode: () => false,
      setSelectionEditMode: vi.fn(),
      selectionConstraintMode: () => "normal",
      setSelectionConstraintMode: vi.fn(),
      selectionRatioW: () => 1,
      setSelectionRatioW: vi.fn(),
      selectionRatioH: () => 1,
      setSelectionRatioH: vi.fn(),
      selectionSizeW: () => 100,
      setSelectionSizeW: vi.fn(),
      selectionSizeH: () => 100,
      setSelectionSizeH: vi.fn(),
      selectionShape: () => "rect",
      setSelectionShape: vi.fn(),
    });

    const root = document.createElement("div");
    document.body.appendChild(root);
    const dispose = render(() => <SelectionOptionBar />, root);
    const cutBtn = Array.from(root.querySelectorAll("button")).find((b) =>
      b.textContent?.includes("Cut"),
    );
    expect(cutBtn).toBeDefined();
    cutBtn!.click();
    dispose();
    root.remove();

    expect(uploadImage).toHaveBeenCalledTimes(1);
    expect(uploadImage).toHaveBeenCalledWith(layer.id, expect.anything(), {
      x: 10,
      y: 10,
      width: 20,
      height: 20,
    });
  });

  it("option-bar Delete button passes the selection AABB as the upload dirtyRect", () => {
    stubCanvas();
    const engine = new DocumentEngine("bar-delete", "D", 100, 100);
    const layer = engine.addLayer("L", 100, 100);
    engine.setActiveLayer(layer.id);
    engine.setLayerImageBitmap(layer.id, { width: 100, height: 100 } as ImageBitmap);
    engine.createSelection(10, 10, 20, 20);
    const history = new CommandHistory();
    const uploadImage = vi.fn();
    mockUseEditor({
      workspace: {
        getActiveEngine: () => engine,
        getActiveHistory: () => history,
      },
      renderer: { uploadImage },
      scheduler: { requestRender: vi.fn() },
      selection: () => engine.getSelection(),
      activeTool: () => "selection",
      selectionEditMode: () => false,
      setSelectionEditMode: vi.fn(),
      selectionConstraintMode: () => "normal",
      setSelectionConstraintMode: vi.fn(),
      selectionRatioW: () => 1,
      setSelectionRatioW: vi.fn(),
      selectionRatioH: () => 1,
      setSelectionRatioH: vi.fn(),
      selectionSizeW: () => 100,
      setSelectionSizeW: vi.fn(),
      selectionSizeH: () => 100,
      setSelectionSizeH: vi.fn(),
      selectionShape: () => "rect",
      setSelectionShape: vi.fn(),
    });

    const root = document.createElement("div");
    document.body.appendChild(root);
    const dispose = render(() => <SelectionOptionBar />, root);
    const delBtn = Array.from(root.querySelectorAll("button")).find((b) =>
      b.textContent?.includes("Delete"),
    );
    expect(delBtn).toBeDefined();
    delBtn!.click();
    dispose();
    root.remove();

    expect(uploadImage).toHaveBeenCalledTimes(1);
    expect(uploadImage).toHaveBeenCalledWith(layer.id, expect.anything(), {
      x: 10,
      y: 10,
      width: 20,
      height: 20,
    });
  });
});
