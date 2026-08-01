import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import { mockUseEditor } from "@/__tests__/mockUseEditor";
import { render } from "solid-js/web";
import { createSignal } from "solid-js";
import { BrushCursorOverlay } from "../BrushCursorOverlay";
import { ViewportCamera } from "../../../viewport/viewportCamera";

describe("BrushCursorOverlay", () => {
  it("reacts to brush hardness with one calibrated cursor ring", () => {
    const [activeTool, setActiveTool] = createSignal("brush");
    const [zoom] = createSignal(1);
    const [brushSize] = createSignal(24);
    const [brushHardness, setBrushHardness] = createSignal(0);

    mockUseEditor({
      activeTool,
      setActiveTool,
      zoom,
      camera: new ViewportCamera(),
      brushSize,
      brushHardness,
      brushOpacity: () => 1,
      eraserSize: () => 40,
      eraserHardness: () => 1,
      eraserOpacity: () => 1,
    });

    const root = document.createElement("svg");
    document.body.appendChild(root);
    const dispose = render(() => <BrushCursorOverlay forceVisibleForTest cursorPosForTest={{ x: 10, y: 10 }} />, root);

    const softRadius = Number(root.querySelector("[data-paint-cursor-outer]")?.getAttribute("rx"));
    expect(softRadius).toBeCloseTo(12 * 0.661 * Math.sqrt(-Math.log(0.2)), 10);
    expect(root.querySelector("[data-paint-cursor-hardness]")).toBeNull();

    setBrushHardness(0.97);
    expect(root.querySelector("[data-paint-cursor-outer]")?.getAttribute("rx")).toBe("12");

    dispose();
    root.remove();
    vi.restoreAllMocks();
  });

  it("uses eraser size and hardness when eraser is active", () => {
    const [activeTool, setActiveTool] = createSignal("eraser");
    const [zoom] = createSignal(1);
    const [eraserSize] = createSignal(40);

    mockUseEditor({
      activeTool,
      setActiveTool,
      zoom,
      camera: new ViewportCamera(),
      brushSize: () => 24,
      brushHardness: () => 0.5,
      brushOpacity: () => 1,
      eraserSize,
      eraserHardness: () => 0,
      eraserOpacity: () => 1,
    });

    const root = document.createElement("svg");
    document.body.appendChild(root);
    const dispose = render(() => <BrushCursorOverlay forceVisibleForTest cursorPosForTest={{ x: 10, y: 10 }} />, root);

    const radius = Number(root.querySelector("[data-paint-cursor-outer]")?.getAttribute("rx"));
    expect(radius).toBeCloseTo(20 * 0.661 * Math.sqrt(-Math.log(0.2)), 10);

    dispose();
    root.remove();
    vi.restoreAllMocks();
  });

  it("keeps radius in document space at zoom != 1 (not divided by zoom)", () => {
    const [activeTool] = createSignal("brush");
    const [zoom] = createSignal(2);
    const [brushSize] = createSignal(24);

    mockUseEditor({
      activeTool,
      zoom,
      camera: new ViewportCamera(),
      brushSize,
      brushHardness: () => 0.97,
      brushOpacity: () => 1,
      eraserSize: () => 40,
      eraserHardness: () => 1,
      eraserOpacity: () => 1,
    });

    const root = document.createElement("svg");
    document.body.appendChild(root);
    const dispose = render(() => <BrushCursorOverlay forceVisibleForTest cursorPosForTest={{ x: 10, y: 10 }} />, root);

    // r should be screen-space radius (12 * zoom = 24)
    expect(root.querySelector("[data-paint-cursor-outer]")?.getAttribute("rx")).toBe("24");

    dispose();
    root.remove();
    vi.restoreAllMocks();
  });

  it("responds to window pointermove event (wiring test)", () => {
    const [activeTool] = createSignal("brush");
    const [zoom] = createSignal(1);

    // Provide a viewport container so updatePosition() can compute document coords
    const vpContainer = document.createElement("div");
    vpContainer.setAttribute("data-viewport-container", "");
    document.body.appendChild(vpContainer);

    mockUseEditor({
      activeTool,
      zoom,
      camera: new ViewportCamera(),
      brushSize: () => 24,
      brushHardness: () => 0.97,
      brushOpacity: () => 1,
      eraserSize: () => 40,
      eraserHardness: () => 1,
      eraserOpacity: () => 1,
    });

    const root = document.createElement("svg");
    document.body.appendChild(root);
    const dispose = render(() => BrushCursorOverlay(), root);

    // Before any pointer move, cursor should NOT be visible
    expect(root.querySelector("[data-paint-cursor-outer]")).toBeNull();

    // Fire pointermove — this exercises the production event listener
    window.dispatchEvent(new PointerEvent("pointermove", {
      clientX: 100,
      clientY: 200,
      bubbles: true,
    }));

    // Cursor circle should now be rendered
    const circle = root.querySelector("[data-paint-cursor-outer]");
    expect(circle).not.toBeNull();
    expect(circle?.getAttribute("rx")).toBe("12");

    dispose();
    root.remove();
    vpContainer.remove();
    vi.restoreAllMocks();
  });

  it("hides the center crosshair for the eraser tool", () => {
    const [activeTool] = createSignal("eraser");
    const [zoom] = createSignal(1);
    const [eraserSize] = createSignal(40);

    mockUseEditor({
      activeTool,
      setActiveTool: () => {},
      zoom,
      camera: new ViewportCamera(),
      brushSize: () => 24,
      brushHardness: () => 0.5,
      brushOpacity: () => 1,
      eraserSize,
      eraserHardness: () => 0,
      eraserOpacity: () => 1,
    });

    const root = document.createElement("svg");
    document.body.appendChild(root);
    const dispose = render(() => <BrushCursorOverlay forceVisibleForTest cursorPosForTest={{ x: 10, y: 10 }} />, root);

    expect(root.querySelector("[data-paint-cursor-outer]")).not.toBeNull();
    expect(root.querySelector("[data-paint-cursor-crosshair]")).toBeNull();

    dispose();
    root.remove();
    vi.restoreAllMocks();
  });

  it("shows the center crosshair for the brush tool", () => {
    const [activeTool] = createSignal("brush");
    const [zoom] = createSignal(1);
    const [brushSize] = createSignal(24);

    mockUseEditor({
      activeTool,
      setActiveTool: () => {},
      zoom,
      camera: new ViewportCamera(),
      brushSize,
      brushHardness: () => 0.97,
      brushOpacity: () => 1,
      eraserSize: () => 40,
      eraserHardness: () => 1,
      eraserOpacity: () => 1,
    });

    const root = document.createElement("svg");
    document.body.appendChild(root);
    const dispose = render(() => <BrushCursorOverlay forceVisibleForTest cursorPosForTest={{ x: 10, y: 10 }} />, root);

    expect(root.querySelector("[data-paint-cursor-crosshair]")).not.toBeNull();

    dispose();
    root.remove();
    vi.restoreAllMocks();
  });
});
