import { describe, it, expect, vi } from "vitest";
import { render } from "solid-js/web";
import { EditorProvider, useEditor } from "../shell/EditorContext";
import { SelectionTransformOverlay } from "../SelectionTransformOverlay";
import { WorkspaceManager } from "@/engine/workspace";
import { WebGL2Backend } from "@/renderer/webgl2";
import { RenderScheduler } from "@/renderer/scheduler";
import type { LayerNode } from "@/engine/types";

function h(tag: any, props: Record<string, unknown> | null, ...children: any[]): any {
  return () => {
    const el = tag({ ...(props || {}), children: children.length === 1 ? children[0] : children.length > 1 ? children : undefined });
    return el;
  };
}

// The transform box is intentionally suppressed for the Background layer
// (useSelectionTransformDrag.getLayer guard). These overlay-mechanics tests
// open a blank doc whose only layer is the Background, so convert it to a
// normal, transformable layer to exercise the box — geometry/coordinate
// assertions already assume a doc-sized layer at the origin, which the
// Background is, so they remain valid.
// Note: the hook reads layers from the EditorContext `layers()` signal, which
// is a *copy* of the engine layers (workspaceSync.ts). Mutating the engine
// object alone is invisible to the signal, so we trigger a re-sync by calling
// transformLayer (which fires engine.onChange -> workspace notifyChange ->
// syncState re-copies the layers).
function activateBackgroundAsNormal(session: {
  engine: {
    getLayers: () => readonly LayerNode[];
    transformLayer: (id: string, t: Partial<{ scaleX: number; scaleY: number }>) => void;
  };
}) {
  const bg = session.engine.getLayers()[0];
  if (bg) {
    bg.isBackground = false;
    bg.lockPosition = false;
    bg.lockRotation = false;
    session.engine.transformLayer(bg.id, {});
  }
}

describe("Resize pointer-capture fix", () => {
  it("renders the move transform box in the same screen-space viewport as the layer", async () => {
    const ws = new WorkspaceManager();
    const session = WorkspaceManager.createBlankDocument("alignment", "Alignment", 472, 709);
    session.engine.setViewport({ panX: 100, panY: 50, zoom: 0.6 });

    const mockRenderer = {} as unknown as WebGL2Backend;
    const mockScheduler = { requestRender: vi.fn() } as unknown as RenderScheduler;

    const container = document.createElement("div");
    document.body.appendChild(container);

    const dispose = render(
      h(
        EditorProvider,
        { workspace: ws, renderer: mockRenderer, scheduler: mockScheduler },
        h(SelectionTransformOverlay, null),
      ),
      container,
    );

    ws.addDocument(session);
    activateBackgroundAsNormal(session);
    await Promise.resolve();

    const moveRect = container.querySelector("rect[data-move]") as SVGElement;
    expect(moveRect).not.toBeNull();
    expect(Number(moveRect.getAttribute("x"))).toBeCloseTo(100);
    expect(Number(moveRect.getAttribute("y"))).toBeCloseTo(50);
    expect(Number(moveRect.getAttribute("width"))).toBeCloseTo(283.2);
    expect(Number(moveRect.getAttribute("height"))).toBeCloseTo(425.4);

    dispose();
    container.parentNode?.removeChild(container);
  });

  it("keeps the root overlay cursor default so pasteboard does not inherit rotate cursor", async () => {
    let editor: any = null;
    const TestComponent = () => {
      editor = useEditor();
      return h(SelectionTransformOverlay, null)();
    };

    const ws = new WorkspaceManager();
    const session = WorkspaceManager.createBlankDocument("cursor-root", "Cursor Root", 800, 600);
    const mockRenderer = {} as unknown as WebGL2Backend;
    const mockScheduler = { requestRender: vi.fn() } as unknown as RenderScheduler;

    const container = document.createElement("div");
    document.body.appendChild(container);

    const dispose = render(
      h(
        EditorProvider,
        { workspace: ws, renderer: mockRenderer, scheduler: mockScheduler },
        h(TestComponent, null),
      ),
      container,
    );

    ws.addDocument(session);
    activateBackgroundAsNormal(session);
    await Promise.resolve();

    editor.setHoverHandle("rotate-nw");
    editor.setHoverPos({ x: 120, y: 120 });
    await Promise.resolve();

    const svg = container.querySelector("svg[data-overlay-svg]") as SVGSVGElement;
    expect(svg).not.toBeNull();
    expect(svg.style.cursor).toBe("default");

    dispose();
    container.parentNode?.removeChild(container);
  });

  it("keeps the active resize cursor on the root SVG during pointer-captured resize drag", async () => {
    const ws = new WorkspaceManager();
    const session = WorkspaceManager.createBlankDocument("cursor-drag", "Cursor Drag", 800, 600);
    const mockRenderer = {} as unknown as WebGL2Backend;
    const mockScheduler = { requestRender: vi.fn() } as unknown as RenderScheduler;
    const origSet = SVGElement.prototype.setPointerCapture;
    const setSpy = vi.fn();
    SVGElement.prototype.setPointerCapture = setSpy;
    const origRelease = SVGElement.prototype.releasePointerCapture;
    const releaseSpy = vi.fn();
    SVGElement.prototype.releasePointerCapture = releaseSpy;
    const container = document.createElement("div");
    document.body.appendChild(container);

    const dispose = render(
      h(
        EditorProvider,
        { workspace: ws, renderer: mockRenderer, scheduler: mockScheduler },
        h(SelectionTransformOverlay, null),
      ),
      container,
    );
    ws.addDocument(session);
    activateBackgroundAsNormal(session);
    await Promise.resolve();

    const svg = container.querySelector("svg[data-overlay-svg]") as SVGSVGElement;
    const nwHandle = container.querySelector("[data-handle='nw']") as SVGElement;
    expect(svg).not.toBeNull();
    expect(nwHandle).not.toBeNull();
    expect(svg.style.cursor).toBe("default");

    nwHandle.dispatchEvent(new PointerEvent("pointerdown", {
      pointerId: 77,
      bubbles: true,
      cancelable: true,
      clientX: 0,
      clientY: 0,
    }));

    expect(setSpy).toHaveBeenCalledWith(77);
    expect(svg.style.cursor).toBe("nwse-resize");

    svg.dispatchEvent(new PointerEvent("pointerup", {
      pointerId: 77,
      bubbles: true,
    }));

    expect(svg.style.cursor).toBe("default");
    dispose();
    SVGElement.prototype.setPointerCapture = origSet;
    SVGElement.prototype.releasePointerCapture = origRelease;
    container.parentNode?.removeChild(container);
  });

  it("move-zone is a click-through passthrough (canvas hook handles layer drag)", async () => {
    const ws = new WorkspaceManager();
    const session = WorkspaceManager.createBlankDocument("passthrough", "Passthrough", 800, 600);
    const mockRenderer = {} as unknown as WebGL2Backend;
    const mockScheduler = { requestRender: vi.fn() } as unknown as RenderScheduler;
    const origSet = SVGElement.prototype.setPointerCapture;
    const setSpy = vi.fn();
    SVGElement.prototype.setPointerCapture = setSpy;
    const origRelease = SVGElement.prototype.releasePointerCapture;
    const releaseSpy = vi.fn();
    SVGElement.prototype.releasePointerCapture = releaseSpy;
    const container = document.createElement("div");
    document.body.appendChild(container);

    const dispose = render(
      h(
        EditorProvider,
        { workspace: ws, renderer: mockRenderer, scheduler: mockScheduler },
        h(SelectionTransformOverlay, null),
      ),
      container,
    );
    ws.addDocument(session);
    activateBackgroundAsNormal(session);
    await Promise.resolve();

    const svg = container.querySelector("svg[data-overlay-svg]") as SVGSVGElement;
    const moveRect = container.querySelector("rect[data-move]") as SVGElement;
    const eHandle = container.querySelector("[data-handle='e']") as SVGElement;
    expect(svg).not.toBeNull();
    expect(moveRect).not.toBeNull();
    expect(eHandle).not.toBeNull();

    // Move zone must NOT capture pointer events — clicks pass through to canvas
    expect(moveRect.style.pointerEvents).toBe("none");

    moveRect.dispatchEvent(new PointerEvent("pointerdown", {
      pointerId: 88,
      bubbles: true,
      cancelable: true,
      clientX: 0,
      clientY: 0,
    }));

    // No setPointerCapture on move-zone (intentional — canvas handles drag)
    expect(setSpy).not.toHaveBeenCalled();

    // Resize handle still works
    eHandle.dispatchEvent(new PointerEvent("pointerdown", {
      pointerId: 88,
      bubbles: true,
      cancelable: true,
      clientX: 0,
      clientY: 0,
    }));

    expect(setSpy).toHaveBeenCalledWith(88);

    svg.dispatchEvent(new PointerEvent("pointerup", {
      pointerId: 88,
      bubbles: true,
    }));

    dispose();
    SVGElement.prototype.setPointerCapture = origSet;
    SVGElement.prototype.releasePointerCapture = origRelease;
    container.parentNode?.removeChild(container);
  });

  it("captures pointer on root SVG, not on per-handle element", () => {
    const ws = new WorkspaceManager();
    const session = WorkspaceManager.createBlankDocument("test", "Test", 800, 600);

    const mockRenderer = {} as unknown as WebGL2Backend;
    const mockScheduler = { requestRender: vi.fn() } as unknown as RenderScheduler;

    const origSet = SVGElement.prototype.setPointerCapture;
    const setSpy = vi.fn();
    SVGElement.prototype.setPointerCapture = setSpy;

    const origRelease = SVGElement.prototype.releasePointerCapture;
    const releaseSpy = vi.fn();
    SVGElement.prototype.releasePointerCapture = releaseSpy;

    const container = document.createElement("div");
    document.body.appendChild(container);

    const dispose = render(
      h(
        EditorProvider,
        { workspace: ws, renderer: mockRenderer, scheduler: mockScheduler },
        h(SelectionTransformOverlay, null),
      ),
      container,
    );

    // Add document after EditorProvider mounts so onChange fires syncState
    ws.addDocument(session);
    activateBackgroundAsNormal(session);

    const svg = container.querySelector("svg[data-overlay-svg]");
    expect(svg).not.toBeNull();
    const handle = container.querySelector("[data-handle]");
    expect(handle).not.toBeNull();

    setSpy.mockClear();
    releaseSpy.mockClear();

    handle!.dispatchEvent(
      new PointerEvent("pointerdown", {
        pointerId: 42,
        bubbles: true,
        cancelable: true,
        clientX: 100,
        clientY: 100,
      }),
    );

    expect(setSpy).toHaveBeenCalledTimes(1);
    // The `this` context of the setPointerCapture call should be the root SVG
    expect(setSpy.mock.instances[0]).toBe(svg);
    expect(setSpy).toHaveBeenCalledWith(42);

    // Restore
    SVGElement.prototype.setPointerCapture = origSet;
    SVGElement.prototype.releasePointerCapture = origRelease;
    dispose();
    container.parentNode?.removeChild(container);
  });

  it("releases pointer capture on root SVG via pointerup", () => {
    const ws = new WorkspaceManager();
    const session = WorkspaceManager.createBlankDocument("test2", "Test", 800, 600);

    const mockRenderer = {} as unknown as WebGL2Backend;
    const mockScheduler = { requestRender: vi.fn() } as unknown as RenderScheduler;

    const origSet = SVGElement.prototype.setPointerCapture;
    const setSpy = vi.fn();
    SVGElement.prototype.setPointerCapture = setSpy;

    const origRelease = SVGElement.prototype.releasePointerCapture;
    const releaseSpy = vi.fn();
    SVGElement.prototype.releasePointerCapture = releaseSpy;

    const container = document.createElement("div");
    document.body.appendChild(container);

    const dispose = render(
      h(
        EditorProvider,
        { workspace: ws, renderer: mockRenderer, scheduler: mockScheduler },
        h(SelectionTransformOverlay, null),
      ),
      container,
    );
    ws.addDocument(session);
    activateBackgroundAsNormal(session);

    const svg = container.querySelector("svg[data-overlay-svg]") as SVGElement;
    const handle = container.querySelector("[data-handle]") as SVGElement;

    setSpy.mockClear();
    releaseSpy.mockClear();

    handle.dispatchEvent(
      new PointerEvent("pointerdown", {
        pointerId: 7,
        bubbles: true,
        cancelable: true,
        clientX: 100,
        clientY: 100,
      }),
    );
    expect(setSpy).toHaveBeenCalledWith(7);

    svg.dispatchEvent(
      new PointerEvent("pointerup", {
        pointerId: 7,
        bubbles: true,
      }),
    );

    expect(releaseSpy).toHaveBeenCalledTimes(1);
    expect(releaseSpy.mock.instances[0]).toBe(svg);
    expect(releaseSpy).toHaveBeenCalledWith(7);

    SVGElement.prototype.setPointerCapture = origSet;
    SVGElement.prototype.releasePointerCapture = origRelease;
    dispose();
    container.parentNode?.removeChild(container);
  });

  it("ignores pointermove from non-captured pointer", () => {
    const ws = new WorkspaceManager();
    const session = WorkspaceManager.createBlankDocument("test3", "Test", 800, 600);

    const requestRender = vi.fn();
    const mockRenderer = {} as unknown as WebGL2Backend;
    const mockScheduler = { requestRender } as unknown as RenderScheduler;

    const origSet = SVGElement.prototype.setPointerCapture;
    const origRelease = SVGElement.prototype.releasePointerCapture;
    const setSpy = vi.fn();
    const releaseSpy = vi.fn();
    SVGElement.prototype.setPointerCapture = setSpy;
    SVGElement.prototype.releasePointerCapture = releaseSpy;

    const container = document.createElement("div");
    document.body.appendChild(container);

    const dispose = render(
      h(
        EditorProvider,
        { workspace: ws, renderer: mockRenderer, scheduler: mockScheduler },
        h(SelectionTransformOverlay, null),
      ),
      container,
    );
    ws.addDocument(session);
    activateBackgroundAsNormal(session);

    const svg = container.querySelector("svg[data-overlay-svg]") as SVGElement;
    const handle = container.querySelector("[data-handle]") as SVGElement;

    setSpy.mockClear();
    releaseSpy.mockClear();
    requestRender.mockClear();

    // Capture with pointer 1
    handle.dispatchEvent(
      new PointerEvent("pointerdown", {
        pointerId: 1,
        bubbles: true,
        cancelable: true,
        clientX: 100,
        clientY: 100,
      }),
    );

    requestRender.mockClear();

    // Move with a DIFFERENT pointer id — should be ignored
    svg.dispatchEvent(
      new PointerEvent("pointermove", {
        pointerId: 2,
        bubbles: true,
        clientX: 120,
        clientY: 120,
      }),
    );
    expect(requestRender).not.toHaveBeenCalled();

    // Move WITH captured pointer id — should be processed
    svg.dispatchEvent(
      new PointerEvent("pointermove", {
        pointerId: 1,
        bubbles: true,
        clientX: 120,
        clientY: 120,
      }),
    );
    expect(requestRender).toHaveBeenCalledTimes(1);

    SVGElement.prototype.setPointerCapture = origSet;
    SVGElement.prototype.releasePointerCapture = origRelease;
    dispose();
    container.parentNode?.removeChild(container);
  });
});

describe("Snap line cleanup", () => {
  function setup(customProps: Record<string, unknown> = {}) {
    const ws = new WorkspaceManager();
    const session = WorkspaceManager.createBlankDocument("snap-test", "Test", 800, 600);

    const mockRenderer = {} as unknown as WebGL2Backend;
    const mockScheduler = { requestRender: vi.fn() } as unknown as RenderScheduler;

    const origSet = SVGElement.prototype.setPointerCapture;
    const origRelease = SVGElement.prototype.releasePointerCapture;

    const setSpy = vi.fn();
    const releaseSpy = vi.fn();
    SVGElement.prototype.setPointerCapture = setSpy;
    SVGElement.prototype.releasePointerCapture = releaseSpy;

    const container = document.createElement("div");
    document.body.appendChild(container);

    const onSnapClear = vi.fn();
    const onHudUpdate = vi.fn();
    const onComputeSnap = vi.fn(() => ({ dx: 0, dy: 0, lines: [] }));

    const dispose = render(
      h(
        EditorProvider,
        { workspace: ws, renderer: mockRenderer, scheduler: mockScheduler },
        h(SelectionTransformOverlay, { onSnapClear, onHudUpdate, onComputeSnap, ...customProps }),
      ),
      container,
    );
    ws.addDocument(session);
    activateBackgroundAsNormal(session);

    const svg = container.querySelector("svg[data-overlay-svg]") as SVGElement;
    const handle = container.querySelector("[data-handle]") as SVGElement;
    const moveRect = container.querySelector("rect[data-move]") as SVGElement;

    onSnapClear.mockClear();
    onHudUpdate.mockClear();
    onComputeSnap.mockClear();

    return { ws, svg, handle, moveRect, onSnapClear, onHudUpdate, dispose, restore: () => {
      SVGElement.prototype.setPointerCapture = origSet;
      SVGElement.prototype.releasePointerCapture = origRelease;
      dispose();
      container.parentNode?.removeChild(container);
    }};
  }

  function startMoveDrag(svg: SVGElement, handle: SVGElement) {
    handle.dispatchEvent(
      new PointerEvent("pointerdown", {
        pointerId: 10,
        bubbles: true,
        cancelable: true,
        clientX: 100,
        clientY: 100,
      }),
    );
    svg.dispatchEvent(
      new PointerEvent("pointermove", {
        pointerId: 10,
        bubbles: true,
        clientX: 120,
        clientY: 120,
      }),
    );
  }

  it("clears snap lines on pointerup after move drag", () => {
    const { svg, handle, onSnapClear, restore } = setup();
    startMoveDrag(svg, handle);

    svg.dispatchEvent(new PointerEvent("pointerup", { pointerId: 10, bubbles: true }));

    expect(onSnapClear).toHaveBeenCalledTimes(1);
    restore();
  });

  it("clears snap lines on pointercancel after move drag", () => {
    const { svg, handle, onSnapClear, restore } = setup();
    startMoveDrag(svg, handle);

    svg.dispatchEvent(new PointerEvent("pointercancel", { pointerId: 10, bubbles: true }));

    expect(onSnapClear).toHaveBeenCalledTimes(1);
    restore();
  });

  it("clears snap lines on lostpointercapture after move drag", () => {
    const { svg, handle, onSnapClear, restore } = setup();
    startMoveDrag(svg, handle);

    svg.dispatchEvent(new PointerEvent("lostpointercapture", { pointerId: 10, bubbles: true }));

    expect(onSnapClear).toHaveBeenCalledTimes(1);
    restore();
  });

  it("clears dragState on lostpointercapture even with mismatched pointerId", () => {
    const { svg, handle, onSnapClear, restore } = setup();
    startMoveDrag(svg, handle);

    // Browser sends lostpointercapture with a DIFFERENT pointerId than the drag
    svg.dispatchEvent(new PointerEvent("lostpointercapture", { pointerId: 999, bubbles: true }));

    // Snap lines should still be cleared despite mismatched pointerId
    expect(onSnapClear).toHaveBeenCalledTimes(1);

    // Subsequent pointermove with original pointerId should NOT trigger render
    svg.dispatchEvent(new PointerEvent("pointermove", {
      pointerId: 10, bubbles: true, clientX: 140, clientY: 140,
    }));

    // dragState was cleared, so pointermove with any id is a no-op
    restore();
  });

  it("clears snap lines on Escape keydown during drag", () => {
    const { svg, handle, onSnapClear, restore } = setup();
    startMoveDrag(svg, handle);

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));

    expect(onSnapClear).toHaveBeenCalledTimes(1);
    restore();
  });

  it("creates a transform session on resize pointerdown but not on move pointerdown", () => {
    let capturedSession: any = null;
    const TestComponent = () => {
      const { layerTransformSession } = useEditor();
      capturedSession = layerTransformSession;
      return h(SelectionTransformOverlay, null)();
    };

    const ws = new WorkspaceManager();
    const sessionDoc = WorkspaceManager.createBlankDocument("session-test", "Test", 800, 600);
    const mockRenderer = {} as unknown as WebGL2Backend;
    const mockScheduler = { requestRender: vi.fn() } as unknown as RenderScheduler;

    const origSet = SVGElement.prototype.setPointerCapture;
    const setSpy = vi.fn();
    SVGElement.prototype.setPointerCapture = setSpy;

    const origRelease = SVGElement.prototype.releasePointerCapture;
    const releaseSpy = vi.fn();
    SVGElement.prototype.releasePointerCapture = releaseSpy;

    const container = document.createElement("div");
    document.body.appendChild(container);

    const dispose = render(
      h(
        EditorProvider,
        { workspace: ws, renderer: mockRenderer, scheduler: mockScheduler },
        h(TestComponent, null),
      ),
      container,
    );
    ws.addDocument(sessionDoc);
    activateBackgroundAsNormal(sessionDoc);

    const handle = container.querySelector("[data-handle]") as SVGElement;
    const moveRect = container.querySelector("rect[data-move]") as SVGElement;

    expect(capturedSession()).toBeNull();

    // Trigger down on move rect
    moveRect.dispatchEvent(
      new PointerEvent("pointerdown", {
        pointerId: 30,
        bubbles: true,
        cancelable: true,
        clientX: 100,
        clientY: 100,
      }),
    );
    expect(capturedSession()).toBeNull();

    // Trigger down on resize handle
    handle.dispatchEvent(
      new PointerEvent("pointerdown", {
        pointerId: 31,
        bubbles: true,
        cancelable: true,
        clientX: 100,
        clientY: 100,
      }),
    );
    expect(capturedSession()).toBeNull();

    dispose();
    container.parentNode?.removeChild(container);
  });

  it("Escape during active resize restores original snapshot and clears session", () => {
    let capturedSession: any = null;
    const TestComponent = () => {
      const { layerTransformSession } = useEditor();
      capturedSession = layerTransformSession;
      return h(SelectionTransformOverlay, null)();
    };

    const ws = new WorkspaceManager();
    const sessionDoc = WorkspaceManager.createBlankDocument("escape-test", "Test", 800, 600);
    const mockRenderer = {} as unknown as WebGL2Backend;
    const mockScheduler = { requestRender: vi.fn() } as unknown as RenderScheduler;

    const origSet = SVGElement.prototype.setPointerCapture;
    const setSpy = vi.fn();
    SVGElement.prototype.setPointerCapture = setSpy;

    const origRelease = SVGElement.prototype.releasePointerCapture;
    const releaseSpy = vi.fn();
    SVGElement.prototype.releasePointerCapture = releaseSpy;

    const container = document.createElement("div");
    document.body.appendChild(container);

    const dispose = render(
      h(
        EditorProvider,
        { workspace: ws, renderer: mockRenderer, scheduler: mockScheduler },
        h(TestComponent, null),
      ),
      container,
    );
    ws.addDocument(sessionDoc);
    activateBackgroundAsNormal(sessionDoc);

    const handle = container.querySelector("[data-handle]") as SVGElement;

    // Trigger down on resize handle
    handle.dispatchEvent(
      new PointerEvent("pointerdown", {
        pointerId: 40,
        bubbles: true,
        cancelable: true,
        clientX: 100,
        clientY: 100,
      }),
    );
    expect(capturedSession()).toBeNull();

    // Trigger escape keydown
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    expect(capturedSession()).toBeNull();

    SVGElement.prototype.setPointerCapture = origSet;
    SVGElement.prototype.releasePointerCapture = origRelease;
    dispose();
    container.parentNode?.removeChild(container);
  });
});

describe("Space+pan navigation mode", () => {
  function setupNavigationTest(isNavigationMode: boolean) {
    const ws = new WorkspaceManager();
    const session = WorkspaceManager.createBlankDocument("nav-test", "Test", 800, 600);

    const mockRenderer = {} as unknown as WebGL2Backend;
    const mockScheduler = { requestRender: vi.fn() } as unknown as RenderScheduler;

    const origSet = SVGElement.prototype.setPointerCapture;
    const setSpy = vi.fn();
    SVGElement.prototype.setPointerCapture = setSpy;

    const origRelease = SVGElement.prototype.releasePointerCapture;
    const releaseSpy = vi.fn();
    SVGElement.prototype.releasePointerCapture = releaseSpy;

    const container = document.createElement("div");
    document.body.appendChild(container);

    const dispose = render(
      h(
        EditorProvider,
        { workspace: ws, renderer: mockRenderer, scheduler: mockScheduler },
        h(SelectionTransformOverlay, { isNavigationMode }),
      ),
      container,
    );
    ws.addDocument(session);
    activateBackgroundAsNormal(session);

    const svg = container.querySelector("svg[data-overlay-svg]") as SVGElement;
    const moveRect = container.querySelector("rect[data-move]") as SVGElement;
    const handle = container.querySelector("[data-handle]") as SVGElement;

    setSpy.mockClear();
    releaseSpy.mockClear();

    return { svg, moveRect, handle, setSpy, dispose, restore: () => {
      SVGElement.prototype.setPointerCapture = origSet;
      SVGElement.prototype.releasePointerCapture = origRelease;
      dispose();
      container.parentNode?.removeChild(container);
    }};
  }

  it("does not capture pointer on move rect when isNavigationMode is true", () => {
    const { moveRect, setSpy, restore } = setupNavigationTest(true);

    moveRect.dispatchEvent(
      new PointerEvent("pointerdown", {
        pointerId: 50,
        bubbles: true,
        cancelable: true,
        clientX: 100,
        clientY: 100,
      }),
    );

    expect(setSpy).not.toHaveBeenCalled();
    restore();
  });

  it("does not capture pointer on resize handle when isNavigationMode is true", () => {
    const { handle, setSpy, restore } = setupNavigationTest(true);

    handle.dispatchEvent(
      new PointerEvent("pointerdown", {
        pointerId: 51,
        bubbles: true,
        cancelable: true,
        clientX: 100,
        clientY: 100,
      }),
    );

    expect(setSpy).not.toHaveBeenCalled();
    restore();
  });

  it("does NOT capture pointer on move rect (intentional passthrough to canvas)", () => {
    const { moveRect, setSpy, restore } = setupNavigationTest(false);

    moveRect.dispatchEvent(
      new PointerEvent("pointerdown", {
        pointerId: 52,
        bubbles: true,
        cancelable: true,
        clientX: 100,
        clientY: 100,
      }),
    );

    // Move rect is now a click-through passthrough — clicks go to canvas's useCanvasLayerDrag
    expect(setSpy).not.toHaveBeenCalled();
    restore();
  });

  it("move rect events bubble to parent element in navigation mode", () => {
    const { moveRect, restore } = setupNavigationTest(true);

    const parentListener = vi.fn();
    const parent = moveRect.parentElement!;
    parent.addEventListener("pointerdown", parentListener);

    moveRect.dispatchEvent(
      new PointerEvent("pointerdown", {
        pointerId: 53,
        bubbles: true,
        cancelable: true,
        clientX: 100,
        clientY: 100,
      }),
    );

    expect(parentListener).toHaveBeenCalledTimes(1);
    parent.removeEventListener("pointerdown", parentListener);
    restore();
  });

  it("handle events bubble to parent element in navigation mode", () => {
    const { handle, restore } = setupNavigationTest(true);

    const parentListener = vi.fn();
    const parent = handle.parentElement!;
    parent.addEventListener("pointerdown", parentListener);

    handle.dispatchEvent(
      new PointerEvent("pointerdown", {
        pointerId: 54,
        bubbles: true,
        cancelable: true,
        clientX: 100,
        clientY: 100,
      }),
    );

    expect(parentListener).toHaveBeenCalledTimes(1);
    parent.removeEventListener("pointerdown", parentListener);
    restore();
  });
});

describe("Donut rotate ring path", () => {
  it("renders donut rotate ring path with fill-rule='evenodd' and pointer-events style", async () => {
    const ws = new WorkspaceManager();
    const session = WorkspaceManager.createBlankDocument("donut-render", "Donut Render", 800, 600);
    const mockRenderer = {} as unknown as WebGL2Backend;
    const mockScheduler = { requestRender: vi.fn() } as unknown as RenderScheduler;
    const container = document.createElement("div");
    document.body.appendChild(container);

    const dispose = render(
      h(
        EditorProvider,
        { workspace: ws, renderer: mockRenderer, scheduler: mockScheduler },
        h(SelectionTransformOverlay, null),
      ),
      container,
    );
    ws.addDocument(session);
    activateBackgroundAsNormal(session);
    await Promise.resolve();

    const donutPath = container.querySelector('path[fill-rule="evenodd"]') as SVGPathElement;
    expect(donutPath).not.toBeNull();
    expect(donutPath.getAttribute("fill-rule")).toBe("evenodd");
    expect(donutPath.getAttribute("fill")).toBe("transparent");
    // "all" so the whole rotate ring band is interactive (rotate works when
    // clicked anywhere in the band); pasteboard deselect relies on clicks
    // landing outside the ring band, which the E2E tests cover.
    expect(donutPath.style.pointerEvents).toBe("all");

    dispose();
    container.parentNode?.removeChild(container);
  });

  it("calls setPointerCapture on pointerdown on donut ring path", async () => {
    const ws = new WorkspaceManager();
    const session = WorkspaceManager.createBlankDocument("donut-rotate", "Donut Rotate", 800, 600);
    const mockRenderer = {} as unknown as WebGL2Backend;
    const mockScheduler = { requestRender: vi.fn() } as unknown as RenderScheduler;

    const origSet = SVGElement.prototype.setPointerCapture;
    const setSpy = vi.fn();
    SVGElement.prototype.setPointerCapture = setSpy;
    const origRelease = SVGElement.prototype.releasePointerCapture;
    const releaseSpy = vi.fn();
    SVGElement.prototype.releasePointerCapture = releaseSpy;

    const container = document.createElement("div");
    document.body.appendChild(container);

    const dispose = render(
      h(
        EditorProvider,
        { workspace: ws, renderer: mockRenderer, scheduler: mockScheduler },
        h(SelectionTransformOverlay, null),
      ),
      container,
    );
    ws.addDocument(session);
    activateBackgroundAsNormal(session);
    await Promise.resolve();

    const donutPath = container.querySelector('path[fill-rule="evenodd"]') as SVGPathElement;
    expect(donutPath).not.toBeNull();

    setSpy.mockClear();

    donutPath.dispatchEvent(new PointerEvent("pointerdown", {
      pointerId: 55,
      bubbles: true,
      cancelable: true,
      clientX: 100,
      clientY: 100,
    }));

    expect(setSpy).toHaveBeenCalledWith(55);

    SVGElement.prototype.setPointerCapture = origSet;
    SVGElement.prototype.releasePointerCapture = origRelease;
    dispose();
    container.parentNode?.removeChild(container);
  });

  it("maintains aspect ratio lock when resizing after a snapped rotation (holding Shift)", async () => {
    const ws = new WorkspaceManager();
    const session = WorkspaceManager.createBlankDocument("ratio-snap-test", "Ratio Snap Test", 800, 600);
    const layer = session.engine.addLayer("ratio-test-layer", 100, 100);
    session.engine.setActiveLayer("ratio-test-layer");

    const mockRenderer = {} as unknown as WebGL2Backend;
    const mockScheduler = { requestRender: vi.fn() } as unknown as RenderScheduler;

    const container = document.createElement("div");
    document.body.appendChild(container);

    let editorRef: any = null;
    const CaptureEditor = () => {
      editorRef = useEditor();
      return h(SelectionTransformOverlay, null)();
    };

    const dispose = render(
      h(
        EditorProvider,
        { workspace: ws, renderer: mockRenderer, scheduler: mockScheduler },
        h(CaptureEditor, null),
      ),
      container,
    );

    ws.addDocument(session);
    activateBackgroundAsNormal(session);
    await Promise.resolve();

    const donutPath = container.querySelector('path[fill-rule="evenodd"]') as SVGPathElement;
    expect(donutPath).not.toBeNull();

    donutPath.dispatchEvent(new PointerEvent("pointerdown", {
      pointerId: 44,
      bubbles: true,
      cancelable: true,
      clientX: 200,
      clientY: 200,
      shiftKey: true,
    }));

    window.dispatchEvent(new PointerEvent("pointermove", {
      pointerId: 44,
      clientX: 210,
      clientY: 210,
      shiftKey: true,
    }));

    window.dispatchEvent(new PointerEvent("pointerup", {
      pointerId: 44,
    }));

    const transformSession = editorRef.layerTransformSession();
    expect(transformSession).toBeNull();

    const seHandle = container.querySelector("[data-handle='se']") as SVGElement;
    expect(seHandle).not.toBeNull();

    seHandle.dispatchEvent(new PointerEvent("pointerdown", {
      pointerId: 55,
      bubbles: true,
      cancelable: true,
      clientX: 150,
      clientY: 150,
      shiftKey: false,
    }));

    window.dispatchEvent(new PointerEvent("pointermove", {
      pointerId: 55,
      clientX: 200,
      clientY: 160,
      shiftKey: false,
    }));

    window.dispatchEvent(new PointerEvent("pointerup", {
      pointerId: 55,
    }));

    const updatedLayer = session.engine.getLayer(layer.id)!;
    expect(updatedLayer.transform.scaleX).toBeCloseTo(updatedLayer.transform.scaleY, 4);

    dispose();
    container.parentNode?.removeChild(container);
  });

  it("commits transform session when double-clicking inside the active layer's bounds", async () => {
    const ws = new WorkspaceManager();
    const session = WorkspaceManager.createBlankDocument("dblclick-test", "DblClick Test", 800, 600);
    const layer = session.engine.addLayer("dblclick-layer", 100, 100);
    session.engine.setActiveLayer("dblclick-layer");

    const mockRenderer = {} as unknown as WebGL2Backend;
    const mockScheduler = { requestRender: vi.fn() } as unknown as RenderScheduler;

    const container = document.createElement("div");
    document.body.appendChild(container);

    let editorRef: any = null;
    const CaptureEditor = () => {
      editorRef = useEditor();
      return h(SelectionTransformOverlay, null)();
    };

    const dispose = render(
      h(
        EditorProvider,
        { workspace: ws, renderer: mockRenderer, scheduler: mockScheduler },
        h(CaptureEditor, null),
      ),
      container,
    );

    ws.addDocument(session);
    activateBackgroundAsNormal(session);
    await Promise.resolve();

    const currentSession = {
      documentId: session.engine.getId(),
      layerId: layer.id,
      initialTransform: { ...layer.transform },
      currentTransform: { ...layer.transform },
      originalTransform: { ...layer.transform },
      originalSnapshot: session.engine.snapshot(),
      mode: "move" as const,
      startScreenPos: { x: 0, y: 0 },
      startLayerPos: { x: 0, y: 0 },
      aspectRatio: 1,
      lockRatio: false,
    };
    editorRef.setLayerTransformSession(currentSession);
    await Promise.resolve();

    expect(editorRef.layerTransformSession()).not.toBeNull();

    const svgElement = container.querySelector("[data-overlay-svg]") as SVGElement;
    expect(svgElement).not.toBeNull();

    svgElement.dispatchEvent(new MouseEvent("dblclick", {
      bubbles: true,
      cancelable: true,
      clientX: 50,
      clientY: 50,
    }));
    await Promise.resolve();

    expect(editorRef.layerTransformSession()).toBeNull();

    dispose();
    container.parentNode?.removeChild(container);
  });
});
