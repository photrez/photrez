// apps/desktop/src/components/editor/__tests__/textToolWiring.test.tsx
//
// Wiring contract for the text tool pointer chain (plan §8.3, the
// "most-often-forgotten" dispatcher step):
//   pointerdown empty        → addTextLayer (temp) + textEditSession opened
//   pointerdown text layer   → re-edit session (NO new layer)
//   pointermove >3px         → updateTextData boxMode "area" + boxWidth
//   pointerup                → session stays open (no commit yet)
//   commitTextSession        → "Add Text" / "Edit Text" | empty new → deleted
//   cancelTextSession        → temp layer removed, no history entry
//   double-click text layer  → switches to text tool + opens edit session

import { mockUseEditor } from "@/__tests__/mockUseEditor";
import { describe, it, expect, vi } from "vitest";
import { createMockEditorParams, createPointerTools, makePointerEvent } from "../../../__tests__/pointerRoutingHarness";
import { commitTextSession, cancelTextSession, type TextSessionEditor } from "../canvas/pointerTools/textTool";
import type { TextData } from "@/engine/textTypes";

vi.mock("../dialogs/DialogProvider", () => ({
  useDialog: () => ({ confirm: vi.fn().mockResolvedValue(false) }),
}));

const TEXT_DATA: TextData = {
  content: "Hello",
  fontFamily: "Arial",
  fontSize: 48,
  fontWeight: 400,
  fontStyle: "normal",
  color: "#000000",
  align: "left",
  lineHeight: 1.4,
  letterSpacing: 0,
  boxMode: "point",
  boxWidth: 0,
};

function makePointerTools(signals: Record<string, any>) {
  mockUseEditor(signals);
  const params = {
    getCanvasContainerRef: () => document.createElement("div"),
    getCanvasRef: () => document.createElement("canvas"),
    isSpacePressed: () => false,
    isPanning: () => false,
    isAltPressed: () => false,
    stopMomentum: vi.fn(),
    fitToScreenAndRender: vi.fn(),
    commitBrushStroke: vi.fn(),
  };
  return createPointerTools(params);
}

function textLayer(id: string, over: Partial<TextData> = {}) {
  return {
    id, type: "text", name: "Text", width: 100, height: 50,
    visible: true, locked: false,
    transform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0 },
    textData: { ...TEXT_DATA, ...over },
  };
}

describe("text tool pointer wiring", () => {
  it("pointerdown on empty creates a temp text layer and opens a session", () => {
    const { signals, mockEngine, dispose } = createMockEditorParams("text");
    (mockEngine as any).addTextLayer = vi.fn(() => ({ id: "text-1", type: "text" }));

    const { tools, dispose: disposeTools } = makePointerTools(signals);
    tools.onCanvasPointerDown(makePointerEvent({ clientX: 10, clientY: 10 }));

    expect(mockEngine.addTextLayer).toHaveBeenCalled();
    // Temp layer positioned at the click point, session opened as a NEW layer.
    expect(mockEngine.transformLayer).toHaveBeenCalledWith("text-1", { x: 10, y: 10 });
    const session = signals.textEditSession();
    expect(session).not.toBeNull();
    expect(session!.layerId).toBe("text-1");
    expect(session!.isNewLayer).toBe(true);
    expect(session!.boxMode).toBe("point");
    disposeTools();
    dispose();
  });

  it("pointerup keeps the session open (history commits only at session close)", () => {
    const { signals, mockEngine, dispose } = createMockEditorParams("text");
    const history = signals.workspace.getActiveHistory();
    (mockEngine as any).addTextLayer = vi.fn(() => ({ id: "text-1", type: "text" }));

    const { tools, dispose: disposeTools } = makePointerTools(signals);
    tools.onCanvasPointerDown(makePointerEvent({ clientX: 10, clientY: 10 }));
    tools.onCanvasPointerUp(makePointerEvent({ clientX: 10, clientY: 10 }));

    expect(history.commit).not.toHaveBeenCalled();
    expect(signals.textEditSession()).not.toBeNull();
    disposeTools();
    dispose();
  });

  it("pointerdown on an existing text layer starts a re-edit session (no new layer)", () => {
    const { signals, mockEngine, dispose } = createMockEditorParams("text");
    (mockEngine as any).getLayers = vi.fn(() => [textLayer("text-1")]);
    (mockEngine as any).getLayer = vi.fn((id: string) => textLayer(id));
    (mockEngine as any).addTextLayer = vi.fn();

    const { tools, dispose: disposeTools } = makePointerTools(signals);
    tools.onCanvasPointerDown(makePointerEvent({ clientX: 10, clientY: 10 }));

    expect(mockEngine.addTextLayer).not.toHaveBeenCalled();
    const session = signals.textEditSession();
    expect(session).not.toBeNull();
    expect(session!.layerId).toBe("text-1");
    expect(session!.isNewLayer).toBe(false);
    expect(session!.boxWidth).toBe(0);
    disposeTools();
    dispose();
  });

  it("drag beyond 3px turns the temp layer into an area box (boxMode area + boxWidth)", () => {
    const { signals, mockEngine, dispose } = createMockEditorParams("text");
    (mockEngine as any).addTextLayer = vi.fn(() => ({ id: "text-1", type: "text" }));
    (mockEngine as any).getLayer = vi.fn((id: string) => textLayer(id, { boxMode: "point", boxWidth: 0 }));
    (mockEngine as any).getLayerImageBitmap = vi.fn(() => document.createElement("canvas"));
    (mockEngine as any).updateTextData = vi.fn();

    const { tools, dispose: disposeTools } = makePointerTools(signals);
    tools.onCanvasPointerDown(makePointerEvent({ clientX: 10, clientY: 10 }));
    tools.onCanvasPointerMove(makePointerEvent({ clientX: 110, clientY: 10 }));

    expect(mockEngine.updateTextData).toHaveBeenCalled();
    const [id, data] = (mockEngine as any).updateTextData.mock.calls.at(-1)!;
    expect(id).toBe("text-1");
    expect(data.boxMode).toBe("area");
    expect(data.boxWidth).toBe(100);
    expect(signals.textEditSession()!.boxMode).toBe("area");
    expect(signals.textEditSession()!.boxWidth).toBe(100);
    disposeTools();
    dispose();
  });

  it("commit with typed content records one 'Add Text' undo step", () => {
    const { signals, mockEngine, dispose } = createMockEditorParams("text");
    const history = signals.workspace.getActiveHistory();
    (mockEngine as any).addTextLayer = vi.fn(() => ({ id: "text-1", type: "text" }));
    (mockEngine as any).getLayer = vi.fn((id: string) => textLayer(id, { content: "Hello" }));

    const { tools, dispose: disposeTools } = makePointerTools(signals);
    tools.onCanvasPointerDown(makePointerEvent({ clientX: 10, clientY: 10 }));

    commitTextSession(signals as unknown as TextSessionEditor);

    expect(history.commit).toHaveBeenCalledWith(expect.anything(), "Add Text");
    expect(mockEngine.deleteLayer).not.toHaveBeenCalled();
    expect(signals.textEditSession()).toBeNull();
    disposeTools();
    dispose();
  });

  it("empty-commit cleanup: new layer with no content is deleted without a history entry", () => {
    const { signals, mockEngine, dispose } = createMockEditorParams("text");
    const history = signals.workspace.getActiveHistory();
    (mockEngine as any).addTextLayer = vi.fn(() => ({ id: "text-1", type: "text" }));
    (mockEngine as any).getLayer = vi.fn((id: string) => textLayer(id, { content: "" }));

    const { tools, dispose: disposeTools } = makePointerTools(signals);
    tools.onCanvasPointerDown(makePointerEvent({ clientX: 10, clientY: 10 }));

    commitTextSession(signals as unknown as TextSessionEditor);

    expect(mockEngine.deleteLayer).toHaveBeenCalledWith("text-1");
    expect(history.commit).not.toHaveBeenCalled();
    expect(signals.textEditSession()).toBeNull();
    disposeTools();
    dispose();
  });

  it("re-edit commit records 'Edit Text'; unchanged content produces no ghost entry", () => {
    const { signals, mockEngine, dispose } = createMockEditorParams("text");
    const history = signals.workspace.getActiveHistory();
    const layer = textLayer("text-1", { content: "Hello" });
    (mockEngine as any).getLayers = vi.fn(() => [layer]);
    (mockEngine as any).getLayer = vi.fn((id: string) => layer);
    // Snapshot carries the SAME textData → nothing changed → no commit.
    (mockEngine as any).snapshot = vi.fn(() => ({ layers: [layer] }));

    const { tools, dispose: disposeTools } = makePointerTools(signals);
    tools.onCanvasPointerDown(makePointerEvent({ clientX: 10, clientY: 10 }));

    commitTextSession(signals as unknown as TextSessionEditor);
    expect(history.commit).not.toHaveBeenCalled();
    expect(signals.textEditSession()).toBeNull();

    // Changed content → commits "Edit Text".
    (mockEngine as any).snapshot = vi.fn(() => ({ layers: [textLayer("text-1", { content: "Old" })] }));
    tools.onCanvasPointerDown(makePointerEvent({ clientX: 10, clientY: 10 }));
    // Simulate typing: layer content is now different from the pre-snapshot.
    (mockEngine as any).getLayer = vi.fn((id: string) => textLayer(id, { content: "Hello world" }));
    commitTextSession(signals as unknown as TextSessionEditor);
    expect(history.commit).toHaveBeenCalledWith(expect.anything(), "Edit Text");
    disposeTools();
    dispose();
  });

  it("cancel removes a temp (new) layer with no history entry", () => {
    const { signals, mockEngine, dispose } = createMockEditorParams("text");
    const history = signals.workspace.getActiveHistory();
    (mockEngine as any).addTextLayer = vi.fn(() => ({ id: "text-1", type: "text" }));

    const { tools, dispose: disposeTools } = makePointerTools(signals);
    tools.onCanvasPointerDown(makePointerEvent({ clientX: 10, clientY: 10 }));

    cancelTextSession(signals as unknown as TextSessionEditor);

    expect(mockEngine.deleteLayer).toHaveBeenCalledWith("text-1");
    expect(history.commit).not.toHaveBeenCalled();
    expect(signals.textEditSession()).toBeNull();
    disposeTools();
    dispose();
  });

  it("double-click on a text layer switches to the text tool and opens an edit session", () => {
    const { signals, mockEngine, dispose } = createMockEditorParams("move");
    const layer = textLayer("text-1", { content: "Hello" });
    (mockEngine as any).getLayers = vi.fn(() => [layer]);
    (mockEngine as any).getLayer = vi.fn((id: string) => layer);
    // Spy the setter so the tool switch is observable (harness builds plain
    // createSignal setters, not spies). The accessor must read "text" once the
    // switch happens so startTextPointer's activeTool guard passes.
    const setActiveToolSpy = vi.fn(() => signals.activeTool = () => "text");
    signals.setActiveTool = setActiveToolSpy;

    const { tools, dispose: disposeTools } = makePointerTools(signals);
    tools.handleDoubleClick({ clientX: 10, clientY: 10, target: document.createElement("div") } as unknown as MouseEvent);

    expect(setActiveToolSpy).toHaveBeenCalledWith("text");
    const session = signals.textEditSession();
    expect(session).not.toBeNull();
    expect(session!.layerId).toBe("text-1");
    expect(session!.isNewLayer).toBe(false);
    disposeTools();
    dispose();
  });

  it("pointercancel cancels the session and removes the temp layer", () => {
    const { signals, mockEngine, dispose } = createMockEditorParams("text");
    (mockEngine as any).addTextLayer = vi.fn(() => ({ id: "text-1", type: "text" }));

    const { tools, dispose: disposeTools } = makePointerTools(signals);
    tools.onCanvasPointerDown(makePointerEvent({ clientX: 10, clientY: 10 }));
    tools.onCanvasPointerCancel(makePointerEvent());

    expect(mockEngine.deleteLayer).toHaveBeenCalledWith("text-1");
    expect(signals.textEditSession()).toBeNull();
    disposeTools();
    dispose();
  });
});
