import { describe, it, expect, vi } from "vitest";
import * as EditorContextModule from "../shell/EditorContext";
import { mockUseEditor } from "@/__tests__/mockUseEditor";
import { render as solidRender } from "solid-js/web";
import { createSignal, type JSX } from "solid-js";
import { CropOverlay } from "../CropOverlay";
import { ModernCropOverlay } from "../ModernCropOverlay";
import { ViewportCamera } from "../../../viewport/viewportCamera";
import { WorkspaceManager } from "@/engine/workspace";
import type { WebGL2Backend } from "@/renderer/webgl2";
import type { RenderScheduler } from "@/renderer/scheduler";

function render(ui: () => JSX.Element, container: HTMLElement) {
  const workspace = new WorkspaceManager();
  const renderer = {} as WebGL2Backend;
  const scheduler = { requestRender: vi.fn() } as unknown as RenderScheduler;
  return solidRender(
    () => (
      <EditorContextModule.EditorProvider
        workspace={workspace}
        renderer={renderer}
        scheduler={scheduler}
        camera={new ViewportCamera()}
      >
        {ui()}
      </EditorContextModule.EditorProvider>
    ),
    container,
  );
}

describe("CropOverlay pointer capture", () => {
  it("captures pointer on its own svg root", () => {
    const origSet = SVGElement.prototype.setPointerCapture;
    const setSpy = vi.fn();
    SVGElement.prototype.setPointerCapture = setSpy;

    const origRelease = SVGElement.prototype.releasePointerCapture;
    const releaseSpy = vi.fn();
    SVGElement.prototype.releasePointerCapture = releaseSpy;

    const onCropRectChange = vi.fn();
    const container = document.createElement("div");
    document.body.appendChild(container);

    const dispose = render(
      () => (
        <CropOverlay
          cropRect={{ x: 0, y: 0, w: 100, h: 100 }}
          guideMode="thirds"
          canvasWidth={800}
          canvasHeight={600}
          zoom={1}
          cropMode="free"
          cropAspect={null}
          onCropRectChange={onCropRectChange}
        />
      ),
      container,
    );

    const svgEl = container.querySelector("svg");
    expect(svgEl).not.toBeNull();

    setSpy.mockClear();

    const moveZone = container.querySelector("[data-crop-move]");
    expect(moveZone).not.toBeNull();
    moveZone!.dispatchEvent(
      new PointerEvent("pointerdown", {
        pointerId: 1,
        bubbles: true,
        cancelable: true,
        clientX: 50,
        clientY: 50,
      }),
    );

    expect(setSpy).toHaveBeenCalledTimes(1);
    expect(setSpy.mock.instances[0]).toBe(svgEl);

    SVGElement.prototype.setPointerCapture = origSet;
    SVGElement.prototype.releasePointerCapture = origRelease;
    dispose();
    container.parentNode?.removeChild(container);
  });

  it("fires onCropRectChange on move drag", () => {
    const origSet = SVGElement.prototype.setPointerCapture;
    SVGElement.prototype.setPointerCapture = vi.fn();
    const origRelease = SVGElement.prototype.releasePointerCapture;
    SVGElement.prototype.releasePointerCapture = vi.fn();

    const onCropRectChange = vi.fn();
    const container = document.createElement("div");
    document.body.appendChild(container);

    const dispose = render(
      () => (
        <CropOverlay
          cropRect={{ x: 10, y: 10, w: 100, h: 100 }}
          guideMode="thirds"
          canvasWidth={800}
          canvasHeight={600}
          zoom={1}
          cropMode="free"
          cropAspect={null}
          onCropRectChange={onCropRectChange}
        />
      ),
      container,
    );

    const svgEl = container.querySelector("svg");
    expect(svgEl).not.toBeNull();

    onCropRectChange.mockClear();

    const moveZone = container.querySelector("[data-crop-move]");
    expect(moveZone).not.toBeNull();

    moveZone!.dispatchEvent(
      new PointerEvent("pointerdown", {
        pointerId: 2,
        bubbles: true,
        cancelable: true,
        clientX: 50,
        clientY: 50,
      }),
    );

    svgEl!.dispatchEvent(
      new PointerEvent("pointermove", {
        pointerId: 2,
        bubbles: true,
        cancelable: true,
        clientX: 70,
        clientY: 70,
      }),
    );

    expect(onCropRectChange).toHaveBeenCalled();

    SVGElement.prototype.setPointerCapture = origSet;
    SVGElement.prototype.releasePointerCapture = origRelease;
    dispose();
    container.parentNode?.removeChild(container);
  });

  it("does not respond to non-captured pointer events", () => {
    const origSet = SVGElement.prototype.setPointerCapture;
    SVGElement.prototype.setPointerCapture = vi.fn();
    const origRelease = SVGElement.prototype.releasePointerCapture;
    SVGElement.prototype.releasePointerCapture = vi.fn();

    const onCropRectChange = vi.fn();
    const container = document.createElement("div");
    document.body.appendChild(container);

    const dispose = render(
      () => (
        <CropOverlay
          cropRect={{ x: 10, y: 10, w: 100, h: 100 }}
          guideMode="thirds"
          canvasWidth={800}
          canvasHeight={600}
          zoom={1}
          cropMode="free"
          cropAspect={null}
          onCropRectChange={onCropRectChange}
        />
      ),
      container,
    );

    const svgEl = container.querySelector("svg");
    expect(svgEl).not.toBeNull();

    onCropRectChange.mockClear();

    const moveZone = container.querySelector("[data-crop-move]");
    moveZone!.dispatchEvent(
      new PointerEvent("pointerdown", {
        pointerId: 3,
        bubbles: true,
        cancelable: true,
        clientX: 50,
        clientY: 50,
      }),
    );

    svgEl!.dispatchEvent(
      new PointerEvent("pointermove", {
        pointerId: 99,
        bubbles: true,
        cancelable: true,
        clientX: 70,
        clientY: 70,
      }),
    );

    expect(onCropRectChange).not.toHaveBeenCalled();

    SVGElement.prototype.setPointerCapture = origSet;
    SVGElement.prototype.releasePointerCapture = origRelease;
    dispose();
    container.parentNode?.removeChild(container);
  });

  it("releases capture and clears dragState on pointerup", () => {
    const origSet = SVGElement.prototype.setPointerCapture;
    SVGElement.prototype.setPointerCapture = vi.fn();
    const origRelease = SVGElement.prototype.releasePointerCapture;
    const releaseSpy = vi.fn();
    SVGElement.prototype.releasePointerCapture = releaseSpy;

    const onCropRectChange = vi.fn();
    const container = document.createElement("div");
    document.body.appendChild(container);

    const dispose = render(
      () => (
        <CropOverlay
          cropRect={{ x: 10, y: 10, w: 100, h: 100 }}
          guideMode="thirds"
          canvasWidth={800}
          canvasHeight={600}
          zoom={1}
          cropMode="free"
          cropAspect={null}
          onCropRectChange={onCropRectChange}
        />
      ),
      container,
    );

    const svgEl = container.querySelector("svg");
    expect(svgEl).not.toBeNull();

    const moveZone = container.querySelector("[data-crop-move]");
    moveZone!.dispatchEvent(
      new PointerEvent("pointerdown", {
        pointerId: 4,
        bubbles: true,
        cancelable: true,
        clientX: 50,
        clientY: 50,
      }),
    );

    releaseSpy.mockClear();

    svgEl!.dispatchEvent(
      new PointerEvent("pointerup", {
        pointerId: 4,
        bubbles: true,
        cancelable: true,
        clientX: 60,
        clientY: 60,
      }),
    );

    expect(releaseSpy).toHaveBeenCalled();

    svgEl!.dispatchEvent(
      new PointerEvent("pointermove", {
        pointerId: 4,
        bubbles: true,
        cancelable: true,
        clientX: 70,
        clientY: 70,
      }),
    );

    expect(onCropRectChange).toHaveBeenCalledTimes(0);

    SVGElement.prototype.setPointerCapture = origSet;
    SVGElement.prototype.releasePointerCapture = origRelease;
    dispose();
    container.parentNode?.removeChild(container);
  });

  it("clears dragState on lostpointercapture", () => {
    const origSet = SVGElement.prototype.setPointerCapture;
    SVGElement.prototype.setPointerCapture = vi.fn();
    const origRelease = SVGElement.prototype.releasePointerCapture;
    SVGElement.prototype.releasePointerCapture = vi.fn();

    const onCropRectChange = vi.fn();
    const container = document.createElement("div");
    document.body.appendChild(container);

    const dispose = render(
      () => (
        <CropOverlay
          cropRect={{ x: 10, y: 10, w: 100, h: 100 }}
          guideMode="thirds"
          canvasWidth={800}
          canvasHeight={600}
          zoom={1}
          cropMode="free"
          cropAspect={null}
          onCropRectChange={onCropRectChange}
        />
      ),
      container,
    );

    const svgEl = container.querySelector("svg");
    expect(svgEl).not.toBeNull();

    const moveZone = container.querySelector("[data-crop-move]");
    moveZone!.dispatchEvent(
      new PointerEvent("pointerdown", {
        pointerId: 5,
        bubbles: true,
        cancelable: true,
        clientX: 50,
        clientY: 50,
      }),
    );

    onCropRectChange.mockClear();

    svgEl!.dispatchEvent(
      new PointerEvent("lostpointercapture", {
        pointerId: 5,
        bubbles: true,
        cancelable: true,
      }),
    );

    svgEl!.dispatchEvent(
      new PointerEvent("pointermove", {
        pointerId: 5,
        bubbles: true,
        cancelable: true,
        clientX: 70,
        clientY: 70,
      }),
    );

    expect(onCropRectChange).not.toHaveBeenCalled();

    SVGElement.prototype.setPointerCapture = origSet;
    SVGElement.prototype.releasePointerCapture = origRelease;
    dispose();
    container.parentNode?.removeChild(container);
  });

  it("clears dragState on lostpointercapture even with mismatched pointerId", () => {
    const origSet = SVGElement.prototype.setPointerCapture;
    SVGElement.prototype.setPointerCapture = vi.fn();
    const origRelease = SVGElement.prototype.releasePointerCapture;
    SVGElement.prototype.releasePointerCapture = vi.fn();

    const onCropRectChange = vi.fn();
    const container = document.createElement("div");
    document.body.appendChild(container);

    const dispose = render(
      () => (
        <CropOverlay
          cropRect={{ x: 10, y: 10, w: 100, h: 100 }}
          guideMode="thirds"
          canvasWidth={800}
          canvasHeight={600}
          zoom={1}
          cropMode="free"
          cropAspect={null}
          onCropRectChange={onCropRectChange}
        />
      ),
      container,
    );

    const svgEl = container.querySelector("svg")!;
    const moveZone = container.querySelector("[data-crop-move]")!;

    moveZone.dispatchEvent(
      new PointerEvent("pointerdown", {
        pointerId: 5, bubbles: true, cancelable: true, clientX: 50, clientY: 50,
      }),
    );

    onCropRectChange.mockClear();

    // Browser sends lostpointercapture with a DIFFERENT pointerId than the drag
    svgEl.dispatchEvent(
      new PointerEvent("lostpointercapture", {
        pointerId: 999, bubbles: true, cancelable: true,
      }),
    );

    // Subsequent moves with the ORIGINAL pointerId should NOT fire onCropRectChange
    // because dragState should have been cleared regardless of which pointer was lost
    svgEl.dispatchEvent(
      new PointerEvent("pointermove", {
        pointerId: 5, bubbles: true, cancelable: true, clientX: 70, clientY: 70,
      }),
    );

    expect(onCropRectChange).not.toHaveBeenCalled();

    SVGElement.prototype.setPointerCapture = origSet;
    SVGElement.prototype.releasePointerCapture = origRelease;
    dispose();
    container.parentNode?.removeChild(container);
  });
});

describe("CropOverlay handle hit detection", () => {
  it("detects handle hit on pointerdown and starts drag", () => {
    const origSet = SVGElement.prototype.setPointerCapture;
    SVGElement.prototype.setPointerCapture = vi.fn();
    const origRelease = SVGElement.prototype.releasePointerCapture;
    SVGElement.prototype.releasePointerCapture = vi.fn();

    const onCropRectChange = vi.fn();
    const container = document.createElement("div");
    document.body.appendChild(container);

    const dispose = render(
      () => (
        <div style={{ position: "relative", width: "800px", height: "600px" }}>
          <CropOverlay
            cropRect={{ x: 100, y: 100, w: 200, h: 200 }}
            guideMode="thirds"
            canvasWidth={800}
            canvasHeight={600}
            zoom={1}
            cropMode="free"
            cropAspect={null}
            onCropRectChange={onCropRectChange}
          />
        </div>
      ),
      container,
    );

    const svgEl = container.querySelector("svg");
    expect(svgEl).not.toBeNull();

    const rect = svgEl!.getBoundingClientRect();

    const seHandle = container.querySelector('[data-crop-handle="se"]');
    expect(seHandle).not.toBeNull();
    expect(seHandle!.getAttribute("width")).toBe("32");

    onCropRectChange.mockClear();

    seHandle!.dispatchEvent(
      new PointerEvent("pointerdown", {
        pointerId: 6,
        bubbles: true,
        cancelable: true,
        clientX: rect.left + 300,
        clientY: rect.top + 300,
      }),
    );

    svgEl!.dispatchEvent(
      new PointerEvent("pointermove", {
        pointerId: 6,
        bubbles: true,
        cancelable: true,
        clientX: rect.left + 320,
        clientY: rect.top + 320,
      }),
    );

    expect(onCropRectChange).toHaveBeenCalled();

    SVGElement.prototype.setPointerCapture = origSet;
    SVGElement.prototype.releasePointerCapture = origRelease;
    dispose();
    container.parentNode?.removeChild(container);
  });

  it("starts rotate from the outside rotate band without moving or resizing", () => {
    const origSet = SVGElement.prototype.setPointerCapture;
    SVGElement.prototype.setPointerCapture = vi.fn();
    const origRelease = SVGElement.prototype.releasePointerCapture;
    SVGElement.prototype.releasePointerCapture = vi.fn();

    const onCropRectChange = vi.fn();
    const onCropRotationChange = vi.fn();
    const container = document.createElement("div");
    document.body.appendChild(container);

    const dispose = render(
      () => (
        <div style={{ position: "relative", width: "800px", height: "600px" }}>
          <CropOverlay
            cropRect={{ x: 100, y: 100, w: 200, h: 200 }}
            guideMode="thirds"
            canvasWidth={800}
            canvasHeight={600}
            zoom={1}
            cropMode="free"
            cropAspect={null}
            onCropRectChange={onCropRectChange}
            onCropRotationChange={onCropRotationChange}
          />
        </div>
      ),
      container,
    );

    const svgEl = container.querySelector("svg")!;
    const rotateBand = container.querySelector("[data-crop-rotate-band]")!;
    expect(rotateBand).not.toBeNull();

    rotateBand.dispatchEvent(
      new PointerEvent("pointerdown", {
        pointerId: 31,
        bubbles: true,
        cancelable: true,
        clientX: 310,
        clientY: 200,
      }),
    );

    svgEl.dispatchEvent(
      new PointerEvent("pointermove", {
        pointerId: 31,
        bubbles: true,
        cancelable: true,
        clientX: 200,
        clientY: 310,
      }),
    );

    expect(onCropRotationChange).toHaveBeenCalled();
    expect(onCropRectChange).not.toHaveBeenCalled();

    SVGElement.prototype.setPointerCapture = origSet;
    SVGElement.prototype.releasePointerCapture = origRelease;
    dispose();
    container.parentNode?.removeChild(container);
  });
});

describe("CropOverlay hover handle wiring", () => {
  it("reports hover handle to parent callback", () => {
    const onHoverHandleChange = vi.fn();
    const container = document.createElement("div");
    document.body.appendChild(container);

    const dispose = render(
      () => (
        <CropOverlay
          cropRect={{ x: 0, y: 0, w: 100, h: 100 }}
          guideMode="thirds"
          canvasWidth={800}
          canvasHeight={600}
          zoom={1}
          cropMode="free"
          cropAspect={null}
          onCropRectChange={vi.fn()}
          onHoverHandleChange={onHoverHandleChange}
        />
      ),
      container,
    );

    const seHandle = container.querySelector('[data-crop-handle="se"]');
    expect(seHandle).not.toBeNull();

    seHandle!.dispatchEvent(new PointerEvent("pointerenter", { bubbles: true }));
    expect(onHoverHandleChange).toHaveBeenCalledWith("se");

    seHandle!.dispatchEvent(new PointerEvent("pointerleave", { bubbles: true }));
    expect(onHoverHandleChange).toHaveBeenCalledWith(null);

    dispose();
    container.parentNode?.removeChild(container);
  });
});

describe("CropOverlay reactivity", () => {
  it("updates rendered crop box while resizing", () => {
    const origSet = SVGElement.prototype.setPointerCapture;
    SVGElement.prototype.setPointerCapture = vi.fn();
    const origRelease = SVGElement.prototype.releasePointerCapture;
    SVGElement.prototype.releasePointerCapture = vi.fn();

    const container = document.createElement("div");
    document.body.appendChild(container);

    const dispose = render(() => {
      const [cropRect, setCropRect] = createSignal({ x: 10, y: 10, w: 100, h: 100 });
      return (
        <CropOverlay
          cropRect={cropRect()}
          guideMode="thirds"
          canvasWidth={800}
          canvasHeight={600}
          zoom={1}
          cropMode="free"
          cropAspect={null}
          onCropRectChange={setCropRect}
        />
      );
    }, container);

    const svgEl = container.querySelector("svg");
    expect(svgEl).not.toBeNull();
    const visualRect = container.querySelectorAll("rect")[3];
    expect(visualRect).toBeDefined();
    expect(visualRect.getAttribute("width")).toBe("100");

    const seHandle = container.querySelector('[data-crop-handle="se"]');
    expect(seHandle).not.toBeNull();
    const bounds = svgEl!.getBoundingClientRect();
    seHandle!.dispatchEvent(
      new PointerEvent("pointerdown", {
        pointerId: 7,
        bubbles: true,
        cancelable: true,
        clientX: bounds.left + 110,
        clientY: bounds.top + 110,
      }),
    );
    svgEl!.dispatchEvent(
      new PointerEvent("pointermove", {
        pointerId: 7,
        bubbles: true,
        cancelable: true,
        clientX: bounds.left + 130,
        clientY: bounds.top + 110,
      }),
    );

    expect(visualRect.getAttribute("width")).toBe("120");

    SVGElement.prototype.setPointerCapture = origSet;
    SVGElement.prototype.releasePointerCapture = origRelease;
    dispose();
    container.parentNode?.removeChild(container);
  });
});

describe("CropOverlay new crop box drawing", () => {
  it("preserves outside document bounds when drawing a replacement crop box", () => {
    const origSet = SVGElement.prototype.setPointerCapture;
    SVGElement.prototype.setPointerCapture = vi.fn();
    const origRelease = SVGElement.prototype.releasePointerCapture;
    SVGElement.prototype.releasePointerCapture = vi.fn();

    const container = document.createElement("div");
    document.body.appendChild(container);

    const dispose = render(() => {
      const [cropRect, setCropRect] = createSignal({ x: 100, y: 100, w: 200, h: 120 });
      return (
        <CropOverlay
          cropRect={cropRect()}
          guideMode="thirds"
          canvasWidth={800}
          canvasHeight={600}
          zoom={1}
          cropMode="free"
          cropAspect={null}
          onCropRectChange={setCropRect}
        />
      );
    }, container);

    const svgEl = container.querySelector("svg");
    expect(svgEl).not.toBeNull();

    svgEl!.dispatchEvent(
      new PointerEvent("pointerdown", {
        pointerId: 12,
        bubbles: true,
        cancelable: true,
        clientX: -20,
        clientY: -10,
      }),
    );
    svgEl!.dispatchEvent(
      new PointerEvent("pointermove", {
        pointerId: 12,
        bubbles: true,
        cancelable: true,
        clientX: 900,
        clientY: 710,
      }),
    );
    svgEl!.dispatchEvent(
      new PointerEvent("pointerup", {
        pointerId: 12,
        bubbles: true,
        cancelable: true,
        clientX: 900,
        clientY: 710,
      }),
    );

    const visualRect = container.querySelectorAll("rect")[3];
    expect(visualRect.getAttribute("x")).toBe("-20");
    expect(visualRect.getAttribute("y")).toBe("-10");
    expect(visualRect.getAttribute("width")).toBe("920");
    expect(visualRect.getAttribute("height")).toBe("720");

    SVGElement.prototype.setPointerCapture = origSet;
    SVGElement.prototype.releasePointerCapture = origRelease;
    dispose();
    container.parentNode?.removeChild(container);
  });

  it("ignores tiny new crop draws and restores the previous crop box", () => {
    const origSet = SVGElement.prototype.setPointerCapture;
    SVGElement.prototype.setPointerCapture = vi.fn();
    const origRelease = SVGElement.prototype.releasePointerCapture;
    SVGElement.prototype.releasePointerCapture = vi.fn();

    const container = document.createElement("div");
    document.body.appendChild(container);

    const dispose = render(() => {
      const [cropRect, setCropRect] = createSignal({ x: 50, y: 60, w: 200, h: 120 });
      return (
        <CropOverlay
          cropRect={cropRect()}
          guideMode="thirds"
          canvasWidth={800}
          canvasHeight={600}
          zoom={1}
          cropMode="free"
          cropAspect={null}
          onCropRectChange={setCropRect}
        />
      );
    }, container);

    const svgEl = container.querySelector("svg");
    expect(svgEl).not.toBeNull();

    svgEl!.dispatchEvent(
      new PointerEvent("pointerdown", {
        pointerId: 11,
        bubbles: true,
        cancelable: true,
        clientX: 300,
        clientY: 300,
      }),
    );
    svgEl!.dispatchEvent(
      new PointerEvent("pointermove", {
        pointerId: 11,
        bubbles: true,
        cancelable: true,
        clientX: 302,
        clientY: 302,
      }),
    );
    svgEl!.dispatchEvent(
      new PointerEvent("pointerup", {
        pointerId: 11,
        bubbles: true,
        cancelable: true,
        clientX: 302,
        clientY: 302,
      }),
    );

    const visualRect = container.querySelectorAll("rect")[3];
    expect(visualRect.getAttribute("x")).toBe("50");
    expect(visualRect.getAttribute("y")).toBe("60");
    expect(visualRect.getAttribute("width")).toBe("200");
    expect(visualRect.getAttribute("height")).toBe("120");

    SVGElement.prototype.setPointerCapture = origSet;
    SVGElement.prototype.releasePointerCapture = origRelease;
    dispose();
    container.parentNode?.removeChild(container);
  });
});

describe("CropOverlay viewport panning", () => {
  it("moves the crop rect without panning in Classic mode", () => {
    const origSet = SVGElement.prototype.setPointerCapture;
    SVGElement.prototype.setPointerCapture = vi.fn();
    const origRelease = SVGElement.prototype.releasePointerCapture;
    SVGElement.prototype.releasePointerCapture = vi.fn();

    const setViewportSpy = vi.fn();
    const syncViewportSpy = vi.fn();
    const requestRenderSpy = vi.fn();

    const mockEngine = {
      setViewport: setViewportSpy,
    };
    const mockWorkspace = {
      getActiveEngine: () => mockEngine,
    };
    const mockScheduler = {
      requestRender: requestRenderSpy,
    };

    const mockValue = {
      workspace: mockWorkspace,
      scheduler: mockScheduler,
      syncViewport: syncViewportSpy,
      camera: new ViewportCamera(),
      pan: () => ({ x: 100, y: 100 }),
      hoverPos: () => null,
      setHoverPos: vi.fn(),
    };

    const useEditorSpy = mockUseEditor(mockValue);

    const onCropRectChange = vi.fn();
    const container = document.createElement("div");
    document.body.appendChild(container);

    const dispose = render(
      () => (
        <CropOverlay
          cropRect={{ x: 10, y: 10, w: 100, h: 100 }}
          guideMode="thirds"
          canvasWidth={800}
          canvasHeight={600}
          zoom={1.5}
          cropMode="free"
          cropAspect={null}
          onCropRectChange={onCropRectChange}
        />
      ),
      container,
    );

    const svgEl = container.querySelector("svg");
    expect(svgEl).not.toBeNull();

    const moveZone = container.querySelector("[data-crop-move]");
    expect(moveZone).not.toBeNull();

    // Start drag at (50, 50)
    moveZone!.dispatchEvent(
      new PointerEvent("pointerdown", {
        pointerId: 10,
        bubbles: true,
        cancelable: true,
        clientX: 50,
        clientY: 50,
      }),
    );

    // Move to (70, 80) => screen delta (20, 30)
    svgEl!.dispatchEvent(
      new PointerEvent("pointermove", {
        pointerId: 10,
        bubbles: true,
        cancelable: true,
        clientX: 70,
        clientY: 80,
      }),
    );

    // In Classic mode, drag inside moves the crop rect without counter-panning the viewport.
    // zoom = 1.5, delta = (20, 30) screen → (20/1.5, 30/1.5) = (13.33, 20) document-space
    // new rect should be { x: 10 + 13.33, y: 10 + 20, w: 100, h: 100 }
    expect(setViewportSpy).not.toHaveBeenCalled();
    expect(onCropRectChange).toHaveBeenCalled();
    const callArgs = onCropRectChange.mock.calls[onCropRectChange.mock.calls.length - 1][0];
    expect(callArgs.x).toBeCloseTo(23.33, 1);
    expect(callArgs.y).toBeCloseTo(30, 1);
    expect(callArgs.w).toBe(100);
    expect(callArgs.h).toBe(100);

    useEditorSpy.mockRestore();
    SVGElement.prototype.setPointerCapture = origSet;
    SVGElement.prototype.releasePointerCapture = origRelease;
    dispose();
    container.parentNode?.removeChild(container);
  });

  it("keeps the Modern crop frame centered and moves the image underneath it", () => {
    const origSet = SVGElement.prototype.setPointerCapture;
    SVGElement.prototype.setPointerCapture = vi.fn();
    const origRelease = SVGElement.prototype.releasePointerCapture;
    SVGElement.prototype.releasePointerCapture = vi.fn();

    const onFrameChange = vi.fn();
    const onImageTransformChange = vi.fn();
    const container = document.createElement("div");
    document.body.appendChild(container);

    const dispose = render(
      () => (
        <ModernCropOverlay
          frame={{ x: 350, y: 300, w: 300, h: 200 }}
          imageTransform={{ offsetX: 0, offsetY: 0, rotation: 0, scale: 1 }}
          viewportWidth={1000}
          viewportHeight={800}
          projectedWidth={1000}
          projectedHeight={800}
          guideMode="thirds"
          cropMode="free"
          cropAspect={null}
          onFrameChange={onFrameChange}
          onImageTransformChange={onImageTransformChange}
        />
      ),
      container,
    );

    const svgEl = container.querySelector("svg");
    expect(svgEl).not.toBeNull();

    const frameRect = container.querySelector("rect[stroke='rgba(255,255,255,0.9)']");
    expect(frameRect?.getAttribute("x")).toBe("350");
    expect(frameRect?.getAttribute("y")).toBe("300");

    const moveZone = container.querySelector("[data-modern-crop-move]");
    expect(moveZone).not.toBeNull();

    moveZone!.dispatchEvent(
      new PointerEvent("pointerdown", {
        pointerId: 13,
        bubbles: true,
        cancelable: true,
        clientX: 50,
        clientY: 50,
      }),
    );

    svgEl!.dispatchEvent(
      new PointerEvent("pointermove", {
        pointerId: 13,
        bubbles: true,
        cancelable: true,
        clientX: 70,
        clientY: 90,
      }),
    );

    expect(onImageTransformChange).toHaveBeenCalledWith({
      offsetX: 20,
      offsetY: 40,
      rotation: 0,
      scale: 1,
    });

    const eastHandle = container.querySelector("[data-modern-crop-handle='e']");
    expect(eastHandle).not.toBeNull();
    eastHandle!.dispatchEvent(
      new PointerEvent("pointerdown", {
        pointerId: 14,
        bubbles: true,
        cancelable: true,
        clientX: 650,
        clientY: 400,
      }),
    );
    svgEl!.dispatchEvent(
      new PointerEvent("pointermove", {
        pointerId: 14,
        bubbles: true,
        cancelable: true,
        clientX: 670,
        clientY: 400,
      }),
    );
    expect(onFrameChange).toHaveBeenCalledWith({ x: 330, y: 300, w: 340, h: 200 });

    onImageTransformChange.mockClear();
    const rotateRing = container.querySelector("[data-modern-crop-rotate='ring']");
    expect(rotateRing).not.toBeNull();
    rotateRing!.dispatchEvent(
      new PointerEvent("pointerdown", {
        pointerId: 15,
        bubbles: true,
        cancelable: true,
        clientX: 350,
        clientY: 300,
      }),
    );
    svgEl!.dispatchEvent(
      new PointerEvent("pointermove", {
        pointerId: 15,
        bubbles: true,
        cancelable: true,
        clientX: 650,
        clientY: 300,
      }),
    );
    expect(onImageTransformChange).toHaveBeenCalledWith({
      offsetX: 0,
      offsetY: 0,
      rotation: 112.61986494804044,
      scale: 1,
    });

    SVGElement.prototype.setPointerCapture = origSet;
    SVGElement.prototype.releasePointerCapture = origRelease;
    dispose();
    container.parentNode?.removeChild(container);
  });

  it("keeps Modern crop move drag screen-aligned when the image is rotated", () => {
    const origSet = SVGElement.prototype.setPointerCapture;
    SVGElement.prototype.setPointerCapture = vi.fn();
    const origRelease = SVGElement.prototype.releasePointerCapture;
    SVGElement.prototype.releasePointerCapture = vi.fn();

    const onImageTransformChange = vi.fn();
    const container = document.createElement("div");
    document.body.appendChild(container);

    const dispose = render(
      () => (
        <ModernCropOverlay
          frame={{ x: 350, y: 300, w: 300, h: 200 }}
          imageTransform={{ offsetX: 0, offsetY: 0, rotation: 90, scale: 1 }}
          viewportWidth={1000}
          viewportHeight={800}
          projectedWidth={1000}
          projectedHeight={800}
          guideMode="thirds"
          cropMode="free"
          cropAspect={null}
          onFrameChange={vi.fn()}
          onImageTransformChange={onImageTransformChange}
        />
      ),
      container,
    );

    const svgEl = container.querySelector("svg");
    const moveZone = container.querySelector("[data-modern-crop-move]");
    expect(svgEl).not.toBeNull();
    expect(moveZone).not.toBeNull();

    moveZone!.dispatchEvent(new PointerEvent("pointerdown", {
      pointerId: 16,
      bubbles: true,
      cancelable: true,
      clientX: 50,
      clientY: 50,
    }));
    svgEl!.dispatchEvent(new PointerEvent("pointermove", {
      pointerId: 16,
      bubbles: true,
      cancelable: true,
      clientX: 70,
      clientY: 50,
    }));

    const moved = onImageTransformChange.mock.calls[0][0];
    expect(moved.offsetX).toBeCloseTo(0);
    expect(moved.offsetY).toBeCloseTo(-20);
    expect(moved.rotation).toBe(90);
    expect(moved.scale).toBe(1);

    SVGElement.prototype.setPointerCapture = origSet;
    SVGElement.prototype.releasePointerCapture = origRelease;
    dispose();
    container.parentNode?.removeChild(container);
  });

  it("dragging from the top rotate ring changes rotation", () => {
    const origSet = SVGElement.prototype.setPointerCapture;
    SVGElement.prototype.setPointerCapture = vi.fn();
    const origRelease = SVGElement.prototype.releasePointerCapture;
    SVGElement.prototype.releasePointerCapture = vi.fn();

    const onImageTransformChange = vi.fn();
    const container = document.createElement("div");
    document.body.appendChild(container);

    const dispose = render(
      () => (
        <ModernCropOverlay
          frame={{ x: 350, y: 300, w: 300, h: 200 }}
          imageTransform={{ offsetX: 0, offsetY: 0, rotation: 0, scale: 1 }}
          viewportWidth={1000}
          viewportHeight={800}
          projectedWidth={1000}
          projectedHeight={800}
          guideMode="none"
          cropMode="free"
          cropAspect={null}
          onFrameChange={vi.fn()}
          onImageTransformChange={onImageTransformChange}
        />
      ),
      container,
    );

    const svgEl = container.querySelector("svg")!;
    const rotateRing = container.querySelector("[data-modern-crop-rotate='ring']")!;
    const centerX = 500;
    const centerY = 400;

    rotateRing.dispatchEvent(
      new PointerEvent("pointerdown", {
        pointerId: 20,
        bubbles: true,
        cancelable: true,
        clientX: centerX,
        clientY: centerY - 150,
      }),
    );
    svgEl.dispatchEvent(
      new PointerEvent("pointermove", {
        pointerId: 20,
        bubbles: true,
        cancelable: true,
        clientX: centerX + 150,
        clientY: centerY,
      }),
    );

    const lastCall = onImageTransformChange.mock.calls[onImageTransformChange.mock.calls.length - 1][0];
    expect(lastCall.rotation).not.toBe(0);
    expect(lastCall.offsetX).toBe(0);
    expect(lastCall.offsetY).toBe(0);

    SVGElement.prototype.setPointerCapture = origSet;
    SVGElement.prototype.releasePointerCapture = origRelease;
    dispose();
    container.parentNode?.removeChild(container);
  });

  it("dragging from the side rotate ring changes rotation", () => {
    const origSet = SVGElement.prototype.setPointerCapture;
    SVGElement.prototype.setPointerCapture = vi.fn();
    const origRelease = SVGElement.prototype.releasePointerCapture;
    SVGElement.prototype.releasePointerCapture = vi.fn();

    const onImageTransformChange = vi.fn();
    const container = document.createElement("div");
    document.body.appendChild(container);

    const dispose = render(
      () => (
        <ModernCropOverlay
          frame={{ x: 350, y: 300, w: 300, h: 200 }}
          imageTransform={{ offsetX: 0, offsetY: 0, rotation: 0, scale: 1 }}
          viewportWidth={1000}
          viewportHeight={800}
          projectedWidth={1000}
          projectedHeight={800}
          guideMode="none"
          cropMode="free"
          cropAspect={null}
          onFrameChange={vi.fn()}
          onImageTransformChange={onImageTransformChange}
        />
      ),
      container,
    );

    const svgEl = container.querySelector("svg")!;
    const rotateRing = container.querySelector("[data-modern-crop-rotate='ring']")!;
    const centerX = 500;
    const centerY = 400;

    rotateRing.dispatchEvent(
      new PointerEvent("pointerdown", {
        pointerId: 21,
        bubbles: true,
        cancelable: true,
        clientX: centerX + 200,
        clientY: centerY,
      }),
    );
    svgEl.dispatchEvent(
      new PointerEvent("pointermove", {
        pointerId: 21,
        bubbles: true,
        cancelable: true,
        clientX: centerX,
        clientY: centerY - 200,
      }),
    );

    const lastCall = onImageTransformChange.mock.calls[onImageTransformChange.mock.calls.length - 1][0];
    expect(lastCall.rotation).not.toBe(0);

    SVGElement.prototype.setPointerCapture = origSet;
    SVGElement.prototype.releasePointerCapture = origRelease;
    dispose();
    container.parentNode?.removeChild(container);
  });

  it("rotation uses cropbox center as pivot", () => {
    const origSet = SVGElement.prototype.setPointerCapture;
    SVGElement.prototype.setPointerCapture = vi.fn();
    const origRelease = SVGElement.prototype.releasePointerCapture;
    SVGElement.prototype.releasePointerCapture = vi.fn();

    const onImageTransformChange = vi.fn();
    const container = document.createElement("div");
    document.body.appendChild(container);

    const dispose = render(
      () => (
        <ModernCropOverlay
          frame={{ x: 300, y: 250, w: 400, h: 300 }}
          imageTransform={{ offsetX: 0, offsetY: 0, rotation: 0, scale: 1 }}
          viewportWidth={1000}
          viewportHeight={800}
          projectedWidth={1000}
          projectedHeight={800}
          guideMode="none"
          cropMode="free"
          cropAspect={null}
          onFrameChange={vi.fn()}
          onImageTransformChange={onImageTransformChange}
        />
      ),
      container,
    );

    const svgEl = container.querySelector("svg")!;
    const rotateRing = container.querySelector("[data-modern-crop-rotate='ring']")!;
    const centerX = 500;
    const centerY = 400;

    rotateRing.dispatchEvent(
      new PointerEvent("pointerdown", {
        pointerId: 22,
        bubbles: true,
        cancelable: true,
        clientX: centerX,
        clientY: centerY - 200,
      }),
    );
    svgEl.dispatchEvent(
      new PointerEvent("pointermove", {
        pointerId: 22,
        bubbles: true,
        cancelable: true,
        clientX: centerX + 200,
        clientY: centerY,
      }),
    );

    const lastCall = onImageTransformChange.mock.calls[onImageTransformChange.mock.calls.length - 1][0];
    expect(lastCall.rotation).toBeCloseTo(90, 0);
    expect(lastCall.offsetX).toBe(0);
    expect(lastCall.offsetY).toBe(0);

    SVGElement.prototype.setPointerCapture = origSet;
    SVGElement.prototype.releasePointerCapture = origRelease;
    dispose();
    container.parentNode?.removeChild(container);
  });

  it("resize handle still triggers resize, not rotate", () => {
    const origSet = SVGElement.prototype.setPointerCapture;
    SVGElement.prototype.setPointerCapture = vi.fn();
    const origRelease = SVGElement.prototype.releasePointerCapture;
    SVGElement.prototype.releasePointerCapture = vi.fn();

    const onFrameChange = vi.fn();
    const onImageTransformChange = vi.fn();
    const container = document.createElement("div");
    document.body.appendChild(container);

    const dispose = render(
      () => (
        <ModernCropOverlay
          frame={{ x: 350, y: 300, w: 300, h: 200 }}
          imageTransform={{ offsetX: 0, offsetY: 0, rotation: 0, scale: 1 }}
          viewportWidth={1000}
          viewportHeight={800}
          projectedWidth={1000}
          projectedHeight={800}
          guideMode="none"
          cropMode="free"
          cropAspect={null}
          onFrameChange={onFrameChange}
          onImageTransformChange={onImageTransformChange}
        />
      ),
      container,
    );

    const svgEl = container.querySelector("svg")!;
    const eastHandle = container.querySelector("[data-modern-crop-handle='e']")!;
    eastHandle.dispatchEvent(
      new PointerEvent("pointerdown", {
        pointerId: 23,
        bubbles: true,
        cancelable: true,
        clientX: 650,
        clientY: 400,
      }),
    );
    svgEl.dispatchEvent(
      new PointerEvent("pointermove", {
        pointerId: 23,
        bubbles: true,
        cancelable: true,
        clientX: 670,
        clientY: 400,
      }),
    );

    expect(onFrameChange).toHaveBeenCalled();
    const resizeCall = onImageTransformChange.mock.calls[onImageTransformChange.mock.calls.length - 1][0];
    expect(resizeCall.rotation).toBe(0);
    expect(resizeCall.scale).toBe(1);

    SVGElement.prototype.setPointerCapture = origSet;
    SVGElement.prototype.releasePointerCapture = origRelease;
    dispose();
    container.parentNode?.removeChild(container);
  });

  it("Modern free corner resize uses Shift to preserve the current frame aspect", () => {
    const origSet = SVGElement.prototype.setPointerCapture;
    SVGElement.prototype.setPointerCapture = vi.fn();
    const origRelease = SVGElement.prototype.releasePointerCapture;
    SVGElement.prototype.releasePointerCapture = vi.fn();

    const onFrameChange = vi.fn();
    const container = document.createElement("div");
    document.body.appendChild(container);

    const dispose = render(
      () => (
        <ModernCropOverlay
          frame={{ x: 300, y: 250, w: 400, h: 300 }}
          imageTransform={{ offsetX: 0, offsetY: 0, rotation: 0, scale: 1 }}
          viewportWidth={1000}
          viewportHeight={800}
          projectedWidth={1000}
          projectedHeight={800}
          guideMode="none"
          cropMode="free"
          cropAspect={null}
          onFrameChange={onFrameChange}
          onImageTransformChange={vi.fn()}
        />
      ),
      container,
    );

    const svgEl = container.querySelector("svg")!;
    const seHandle = container.querySelector("[data-modern-crop-handle='se']")!;
    seHandle.dispatchEvent(new PointerEvent("pointerdown", {
      pointerId: 24,
      bubbles: true,
      cancelable: true,
      clientX: 700,
      clientY: 550,
    }));
    svgEl.dispatchEvent(new PointerEvent("pointermove", {
      pointerId: 24,
      bubbles: true,
      cancelable: true,
      clientX: 780,
      clientY: 550,
      shiftKey: true,
    }));

    const frame = onFrameChange.mock.calls.at(-1)?.[0];
    expect(frame.w / frame.h).toBeCloseTo(4 / 3);
    expect(frame.h).toBeGreaterThan(300);

    SVGElement.prototype.setPointerCapture = origSet;
    SVGElement.prototype.releasePointerCapture = origRelease;
    dispose();
    container.parentNode?.removeChild(container);
  });

  it("Modern ratio corner resize uses Shift to temporarily free resize", () => {
    const origSet = SVGElement.prototype.setPointerCapture;
    SVGElement.prototype.setPointerCapture = vi.fn();
    const origRelease = SVGElement.prototype.releasePointerCapture;
    SVGElement.prototype.releasePointerCapture = vi.fn();

    const onFrameChange = vi.fn();
    const container = document.createElement("div");
    document.body.appendChild(container);

    const dispose = render(
      () => (
        <ModernCropOverlay
          frame={{ x: 300, y: 250, w: 400, h: 300 }}
          imageTransform={{ offsetX: 0, offsetY: 0, rotation: 0, scale: 1 }}
          viewportWidth={1000}
          viewportHeight={800}
          projectedWidth={1000}
          projectedHeight={800}
          guideMode="none"
          cropMode="ratio"
          cropAspect={{ w: 1, h: 1 }}
          onFrameChange={onFrameChange}
          onImageTransformChange={vi.fn()}
        />
      ),
      container,
    );

    const svgEl = container.querySelector("svg")!;
    const seHandle = container.querySelector("[data-modern-crop-handle='se']")!;
    seHandle.dispatchEvent(new PointerEvent("pointerdown", {
      pointerId: 25,
      bubbles: true,
      cancelable: true,
      clientX: 700,
      clientY: 550,
    }));
    svgEl.dispatchEvent(new PointerEvent("pointermove", {
      pointerId: 25,
      bubbles: true,
      cancelable: true,
      clientX: 780,
      clientY: 570,
      shiftKey: true,
    }));

    expect(onFrameChange.mock.calls.at(-1)?.[0]).toEqual({ x: 260, y: 240, w: 480, h: 320 });

    SVGElement.prototype.setPointerCapture = origSet;
    SVGElement.prototype.releasePointerCapture = origRelease;
    dispose();
    container.parentNode?.removeChild(container);
  });

  it("Modern Alt resize grows from center without image compensation", () => {
    const origSet = SVGElement.prototype.setPointerCapture;
    SVGElement.prototype.setPointerCapture = vi.fn();
    const origRelease = SVGElement.prototype.releasePointerCapture;
    SVGElement.prototype.releasePointerCapture = vi.fn();

    const onFrameChange = vi.fn();
    const onImageTransformChange = vi.fn();
    const container = document.createElement("div");
    document.body.appendChild(container);

    const dispose = render(
      () => (
        <ModernCropOverlay
          frame={{ x: 300, y: 250, w: 400, h: 300 }}
          imageTransform={{ offsetX: 0, offsetY: 0, rotation: 0, scale: 1 }}
          viewportWidth={1000}
          viewportHeight={800}
          projectedWidth={1000}
          projectedHeight={800}
          guideMode="none"
          cropMode="free"
          cropAspect={null}
          onFrameChange={onFrameChange}
          onImageTransformChange={onImageTransformChange}
        />
      ),
      container,
    );

    const svgEl = container.querySelector("svg")!;
    const eastHandle = container.querySelector("[data-modern-crop-handle='e']")!;
    eastHandle.dispatchEvent(new PointerEvent("pointerdown", {
      pointerId: 26,
      bubbles: true,
      cancelable: true,
      clientX: 700,
      clientY: 400,
    }));
    svgEl.dispatchEvent(new PointerEvent("pointermove", {
      pointerId: 26,
      bubbles: true,
      cancelable: true,
      clientX: 740,
      clientY: 400,
      altKey: true,
    }));

    expect(onFrameChange.mock.calls.at(-1)?.[0]).toEqual({ x: 260, y: 250, w: 480, h: 300 });
    expect(onImageTransformChange).not.toHaveBeenCalled();

    SVGElement.prototype.setPointerCapture = origSet;
    SVGElement.prototype.releasePointerCapture = origRelease;
    dispose();
    container.parentNode?.removeChild(container);
  });

  it("Modern Shift+Alt corner resize preserves aspect from center", () => {
    const origSet = SVGElement.prototype.setPointerCapture;
    SVGElement.prototype.setPointerCapture = vi.fn();
    const origRelease = SVGElement.prototype.releasePointerCapture;
    SVGElement.prototype.releasePointerCapture = vi.fn();

    const onFrameChange = vi.fn();
    const onImageTransformChange = vi.fn();
    const container = document.createElement("div");
    document.body.appendChild(container);

    const dispose = render(
      () => (
        <ModernCropOverlay
          frame={{ x: 300, y: 250, w: 400, h: 300 }}
          imageTransform={{ offsetX: 0, offsetY: 0, rotation: 0, scale: 1 }}
          viewportWidth={1000}
          viewportHeight={800}
          projectedWidth={1000}
          projectedHeight={800}
          guideMode="none"
          cropMode="free"
          cropAspect={null}
          onFrameChange={onFrameChange}
          onImageTransformChange={onImageTransformChange}
        />
      ),
      container,
    );

    const svgEl = container.querySelector("svg")!;
    const seHandle = container.querySelector("[data-modern-crop-handle='se']")!;
    seHandle.dispatchEvent(new PointerEvent("pointerdown", {
      pointerId: 27,
      bubbles: true,
      cancelable: true,
      clientX: 700,
      clientY: 550,
    }));
    svgEl.dispatchEvent(new PointerEvent("pointermove", {
      pointerId: 27,
      bubbles: true,
      cancelable: true,
      clientX: 740,
      clientY: 550,
      shiftKey: true,
      altKey: true,
    }));

    const frame = onFrameChange.mock.calls.at(-1)?.[0];
    expect(frame.w / frame.h).toBeCloseTo(4 / 3);
    expect(frame.w).toBeGreaterThan(400);
    expect(onImageTransformChange).not.toHaveBeenCalled();

    SVGElement.prototype.setPointerCapture = origSet;
    SVGElement.prototype.releasePointerCapture = origRelease;
    dispose();
    container.parentNode?.removeChild(container);
  });

  it("classic crop: Escape during handle resize cancels drag and restores rect", () => {
    const origSet = SVGElement.prototype.setPointerCapture;
    SVGElement.prototype.setPointerCapture = vi.fn();
    const origRelease = SVGElement.prototype.releasePointerCapture;
    SVGElement.prototype.releasePointerCapture = vi.fn();

    const onCropRectChange = vi.fn();
    const container = document.createElement("div");
    document.body.appendChild(container);

    const dispose = render(
      () => (
        <div style={{ position: "relative", width: "800px", height: "600px" }}>
          <CropOverlay
            cropRect={{ x: 100, y: 100, w: 200, h: 200 }}
            guideMode="thirds"
            canvasWidth={800}
            canvasHeight={600}
            zoom={1}
            cropMode="free"
            cropAspect={null}
            onCropRectChange={onCropRectChange}
          />
        </div>
      ),
      container,
    );

    const svgEl = container.querySelector("svg")!;
    const seHandle = container.querySelector('[data-crop-handle="se"]')!;
    const rect = svgEl!.getBoundingClientRect();
    onCropRectChange.mockClear();

    seHandle.dispatchEvent(
      new PointerEvent("pointerdown", {
        pointerId: 10, bubbles: true, cancelable: true,
        clientX: rect.left + 300, clientY: rect.top + 300,
      }),
    );

    svgEl.dispatchEvent(
      new PointerEvent("pointermove", {
        pointerId: 10, bubbles: true, cancelable: true,
        clientX: rect.left + 320, clientY: rect.top + 320,
      }),
    );

    expect(onCropRectChange).toHaveBeenCalled();

    onCropRectChange.mockClear();

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }));

    expect(onCropRectChange).toHaveBeenCalled();
    const calledArg = onCropRectChange.mock.calls.at(-1)?.[0];
    expect(calledArg).toEqual({ x: 100, y: 100, w: 200, h: 200 });

    onCropRectChange.mockClear();

    svgEl.dispatchEvent(
      new PointerEvent("pointermove", {
        pointerId: 10, bubbles: true, cancelable: true,
        clientX: rect.left + 340, clientY: rect.top + 340,
      }),
    );

    expect(onCropRectChange).not.toHaveBeenCalled();

    SVGElement.prototype.setPointerCapture = origSet;
    SVGElement.prototype.releasePointerCapture = origRelease;
    dispose();
    container.parentNode?.removeChild(container);
  });

  it("classic crop: Escape during move drag cancels drag and restores rect", () => {
    const origSet = SVGElement.prototype.setPointerCapture;
    SVGElement.prototype.setPointerCapture = vi.fn();
    const origRelease = SVGElement.prototype.releasePointerCapture;
    SVGElement.prototype.releasePointerCapture = vi.fn();

    const onCropRectChange = vi.fn();
    const container = document.createElement("div");
    document.body.appendChild(container);

    const dispose = render(
      () => (
        <CropOverlay
          cropRect={{ x: 10, y: 10, w: 100, h: 100 }}
          guideMode="thirds"
          canvasWidth={800}
          canvasHeight={600}
          zoom={1}
          cropMode="free"
          cropAspect={null}
          onCropRectChange={onCropRectChange}
        />
      ),
      container,
    );

    const svgEl = container.querySelector("svg")!;
    const moveZone = container.querySelector("[data-crop-move]")!;
    onCropRectChange.mockClear();

    moveZone.dispatchEvent(
      new PointerEvent("pointerdown", {
        pointerId: 11, bubbles: true, cancelable: true,
        clientX: 50, clientY: 50,
      }),
    );

    svgEl.dispatchEvent(
      new PointerEvent("pointermove", {
        pointerId: 11, bubbles: true, cancelable: true,
        clientX: 70, clientY: 70,
      }),
    );

    expect(onCropRectChange).toHaveBeenCalled();
    onCropRectChange.mockClear();

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }));

    expect(onCropRectChange).toHaveBeenCalled();
    expect(onCropRectChange.mock.calls.at(-1)?.[0]).toEqual({ x: 10, y: 10, w: 100, h: 100 });

    onCropRectChange.mockClear();

    svgEl.dispatchEvent(
      new PointerEvent("pointermove", {
        pointerId: 11, bubbles: true, cancelable: true,
        clientX: 90, clientY: 90,
      }),
    );

    expect(onCropRectChange).not.toHaveBeenCalled();

    SVGElement.prototype.setPointerCapture = origSet;
    SVGElement.prototype.releasePointerCapture = origRelease;
    dispose();
    container.parentNode?.removeChild(container);
  });

  it("modern crop: Escape during resize drag restores startFrame and startTransform", () => {
    const origSet = SVGElement.prototype.setPointerCapture;
    SVGElement.prototype.setPointerCapture = vi.fn();
    const origRelease = SVGElement.prototype.releasePointerCapture;
    SVGElement.prototype.releasePointerCapture = vi.fn();

    const onFrameChange = vi.fn();
    const onImageTransformChange = vi.fn();
    const container = document.createElement("div");
    document.body.appendChild(container);

    const dispose = render(
      () => (
        <ModernCropOverlay
          frame={{ x: 350, y: 300, w: 300, h: 200 }}
          imageTransform={{ offsetX: 0, offsetY: 0, rotation: 0, scale: 1 }}
          viewportWidth={1000}
          viewportHeight={800}
          projectedWidth={1000}
          projectedHeight={800}
          guideMode="thirds"
          cropMode="free"
          cropAspect={null}
          onFrameChange={onFrameChange}
          onImageTransformChange={onImageTransformChange}
        />
      ),
      container,
    );

    const svgEl = container.querySelector("svg")!;
    const eastHandle = container.querySelector("[data-modern-crop-handle='e']")!;

    eastHandle.dispatchEvent(
      new PointerEvent("pointerdown", {
        pointerId: 12, bubbles: true, cancelable: true,
        clientX: 700, clientY: 400,
      }),
    );

    svgEl.dispatchEvent(
      new PointerEvent("pointermove", {
        pointerId: 12, bubbles: true, cancelable: true,
        clientX: 720, clientY: 400,
      }),
    );

    expect(onFrameChange).toHaveBeenCalled();
    onFrameChange.mockClear();
    onImageTransformChange.mockClear();

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }));

    // Escape should restore startFrame ({ x: 350, y: 300, w: 300, h: 200 })
    expect(onFrameChange).toHaveBeenCalledWith({ x: 350, y: 300, w: 300, h: 200 });
    // Escape should restore startTransform ({ offsetX: 0, offsetY: 0, rotation: 0, scale: 1 })
    expect(onImageTransformChange).toHaveBeenCalledWith({ offsetX: 0, offsetY: 0, rotation: 0, scale: 1 });

    onFrameChange.mockClear();
    onImageTransformChange.mockClear();

    svgEl.dispatchEvent(
      new PointerEvent("pointermove", {
        pointerId: 12, bubbles: true, cancelable: true,
        clientX: 740, clientY: 400,
      }),
    );

    // After Escape clears dragState, further moves should NOT call handlers
    expect(onFrameChange).not.toHaveBeenCalled();
    expect(onImageTransformChange).not.toHaveBeenCalled();

    SVGElement.prototype.setPointerCapture = origSet;
    SVGElement.prototype.releasePointerCapture = origRelease;
    dispose();
    container.parentNode?.removeChild(container);
  });

  it("modern crop: Escape during move drag restores startTransform", () => {
    const origSet = SVGElement.prototype.setPointerCapture;
    SVGElement.prototype.setPointerCapture = vi.fn();
    const origRelease = SVGElement.prototype.releasePointerCapture;
    SVGElement.prototype.releasePointerCapture = vi.fn();

    const onFrameChange = vi.fn();
    const onImageTransformChange = vi.fn();
    const container = document.createElement("div");
    document.body.appendChild(container);

    const dispose = render(
      () => (
        <ModernCropOverlay
          frame={{ x: 350, y: 300, w: 300, h: 200 }}
          imageTransform={{ offsetX: 0, offsetY: 0, rotation: 0, scale: 1 }}
          viewportWidth={1000}
          viewportHeight={800}
          projectedWidth={1000}
          projectedHeight={800}
          guideMode="thirds"
          cropMode="free"
          cropAspect={null}
          onFrameChange={onFrameChange}
          onImageTransformChange={onImageTransformChange}
        />
      ),
      container,
    );

    const svgEl = container.querySelector("svg")!;
    const moveZone = container.querySelector("[data-modern-crop-move]")!;

    moveZone.dispatchEvent(
      new PointerEvent("pointerdown", {
        pointerId: 13, bubbles: true, cancelable: true,
        clientX: 500, clientY: 400,
      }),
    );

    svgEl.dispatchEvent(
      new PointerEvent("pointermove", {
        pointerId: 13, bubbles: true, cancelable: true,
        clientX: 520, clientY: 420,
      }),
    );

    expect(onImageTransformChange).toHaveBeenCalled();
    onImageTransformChange.mockClear();

    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }));

    // Escape should restore startTransform ({ offsetX: 0, offsetY: 0, rotation: 0, scale: 1 })
    expect(onImageTransformChange).toHaveBeenCalledWith({ offsetX: 0, offsetY: 0, rotation: 0, scale: 1 });

    onImageTransformChange.mockClear();

    svgEl.dispatchEvent(
      new PointerEvent("pointermove", {
        pointerId: 13, bubbles: true, cancelable: true,
        clientX: 540, clientY: 440,
      }),
    );

    // After Escape clears dragState, further moves should NOT call handlers
    expect(onImageTransformChange).not.toHaveBeenCalled();

    SVGElement.prototype.setPointerCapture = origSet;
    SVGElement.prototype.releasePointerCapture = origRelease;
    dispose();
    container.parentNode?.removeChild(container);
  });

  it("modern crop: lostpointercapture during move drag stops further updates", () => {
    const origSet = SVGElement.prototype.setPointerCapture;
    SVGElement.prototype.setPointerCapture = vi.fn();
    const origRelease = SVGElement.prototype.releasePointerCapture;
    SVGElement.prototype.releasePointerCapture = vi.fn();

    const onFrameChange = vi.fn();
    const onImageTransformChange = vi.fn();
    const container = document.createElement("div");
    document.body.appendChild(container);

    const dispose = render(
      () => (
        <ModernCropOverlay
          frame={{ x: 350, y: 300, w: 300, h: 200 }}
          imageTransform={{ offsetX: 0, offsetY: 0, rotation: 0, scale: 1 }}
          viewportWidth={1000}
          viewportHeight={800}
          projectedWidth={1000}
          projectedHeight={800}
          guideMode="thirds"
          cropMode="free"
          cropAspect={null}
          onFrameChange={onFrameChange}
          onImageTransformChange={onImageTransformChange}
        />
      ),
      container,
    );

    const svgEl = container.querySelector("svg")!;
    const moveZone = container.querySelector("[data-modern-crop-move]")!;

    moveZone.dispatchEvent(
      new PointerEvent("pointerdown", {
        pointerId: 20, bubbles: true, cancelable: true,
        clientX: 500, clientY: 400,
      }),
    );

    svgEl.dispatchEvent(
      new PointerEvent("pointermove", {
        pointerId: 20, bubbles: true, cancelable: true,
        clientX: 520, clientY: 420,
      }),
    );

    expect(onImageTransformChange).toHaveBeenCalled();
    onImageTransformChange.mockClear();

    svgEl.dispatchEvent(new PointerEvent("lostpointercapture", { bubbles: false, pointerId: 20 }));

    svgEl.dispatchEvent(
      new PointerEvent("pointermove", {
        pointerId: 20, bubbles: true, cancelable: true,
        clientX: 540, clientY: 440,
      }),
    );

    expect(onImageTransformChange).not.toHaveBeenCalled();

    SVGElement.prototype.setPointerCapture = origSet;
    SVGElement.prototype.releasePointerCapture = origRelease;
    dispose();
    container.parentNode?.removeChild(container);
  });

  it("modern crop: lostpointercapture during resize drag stops further updates", () => {
    const origSet = SVGElement.prototype.setPointerCapture;
    SVGElement.prototype.setPointerCapture = vi.fn();
    const origRelease = SVGElement.prototype.releasePointerCapture;
    SVGElement.prototype.releasePointerCapture = vi.fn();

    const onFrameChange = vi.fn();
    const onImageTransformChange = vi.fn();
    const container = document.createElement("div");
    document.body.appendChild(container);

    const dispose = render(
      () => (
        <ModernCropOverlay
          frame={{ x: 350, y: 300, w: 300, h: 200 }}
          imageTransform={{ offsetX: 0, offsetY: 0, rotation: 0, scale: 1 }}
          viewportWidth={1000}
          viewportHeight={800}
          projectedWidth={1000}
          projectedHeight={800}
          guideMode="thirds"
          cropMode="free"
          cropAspect={null}
          onFrameChange={onFrameChange}
          onImageTransformChange={onImageTransformChange}
        />
      ),
      container,
    );

    const svgEl = container.querySelector("svg")!;
    const eastHandle = container.querySelector("[data-modern-crop-handle='e']")!;

    eastHandle.dispatchEvent(
      new PointerEvent("pointerdown", {
        pointerId: 21, bubbles: true, cancelable: true,
        clientX: 700, clientY: 400,
      }),
    );

    svgEl.dispatchEvent(
      new PointerEvent("pointermove", {
        pointerId: 21, bubbles: true, cancelable: true,
        clientX: 720, clientY: 400,
      }),
    );

    expect(onFrameChange).toHaveBeenCalled();
    onFrameChange.mockClear();

    svgEl.dispatchEvent(new PointerEvent("lostpointercapture", { bubbles: false, pointerId: 21 }));

    svgEl.dispatchEvent(
      new PointerEvent("pointermove", {
        pointerId: 21, bubbles: true, cancelable: true,
        clientX: 740, clientY: 400,
      }),
    );

    expect(onFrameChange).not.toHaveBeenCalled();

    SVGElement.prototype.setPointerCapture = origSet;
    SVGElement.prototype.releasePointerCapture = origRelease;
    dispose();
    container.parentNode?.removeChild(container);
  });

  it("modern crop: lostpointercapture during rotate drag stops further updates", () => {
    const origSet = SVGElement.prototype.setPointerCapture;
    SVGElement.prototype.setPointerCapture = vi.fn();
    const origRelease = SVGElement.prototype.releasePointerCapture;
    SVGElement.prototype.releasePointerCapture = vi.fn();

    const onFrameChange = vi.fn();
    const onImageTransformChange = vi.fn();
    const container = document.createElement("div");
    document.body.appendChild(container);

    const dispose = render(
      () => (
        <ModernCropOverlay
          frame={{ x: 350, y: 300, w: 300, h: 200 }}
          imageTransform={{ offsetX: 0, offsetY: 0, rotation: 0, scale: 1 }}
          viewportWidth={1000}
          viewportHeight={800}
          projectedWidth={1000}
          projectedHeight={800}
          guideMode="thirds"
          cropMode="free"
          cropAspect={null}
          onFrameChange={onFrameChange}
          onImageTransformChange={onImageTransformChange}
        />
      ),
      container,
    );

    const svgEl = container.querySelector("svg")!;
    const rotateZone = container.querySelector("[data-modern-crop-rotate]")!;

    rotateZone.dispatchEvent(
      new PointerEvent("pointerdown", {
        pointerId: 22, bubbles: true, cancelable: true,
        clientX: 500, clientY: 300,
      }),
    );

    svgEl.dispatchEvent(
      new PointerEvent("pointermove", {
        pointerId: 22, bubbles: true, cancelable: true,
        clientX: 520, clientY: 300,
      }),
    );

    expect(onImageTransformChange).toHaveBeenCalled();
    onImageTransformChange.mockClear();

    svgEl.dispatchEvent(new PointerEvent("lostpointercapture", { bubbles: false, pointerId: 22 }));

    svgEl.dispatchEvent(
      new PointerEvent("pointermove", {
        pointerId: 22, bubbles: true, cancelable: true,
        clientX: 540, clientY: 300,
      }),
    );

    expect(onImageTransformChange).not.toHaveBeenCalled();

    SVGElement.prototype.setPointerCapture = origSet;
    SVGElement.prototype.releasePointerCapture = origRelease;
    dispose();
    container.parentNode?.removeChild(container);
  });

  it("modern crop: double-click on move zone calls onApplyCrop", () => {
    const origSet = SVGElement.prototype.setPointerCapture;
    SVGElement.prototype.setPointerCapture = vi.fn();
    const origRelease = SVGElement.prototype.releasePointerCapture;
    SVGElement.prototype.releasePointerCapture = vi.fn();
    const origElementFromPoint = document.elementFromPoint;
    document.elementFromPoint = vi.fn();

    const onApplyCrop = vi.fn();
    const onFrameChange = vi.fn();
    const onImageTransformChange = vi.fn();
    const container = document.createElement("div");
    document.body.appendChild(container);

    const dispose = render(
      () => (
        <ModernCropOverlay
          frame={{ x: 350, y: 300, w: 300, h: 200 }}
          imageTransform={{ offsetX: 0, offsetY: 0, rotation: 0, scale: 1 }}
          viewportWidth={1000}
          viewportHeight={800}
          projectedWidth={1000}
          projectedHeight={800}
          guideMode="thirds"
          cropMode="free"
          cropAspect={null}
          onFrameChange={onFrameChange}
          onImageTransformChange={onImageTransformChange}
          onApplyCrop={onApplyCrop}
        />
      ),
      container,
    );

    const moveZone = container.querySelector("[data-modern-crop-move]")!;
    expect(moveZone).not.toBeNull();

    const svgEl = container.querySelector("svg")!;

    // Mock elementFromPoint to return the move zone
    (document.elementFromPoint as any).mockReturnValue(moveZone);

    // First click: pointerdown + pointerup on move zone
    moveZone.dispatchEvent(new PointerEvent("pointerdown", {
      pointerId: 50, bubbles: true, cancelable: true,
      clientX: 500, clientY: 400,
    }));
    svgEl.dispatchEvent(new PointerEvent("pointerup", {
      pointerId: 50, bubbles: true, cancelable: true,
      clientX: 500, clientY: 400,
    }));

    expect(onApplyCrop).not.toHaveBeenCalled();

    // Second click: pointerdown + pointerup → browser generates dblclick on SVG
    moveZone.dispatchEvent(new PointerEvent("pointerdown", {
      pointerId: 51, bubbles: true, cancelable: true,
      clientX: 500, clientY: 400,
    }));
    svgEl.dispatchEvent(new PointerEvent("pointerup", {
      pointerId: 51, bubbles: true, cancelable: true,
      clientX: 500, clientY: 400,
    }));

    // Simulate dblclick on SVG (browser would generate this after two clicks)
    svgEl.dispatchEvent(new MouseEvent("dblclick", {
      bubbles: true, cancelable: true,
      clientX: 500, clientY: 400,
    }));

    expect(onApplyCrop).toHaveBeenCalledTimes(1);

    document.elementFromPoint = origElementFromPoint;
    SVGElement.prototype.setPointerCapture = origSet;
    SVGElement.prototype.releasePointerCapture = origRelease;
    dispose();
    container.parentNode?.removeChild(container);
  });

  it("modern crop: double-click outside move zone does NOT call onApplyCrop", () => {
    const origSet = SVGElement.prototype.setPointerCapture;
    SVGElement.prototype.setPointerCapture = vi.fn();
    const origRelease = SVGElement.prototype.releasePointerCapture;
    SVGElement.prototype.releasePointerCapture = vi.fn();
    const origElementFromPoint = document.elementFromPoint;
    document.elementFromPoint = vi.fn();

    const onApplyCrop = vi.fn();
    const onFrameChange = vi.fn();
    const onImageTransformChange = vi.fn();
    const container = document.createElement("div");
    document.body.appendChild(container);

    const dispose = render(
      () => (
        <ModernCropOverlay
          frame={{ x: 350, y: 300, w: 300, h: 200 }}
          imageTransform={{ offsetX: 0, offsetY: 0, rotation: 0, scale: 1 }}
          viewportWidth={1000}
          viewportHeight={800}
          projectedWidth={1000}
          projectedHeight={800}
          guideMode="thirds"
          cropMode="free"
          cropAspect={null}
          onFrameChange={onFrameChange}
          onImageTransformChange={onImageTransformChange}
          onApplyCrop={onApplyCrop}
        />
      ),
      container,
    );

    const svgEl = container.querySelector("svg")!;

    // Mock elementFromPoint to return a non-move-zone element
    const outsideEl = document.createElement("div");
    (document.elementFromPoint as any).mockReturnValue(outsideEl);

    // dblclick outside move zone
    svgEl.dispatchEvent(new MouseEvent("dblclick", {
      bubbles: true, cancelable: true,
      clientX: 50, clientY: 50,
    }));

    expect(onApplyCrop).not.toHaveBeenCalled();

    document.elementFromPoint = origElementFromPoint;
    SVGElement.prototype.setPointerCapture = origSet;
    SVGElement.prototype.releasePointerCapture = origRelease;
    dispose();
    container.parentNode?.removeChild(container);
  });

  it("modern crop: double-click during drag does NOT call onApplyCrop", () => {
    const origSet = SVGElement.prototype.setPointerCapture;
    SVGElement.prototype.setPointerCapture = vi.fn();
    const origRelease = SVGElement.prototype.releasePointerCapture;
    SVGElement.prototype.releasePointerCapture = vi.fn();
    const origElementFromPoint = document.elementFromPoint;
    document.elementFromPoint = vi.fn();

    const onApplyCrop = vi.fn();
    const onFrameChange = vi.fn();
    const onImageTransformChange = vi.fn();
    const container = document.createElement("div");
    document.body.appendChild(container);

    const dispose = render(
      () => (
        <ModernCropOverlay
          frame={{ x: 350, y: 300, w: 300, h: 200 }}
          imageTransform={{ offsetX: 0, offsetY: 0, rotation: 0, scale: 1 }}
          viewportWidth={1000}
          viewportHeight={800}
          projectedWidth={1000}
          projectedHeight={800}
          guideMode="thirds"
          cropMode="free"
          cropAspect={null}
          onFrameChange={onFrameChange}
          onImageTransformChange={onImageTransformChange}
          onApplyCrop={onApplyCrop}
        />
      ),
      container,
    );

    const moveZone = container.querySelector("[data-modern-crop-move]")!;
    const svgEl = container.querySelector("svg")!;

    // Start a drag
    moveZone.dispatchEvent(new PointerEvent("pointerdown", {
      pointerId: 60, bubbles: true, cancelable: true,
      clientX: 500, clientY: 400,
    }));

    // dblclick during active drag should be ignored
    svgEl.dispatchEvent(new MouseEvent("dblclick", {
      bubbles: true, cancelable: true,
      clientX: 500, clientY: 400,
    }));

    expect(onApplyCrop).not.toHaveBeenCalled();

    // Cleanup drag
    svgEl.dispatchEvent(new PointerEvent("pointerup", {
      pointerId: 60, bubbles: true, cancelable: true,
      clientX: 500, clientY: 400,
    }));

    document.elementFromPoint = origElementFromPoint;
    SVGElement.prototype.setPointerCapture = origSet;
    SVGElement.prototype.releasePointerCapture = origRelease;
    dispose();
    container.parentNode?.removeChild(container);
  });

  it("modern crop: single click does NOT call onApplyCrop", () => {
    const origSet = SVGElement.prototype.setPointerCapture;
    SVGElement.prototype.setPointerCapture = vi.fn();
    const origRelease = SVGElement.prototype.releasePointerCapture;
    SVGElement.prototype.releasePointerCapture = vi.fn();
    const origElementFromPoint = document.elementFromPoint;
    document.elementFromPoint = vi.fn();

    const onApplyCrop = vi.fn();
    const onFrameChange = vi.fn();
    const onImageTransformChange = vi.fn();
    const container = document.createElement("div");
    document.body.appendChild(container);

    const dispose = render(
      () => (
        <ModernCropOverlay
          frame={{ x: 350, y: 300, w: 300, h: 200 }}
          imageTransform={{ offsetX: 0, offsetY: 0, rotation: 0, scale: 1 }}
          viewportWidth={1000}
          viewportHeight={800}
          projectedWidth={1000}
          projectedHeight={800}
          guideMode="thirds"
          cropMode="free"
          cropAspect={null}
          onFrameChange={onFrameChange}
          onImageTransformChange={onImageTransformChange}
          onApplyCrop={onApplyCrop}
        />
      ),
      container,
    );

    const moveZone = container.querySelector("[data-modern-crop-move]")!;
    const svgEl = container.querySelector("svg")!;

    // Single click: pointerdown + pointerup, no second click
    moveZone.dispatchEvent(new PointerEvent("pointerdown", {
      pointerId: 70, bubbles: true, cancelable: true,
      clientX: 500, clientY: 400,
    }));
    svgEl.dispatchEvent(new PointerEvent("pointerup", {
      pointerId: 70, bubbles: true, cancelable: true,
      clientX: 500, clientY: 400,
    }));

    // Dispatch a single click event (not dblclick)
    svgEl.dispatchEvent(new MouseEvent("click", {
      bubbles: true, cancelable: true,
      clientX: 500, clientY: 400,
    }));

    expect(onApplyCrop).not.toHaveBeenCalled();

    document.elementFromPoint = origElementFromPoint;
    SVGElement.prototype.setPointerCapture = origSet;
    SVGElement.prototype.releasePointerCapture = origRelease;
    dispose();
    container.parentNode?.removeChild(container);
  });

  it("classic crop east handle resize with 45 degree rotation", () => {
    const origSet = SVGElement.prototype.setPointerCapture;
    SVGElement.prototype.setPointerCapture = vi.fn();
    const origRelease = SVGElement.prototype.releasePointerCapture;
    SVGElement.prototype.releasePointerCapture = vi.fn();

    const onCropRectChange = vi.fn();
    const container = document.createElement("div");
    document.body.appendChild(container);

    const dispose = render(
      () => (
        <CropOverlay
          cropRect={{ x: 100, y: 100, w: 200, h: 100 }}
          guideMode="none"
          canvasWidth={800}
          canvasHeight={600}
          zoom={1}
          cropMode="free"
          cropAspect={null}
          cropRotation={45}
          onCropRectChange={onCropRectChange}
          onCropRotationChange={vi.fn()}
        />
      ),
      container,
    );

    const svgEl = container.querySelector("svg")!;

    // East handle at unrotated SVG position: (300, 150)
    // Visually rotated 45° about center (200, 150): (270.7, 220.7)
    const cx = 200, cy = 150;
    const ox = 300 - cx, oy = 150 - cy;
    const vx = cx + ox * 0.70710678 - oy * 0.70710678;
    const vy = cy + ox * 0.70710678 + oy * 0.70710678;

    const eastHandle = container.querySelector("[data-crop-handle='e']")!;
    expect(eastHandle).not.toBeNull();
    eastHandle.dispatchEvent(new PointerEvent("pointerdown", {
      pointerId: 50, bubbles: true, cancelable: true,
      clientX: vx, clientY: vy,
    }));
    svgEl.dispatchEvent(new PointerEvent("pointermove", {
      pointerId: 50, bubbles: true, cancelable: true,
      clientX: vx + 20, clientY: vy,
    }));
    const lastRect = onCropRectChange.mock.calls.at(-1)?.[0];

    // East handle at 45° rotation should resize along local X axis.
    // local dx = 20 * cos(-45) = 14.14
    expect(lastRect.w).toBeCloseTo(214.14, 1);
    expect(lastRect.h).toBe(100);
    expect(lastRect.x).toBeCloseTo(97.93, 1);
    expect(lastRect.y).toBe(105);

    SVGElement.prototype.setPointerCapture = origSet;
    SVGElement.prototype.releasePointerCapture = origRelease;
    dispose();
    container.parentNode?.removeChild(container);
  });

  it("classic crop east handle resize with 90 degree rotation", () => {
    const origSet = SVGElement.prototype.setPointerCapture;
    SVGElement.prototype.setPointerCapture = vi.fn();
    const origRelease = SVGElement.prototype.releasePointerCapture;
    SVGElement.prototype.releasePointerCapture = vi.fn();

    const onCropRectChange = vi.fn();
    const container = document.createElement("div");
    document.body.appendChild(container);

    const dispose = render(
      () => (
        <CropOverlay
          cropRect={{ x: 100, y: 100, w: 200, h: 100 }}
          guideMode="none"
          canvasWidth={800}
          canvasHeight={600}
          zoom={1}
          cropMode="free"
          cropAspect={null}
          cropRotation={90}
          onCropRectChange={onCropRectChange}
          onCropRotationChange={vi.fn()}
        />
      ),
      container,
    );

    const svgEl = container.querySelector("svg")!;

    // East handle at 90°: drag DOWN should increase w (local width) because East handle is rotated to bottom
    const eastHandle = container.querySelector("[data-crop-handle='e']")!;
    expect(eastHandle).not.toBeNull();
    eastHandle.dispatchEvent(new PointerEvent("pointerdown", {
      pointerId: 60, bubbles: true, cancelable: true,
      clientX: 200, clientY: 250,
    }));
    svgEl.dispatchEvent(new PointerEvent("pointermove", {
      pointerId: 60, bubbles: true, cancelable: true,
      clientX: 200, clientY: 260,
    }));
    const lastRect = onCropRectChange.mock.calls.at(-1)?.[0];
    // local delta: dx = 10, dy = 0 → w += 10
    expect(lastRect.h).toBe(100);
    expect(lastRect.w).toBe(210);

    SVGElement.prototype.setPointerCapture = origSet;
    SVGElement.prototype.releasePointerCapture = origRelease;
    dispose();
    container.parentNode?.removeChild(container);
  });

  it("classic crop south handle resize with 90 degree rotation", () => {
    const origSet = SVGElement.prototype.setPointerCapture;
    SVGElement.prototype.setPointerCapture = vi.fn();
    const origRelease = SVGElement.prototype.releasePointerCapture;
    SVGElement.prototype.releasePointerCapture = vi.fn();

    const onCropRectChange = vi.fn();
    const container = document.createElement("div");
    document.body.appendChild(container);

    const dispose = render(
      () => (
        <CropOverlay
          cropRect={{ x: 100, y: 100, w: 200, h: 100 }}
          guideMode="none"
          canvasWidth={800}
          canvasHeight={600}
          zoom={1}
          cropMode="free"
          cropAspect={null}
          cropRotation={90}
          onCropRectChange={onCropRectChange}
          onCropRotationChange={vi.fn()}
        />
      ),
      container,
    );

    const svgEl = container.querySelector("svg")!;

    // South handle at 90°: drag LEFT should increase h (local height) because South handle is rotated to left
    const southHandle = container.querySelector("[data-crop-handle='s']")!;
    expect(southHandle).not.toBeNull();
    southHandle.dispatchEvent(new PointerEvent("pointerdown", {
      pointerId: 61, bubbles: true, cancelable: true,
      clientX: 150, clientY: 200,
    }));
    svgEl.dispatchEvent(new PointerEvent("pointermove", {
      pointerId: 61, bubbles: true, cancelable: true,
      clientX: 140, clientY: 200,
    }));
    const lastRect = onCropRectChange.mock.calls.at(-1)?.[0];
    // local delta: dx = 0, dy = 10 → h += 10
    expect(lastRect.w).toBe(200);
    expect(lastRect.h).toBe(110);
    expect(lastRect.x).toBe(95);
    expect(lastRect.y).toBe(95);

    SVGElement.prototype.setPointerCapture = origSet;
    SVGElement.prototype.releasePointerCapture = origRelease;
    dispose();
    container.parentNode?.removeChild(container);
  });

  it("classic crop east handle resize with 180 degree rotation", () => {
    const origSet = SVGElement.prototype.setPointerCapture;
    SVGElement.prototype.setPointerCapture = vi.fn();
    const origRelease = SVGElement.prototype.releasePointerCapture;
    SVGElement.prototype.releasePointerCapture = vi.fn();

    const onCropRectChange = vi.fn();
    const container = document.createElement("div");
    document.body.appendChild(container);

    const dispose = render(
      () => (
        <CropOverlay
          cropRect={{ x: 100, y: 100, w: 200, h: 100 }}
          guideMode="none"
          canvasWidth={800}
          canvasHeight={600}
          zoom={1}
          cropMode="free"
          cropAspect={null}
          cropRotation={180}
          onCropRectChange={onCropRectChange}
          onCropRotationChange={vi.fn()}
        />
      ),
      container,
    );

    const svgEl = container.querySelector("svg")!;

    const eastHandle = container.querySelector("[data-crop-handle='e']")!;
    expect(eastHandle).not.toBeNull();
    eastHandle.dispatchEvent(new PointerEvent("pointerdown", {
      pointerId: 62, bubbles: true, cancelable: true,
      clientX: 300, clientY: 150,
    }));
    svgEl.dispatchEvent(new PointerEvent("pointermove", {
      pointerId: 62, bubbles: true, cancelable: true,
      clientX: 310, clientY: 150,
    }));
    const lastRect = onCropRectChange.mock.calls.at(-1)?.[0];
    // local delta at 180 deg: dx = -10, dy = 0 → w -= 10 (since East handle drags right but is rotated to left)
    expect(lastRect.w).toBe(190);
    expect(lastRect.h).toBe(100);
    expect(lastRect.x).toBe(110);
    expect(lastRect.y).toBe(100);

    SVGElement.prototype.setPointerCapture = origSet;
    SVGElement.prototype.releasePointerCapture = origRelease;
    dispose();
    container.parentNode?.removeChild(container);
  });

  it("classic crop SE corner resize with 45 degree rotation", () => {
    const origSet = SVGElement.prototype.setPointerCapture;
    SVGElement.prototype.setPointerCapture = vi.fn();
    const origRelease = SVGElement.prototype.releasePointerCapture;
    SVGElement.prototype.releasePointerCapture = vi.fn();

    const onCropRectChange = vi.fn();
    const container = document.createElement("div");
    document.body.appendChild(container);

    const dispose = render(
      () => (
        <CropOverlay
          cropRect={{ x: 100, y: 100, w: 200, h: 100 }}
          guideMode="none"
          canvasWidth={800}
          canvasHeight={600}
          zoom={1}
          cropMode="free"
          cropAspect={null}
          cropRotation={45}
          onCropRectChange={onCropRectChange}
          onCropRotationChange={vi.fn()}
        />
      ),
      container,
    );

    const svgEl = container.querySelector("svg")!;

    // SE corner at unrotated SVG position: (300, 200)
    // Visually rotated 45° about center (200, 150):
    const cx2 = 200, cy2 = 150;
    const ox2 = 300 - cx2, oy2 = 200 - cy2;
    const vx2 = cx2 + ox2 * 0.70710678 - oy2 * 0.70710678;
    const vy2 = cy2 + ox2 * 0.70710678 + oy2 * 0.70710678;

    const seHandle = container.querySelector("[data-crop-handle='se']")!;
    expect(seHandle).not.toBeNull();
    seHandle.dispatchEvent(new PointerEvent("pointerdown", {
      pointerId: 51, bubbles: true, cancelable: true,
      clientX: vx2, clientY: vy2,
    }));
    svgEl.dispatchEvent(new PointerEvent("pointermove", {
      pointerId: 51, bubbles: true, cancelable: true,
      clientX: vx2 + 20, clientY: vy2,
    }));
    const lastRect = onCropRectChange.mock.calls.at(-1)?.[0];

    // SE corner: both w and h change. At 45°, rightward 20px gives:
    // local = (14.14, -14.14) → w += 14.14, h += -14.14 (h decreases!)
    expect(lastRect.w).toBeCloseTo(214.14, 1);
    expect(lastRect.h).toBeCloseTo(85.86, 1);

    SVGElement.prototype.setPointerCapture = origSet;
    SVGElement.prototype.releasePointerCapture = origRelease;
    dispose();
    container.parentNode?.removeChild(container);
  });
});
