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
import { commitTextSession, cancelTextSession, openTextEditSession, setPendingTextFlush, flushPendingText, syncTextSessionBase, type TextSessionEditor, type TextSessionOpener } from "../canvas/pointerTools/textTool";
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
  boxHeight: 0,
  stroke: { width: 0, color: "#000000" },
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

  it("re-edit session anchors at the LAYER origin, not the click point (B1 regression)", () => {
    const { signals, mockEngine, dispose } = createMockEditorParams("text");
    const layer = textLayer("text-1", { content: "Hello" });
    layer.transform = { x: 40, y: 30, scaleX: 1, scaleY: 1, rotation: 0 };
    (mockEngine as any).getLayers = vi.fn(() => [layer]);
    (mockEngine as any).getLayer = vi.fn((id: string) => layer);

    const { tools, dispose: disposeTools } = makePointerTools(signals);
    // Click lands inside the layer's box (x 40..140, y 30..80) but far from
    // its origin — the overlay must cover the ACTUAL text, so docX/docY are
    // the layer transform, never the click point.
    tools.onCanvasPointerDown(makePointerEvent({ clientX: 60, clientY: 40 }));

    const session = signals.textEditSession();
    expect(session).not.toBeNull();
    expect(session!.layerId).toBe("text-1");
    expect(session!.isNewLayer).toBe(false);
    expect(session!.docX).toBe(40);
    expect(session!.docY).toBe(30);
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

  it("drag beyond MIN_AREA_PX diagonally sets both boxWidth and boxHeight in 2D", () => {
    const { signals, mockEngine, dispose } = createMockEditorParams("text");
    (mockEngine as any).addTextLayer = vi.fn(() => ({ id: "text-1", type: "text" }));
    (mockEngine as any).getLayer = vi.fn((id: string) => textLayer(id));

    const { tools, dispose: disposeTools } = makePointerTools(signals);
    tools.onCanvasPointerDown(makePointerEvent({ clientX: 10, clientY: 10 }));
    tools.onCanvasPointerMove(makePointerEvent({ clientX: 210, clientY: 160 }));

    expect(mockEngine.updateTextData).toHaveBeenCalled();
    const [id, data] = (mockEngine as any).updateTextData.mock.calls.at(-1)!;
    expect(id).toBe("text-1");
    expect(data.boxMode).toBe("area");
    expect(data.boxWidth).toBe(200);
    expect(data.boxHeight).toBe(150);
    expect(signals.textEditSession()!.boxMode).toBe("area");
    expect(signals.textEditSession()!.boxWidth).toBe(200);
    expect(signals.textEditSession()!.boxHeight).toBe(150);
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
    expect(mockEngine.restore).not.toHaveBeenCalled();
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
    // Overlay anchor = layer origin (0,0), not the double-click point (10,10).
    expect(session!.docX).toBe(0);
    expect(session!.docY).toBe(0);
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

  it("cancel on a RE-EDIT restores the pre-session snapshot (no ghost mutation)", () => {
    const { signals, mockEngine, dispose } = createMockEditorParams("text");
    const pre = { layers: [textLayer("text-1", { content: "Hello" })] };
    // Typing already live-mutated the layer through the debounced push.
    (mockEngine as any).getLayer = vi.fn(() => textLayer("text-1", { content: "Hello world" }));
    (mockEngine as any).restore = vi.fn();
    signals.setTextEditSession({ layerId: "text-1", docX: 0, docY: 0, boxMode: "point", boxWidth: 0, isNewLayer: false, preSnapshot: pre });

    const { tools, dispose: disposeTools } = makePointerTools(signals);
    cancelTextSession(signals as unknown as TextSessionEditor);

    // Cancel must roll the ENGINE back (viewport untouched) — not leave the
    // mutated text behind with no undo entry.
    expect((mockEngine as any).restore).toHaveBeenCalledWith(pre, { restoreViewport: false });
    expect(signals.textEditSession()).toBeNull();
    disposeTools();
    dispose();
  });

  it("cancel on a RE-EDIT whose layer was deleted externally closes WITHOUT restoring (B6)", () => {
    const { signals, mockEngine, dispose } = createMockEditorParams("text");
    setPendingTextFlush("text-1", null);
    const pre = { layers: [textLayer("text-1", { content: "Hello" })] };
    // Layer deleted externally (e.g. layer-panel Delete) while the session was
    // open — restoring the pre-snapshot here would RESURRECT the deleted layer.
    (mockEngine as any).getLayer = vi.fn(() => undefined);
    (mockEngine as any).restore = vi.fn();
    signals.setTextEditSession({ layerId: "text-1", docX: 0, docY: 0, boxMode: "point", boxWidth: 0, isNewLayer: false, preSnapshot: pre });

    const { tools, dispose: disposeTools } = makePointerTools(signals);
    cancelTextSession(signals as unknown as TextSessionEditor);

    expect((mockEngine as any).restore).not.toHaveBeenCalled();
    expect(signals.textEditSession()).toBeNull();
    disposeTools();
    dispose();
  });

  it("commit on a layer deleted externally closes with NO ghost 'Edit Text' entry (B6)", () => {
    const { signals, mockEngine, dispose } = createMockEditorParams("text");
    setPendingTextFlush("text-1", null);
    const history = signals.workspace.getActiveHistory();
    (mockEngine as any).getLayer = vi.fn(() => undefined);
    signals.setTextEditSession({
      layerId: "text-1", docX: 0, docY: 0, boxMode: "point", boxWidth: 0,
      isNewLayer: false, preSnapshot: { layers: [textLayer("text-1", { content: "Hello" })] },
    });

    const { tools, dispose: disposeTools } = makePointerTools(signals);
    commitTextSession(signals as unknown as TextSessionEditor);

    // No ghost commit: the deletion already recorded its own undo step, and
    // committing the stale preSnapshot would re-add the layer on undo.
    expect(history.commit).not.toHaveBeenCalled();
    expect(signals.textEditSession()).toBeNull();
    disposeTools();
    dispose();
  });

  it("drag to the LEFT puts the area box at the left edge (box follows the drag)", () => {
    const { signals, mockEngine, dispose } = createMockEditorParams("text");
    (mockEngine as any).addTextLayer = vi.fn(() => ({ id: "text-1", type: "text" }));
    (mockEngine as any).getLayer = vi.fn((id: string) => textLayer(id, { boxMode: "point", boxWidth: 0 }));
    (mockEngine as any).getLayerImageBitmap = vi.fn(() => document.createElement("canvas"));
    (mockEngine as any).updateTextData = vi.fn();
    (mockEngine as any).transformLayer = vi.fn();

    const { tools, dispose: disposeTools } = makePointerTools(signals);
    tools.onCanvasPointerDown(makePointerEvent({ clientX: 110, clientY: 10 }));
    tools.onCanvasPointerMove(makePointerEvent({ clientX: 10, clientY: 10 }));

    expect((mockEngine as any).transformLayer).toHaveBeenLastCalledWith("text-1", { x: 10, y: 10 });
    const s = signals.textEditSession();
    expect(s!.boxMode).toBe("area");
    expect(s!.boxWidth).toBe(100);
    expect(s!.docX).toBe(10);
    disposeTools();
    dispose();
  });

  it("doc-switch commit flushes into the SESSION engine and unhides the layer there (B2)", () => {
    const { signals, dispose } = createMockEditorParams("text");
    const history = signals.workspace.getActiveHistory();
    // The SOURCE engine (doc A) the session belongs to. The doc-switch path
    // passes a workspace wrapper whose getActiveEngine returns THIS engine,
    // while the overlay's own workspace would read the newly active one.
    const sourceEngine = {
      getLayer: vi.fn((id: string) => textLayer(id, { content: "Hello" })),
      deleteLayer: vi.fn(),
      restore: vi.fn(),
      getLayerImageBitmap: vi.fn(() => null),
      snapshot: vi.fn(() => ({})),
      setRenderHiddenLayerId: vi.fn(),
    };
    // The overlay registers its pending-content flush; capture the engine it
    // receives (in the real overlay this pushes the typed content there).
    let flushEngine: unknown;
    setPendingTextFlush("text-1", (engine) => { flushEngine = engine; });

    signals.setTextEditSession({
      layerId: "text-1", docX: 0, docY: 0, boxMode: "point", boxWidth: 0,
      isNewLayer: true, preSnapshot: { layers: [] },
    });

    commitTextSession({
      ...signals,
      workspace: {
        ...signals.workspace,
        getActiveEngine: () => sourceEngine,
        getActiveHistory: () => history,
      },
    } as unknown as TextSessionEditor);

    // The flush targets the SESSION engine (never the active mockEngine), and
    // the hidden id is cleared there — the layer stays visible on return to
    // the source document (no invisible-text bug on doc switch).
    expect(flushEngine).toBe(sourceEngine);
    expect(sourceEngine.setRenderHiddenLayerId).toHaveBeenCalledWith(null);
    expect(history.commit).toHaveBeenCalledWith(expect.anything(), "Add Text");
    expect(signals.textEditSession()).toBeNull();
    dispose();
  });

  it("pending flush registry is keyed per layer (no cross-session clobber)", () => {
    // Best-practice fix #1: the flush must be scoped per session layerId, not a
    // single process-wide singleton — otherwise a second open session could
    // overwrite the first's pending-content flush.
    let a = 0;
    let b = 0;
    setPendingTextFlush("layer-a", () => { a++; });
    setPendingTextFlush("layer-b", () => { b++; });
    flushPendingText("layer-a", null);
    expect(a).toBe(1);
    expect(b).toBe(0);
    // Consumed: flushing again is a no-op (no double-commit).
    flushPendingText("layer-a", null);
    expect(a).toBe(1);
    // Clearing an unrelated layer leaves the other intact.
    setPendingTextFlush("layer-b", null);
    expect(b).toBe(0);
  });
});

describe("openTextEditSession (layer panel §7.3 re-edit path)", () => {
  it("opens a re-edit session on an existing text layer by id, switching to the text tool", () => {
    const { signals, mockEngine, dispose } = createMockEditorParams("move");
    const layer = textLayer("text-1", { boxMode: "area", boxWidth: 120 });
    (mockEngine as any).getLayer = vi.fn((id: string) => textLayer(id, { boxMode: "area", boxWidth: 120 }));
    (mockEngine as any).snapshot = vi.fn(() => ({ layers: [layer] }));
    // createMockEditorParams exposes the explicit `setSelectedLayerId: vi.fn()`
    // default as a bare accessor signal — swap in a spy (same pattern as the
    // double-click test above) so we can assert the selection call.
    const setSelectedLayerIdSpy = vi.fn();
    signals.setSelectedLayerId = setSelectedLayerIdSpy;

    const ok = openTextEditSession(signals as unknown as TextSessionOpener, "text-1");

    expect(ok).toBe(true);
    expect(signals.activeTool()).toBe("text");
    const session = signals.textEditSession();
    expect(session).not.toBeNull();
    expect(session!.layerId).toBe("text-1");
    expect(session!.isNewLayer).toBe(false);
    // Session anchors at the layer's doc position with its box geometry.
    expect(session!.boxMode).toBe("area");
    expect(session!.boxWidth).toBe(120);
    expect(setSelectedLayerIdSpy).toHaveBeenCalledWith("text-1");
    expect(signals.scheduler.requestRender).toHaveBeenCalled();
    dispose();
  });

  it("returns false for non-text layers so the panel keeps rename behavior", () => {
    const { signals, mockEngine, dispose } = createMockEditorParams("move");
    (mockEngine as any).getLayer = vi.fn(() => ({ id: "raster-1", type: "raster" }));

    const ok = openTextEditSession(signals as unknown as TextSessionOpener, "raster-1");

    expect(ok).toBe(false);
    expect(signals.textEditSession()).toBeNull();
    dispose();
  });

  it("commits any pending session first (click-away pattern), then opens the new one", () => {
    const { signals, mockEngine, dispose } = createMockEditorParams("text");
    const history = signals.workspace.getActiveHistory();
    // A pending session on another layer is open.
    signals.setTextEditSession({
      layerId: "pending-1", docX: 0, docY: 0, boxMode: "point", boxWidth: 0,
      isNewLayer: false, preSnapshot: {},
    });
    (mockEngine as any).getLayer = vi.fn((id: string) => {
      if (id === "pending-1") return textLayer("pending-1", { content: "Old" });
      return textLayer(id);
    });
    // Snapshot for the pre-session commit carries DIFFERENT content → real commit.
    (mockEngine as any).snapshot = vi.fn(() => ({ layers: [textLayer("pending-1", { content: "Old" })] }));

    const ok = openTextEditSession(signals as unknown as TextSessionOpener, "text-2");

    expect(ok).toBe(true);
    expect(history.commit).toHaveBeenCalled(); // pending session persisted as one step
    expect(signals.textEditSession()!.layerId).toBe("text-2");
    dispose();
  });
});

describe("syncTextSessionBase (undo/redo re-anchoring)", () => {
  function editorFor(signals: Record<string, any>) {
    return {
      workspace: signals.workspace,
      textEditSession: signals.textEditSession,
      setTextEditSession: signals.setTextEditSession,
      scheduler: signals.scheduler,
    } as unknown as TextSessionEditor;
  }

  it("re-anchors a still-open session's preSnapshot to the restored engine state", () => {
    const { signals, mockEngine, dispose } = createMockEditorParams("text");
    const layer = textLayer("text-1", { content: "Before" });
    (mockEngine as any).getLayer = vi.fn((id: string) => (id === "text-1" ? layer : undefined));
    (mockEngine as any).snapshot = vi.fn(() => ({ layers: [{ ...layer, textData: { ...layer.textData, content: "Older" } }] }));
    signals.setTextEditSession({
      layerId: "text-1", docX: 0, docY: 0, boxMode: "point", boxWidth: 0,
      isNewLayer: false, preSnapshot: { layers: [] },
    });

    syncTextSessionBase(editorFor(signals));

    // The next commit diffs against what the user NOW sees, not the stale
    // session-open state — otherwise the undo step would be jumped back over.
    expect((signals.textEditSession()!.preSnapshot as any).layers[0].textData.content).toBe("Older");
    dispose();
  });

  it("leaves the session untouched when undo deleted its layer", () => {
    const { signals, mockEngine, dispose } = createMockEditorParams("text");
    (mockEngine as any).getLayer = vi.fn(() => undefined);
    const old = { layers: [] };
    signals.setTextEditSession({
      layerId: "text-1", docX: 0, docY: 0, boxMode: "point", boxWidth: 0,
      isNewLayer: false, preSnapshot: old,
    });

    syncTextSessionBase(editorFor(signals));

    expect(signals.textEditSession()!.preSnapshot).toBe(old);
    dispose();
  });

  it("is a no-op when no session is open", () => {
    const { signals, mockEngine, dispose } = createMockEditorParams("text");
    (mockEngine as any).getLayer = vi.fn(() => undefined);

    syncTextSessionBase(editorFor(signals));

    expect(signals.textEditSession()).toBeNull();
    dispose();
  });
});
