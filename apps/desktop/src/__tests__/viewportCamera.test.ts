import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { ViewportCamera, MIN_ZOOM, MAX_ZOOM } from "../viewport/viewportCamera";
import { easeOutCubic, linear } from "../viewport/easing";
import { screenToDocument } from "../viewport/coords";

describe("ViewportCamera", () => {
  let camera: ViewportCamera;

  beforeEach(() => {
    camera = new ViewportCamera();
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("initializes with default values", () => {
    const state = camera.getState();
    expect(state.x).toBe(0);
    expect(state.y).toBe(0);
    expect(state.zoom).toBe(1.0);
  });

  it("can set and get state", () => {
    camera.setState({ x: 100, y: 200, zoom: 2.0 });
    const state = camera.getState();
    expect(state.x).toBe(100);
    expect(state.y).toBe(200);
    expect(state.zoom).toBe(2.0);
  });

  it("clamps zoom to limits in setState", () => {
    camera.setState({ x: 0, y: 0, zoom: MIN_ZOOM - 0.005 });
    expect(camera.getState().zoom).toBe(MIN_ZOOM);

    camera.setState({ x: 0, y: 0, zoom: MAX_ZOOM + 10 });
    expect(camera.getState().zoom).toBe(MAX_ZOOM);
  });

  it("pans correctly", () => {
    camera.pan(50, -30);
    expect(camera.getState()).toEqual({ x: 50, y: -30, zoom: 1.0 });
  });

  it("converts screen to document and back correctly", () => {
    camera.setState({ x: 100, y: 200, zoom: 2.5 });

    // screenToDocument: (screen - pan) / zoom
    // (350 - 100) / 2.5 = 100
    // (700 - 200) / 2.5 = 200
    const doc = camera.screenToDocument(350, 700);
    expect(doc.x).toBeCloseTo(100);
    expect(doc.y).toBeCloseTo(200);

    const screen = camera.documentToScreen(100, 200);
    expect(screen.x).toBeCloseTo(350);
    expect(screen.y).toBeCloseTo(700);
  });

  it("round-trips document and screen coordinates after pan and zoom", () => {
    camera.setState({ x: 86, y: 124, zoom: 0.6 });

    const screen = camera.documentToScreen(472, 709);
    const doc = camera.screenToDocument(screen.x, screen.y);

    expect(doc.x).toBeCloseTo(472, 4);
    expect(doc.y).toBeCloseTo(709, 4);
  });

  it("zoomToPoint keeps anchor point stable", () => {
    camera.setState({ x: 50, y: 50, zoom: 1.0 });

    const screenX = 200;
    const screenY = 150;

    // Document coordinate under pointer before zoom
    const docBefore = camera.screenToDocument(screenX, screenY);

    camera.zoomToPoint(1.5, screenX, screenY);

    // Document coordinate under pointer after zoom must be identical
    const docAfter = camera.screenToDocument(screenX, screenY);

    expect(docAfter.x).toBeCloseTo(docBefore.x);
    expect(docAfter.y).toBeCloseTo(docBefore.y);
    expect(camera.getState().zoom).toBe(1.5);
  });

  it("getViewProjectionMatrix returns correct ortho matrix", () => {
    camera.setState({ x: 100, y: 50, zoom: 2.0 });

    const matrix = camera.getViewProjectionMatrix(800, 600);
    expect(matrix).toHaveLength(16);

    // m[0] = 2 * zoom / w = 4 / 800 = 0.005
    expect(matrix[0]).toBeCloseTo(0.005);
    // m[5] = -2 * zoom / h = -4 / 600 = -0.0066666
    expect(matrix[5]).toBeCloseTo(-0.0066666);
    // m[10] = 1
    expect(matrix[10]).toBe(1);
    // m[12] = -1 + 2 * x / w = -1 + 200 / 800 = -0.75
    expect(matrix[12]).toBeCloseTo(-0.75);
    // m[13] = 1 - 2 * y / h = 1 - 100 / 600 = 0.833333
    expect(matrix[13]).toBeCloseTo(0.833333);
  });

  it("interpolates animation values smoothly", () => {
    camera.setState({ x: 0, y: 0, zoom: 1.0 });

    const target = { x: 100, y: 200, zoom: 2.0 };
    const start = performance.now();

    const startCallback = vi.fn();
    const endCallback = vi.fn();
    camera.onAnimationStart = startCallback;
    camera.onAnimationEnd = endCallback;

    camera.animateTo(target, 200, linear);

    expect(camera.isAnimating()).toBe(true);
    expect(startCallback).toHaveBeenCalledTimes(1);

    // Tick at 50% time
    vi.advanceTimersByTime(100);
    camera.tick(start + 100);

    expect(camera.getState().x).toBeCloseTo(50);
    expect(camera.getState().y).toBeCloseTo(100);
    expect(camera.getState().zoom).toBeCloseTo(1.5);
    expect(camera.isAnimating()).toBe(true);

    // Tick at 100% time
    vi.advanceTimersByTime(100);
    camera.tick(start + 200);

    expect(camera.getState().x).toBe(100);
    expect(camera.getState().y).toBe(200);
    expect(camera.getState().zoom).toBe(2.0);
    expect(camera.isAnimating()).toBe(false);
    expect(endCallback).toHaveBeenCalledTimes(1);
  });

  it("animateZoomToPoint zoom targets point correctly", () => {
    camera.setState({ x: 50, y: 50, zoom: 1.0 });

    const screenX = 200;
    const screenY = 150;
    const docBefore = camera.screenToDocument(screenX, screenY);

    camera.animateZoomToPoint(2.0, screenX, screenY, 150, easeOutCubic);

    const start = performance.now();
    // Complete animation immediately
    vi.advanceTimersByTime(150);
    camera.tick(start + 150);

    expect(camera.isAnimating()).toBe(false);
    expect(camera.getState().zoom).toBe(2.0);

    const docAfter = camera.screenToDocument(screenX, screenY);
    expect(docAfter.x).toBeCloseTo(docBefore.x);
    expect(docAfter.y).toBeCloseTo(docBefore.y);
  });

  it("setImageTransform stores state retrievable via getImageTransform", () => {
    camera.setImageTransform({
      offsetX: 10,
      offsetY: 20,
      rotation: 30,
      scale: 1.5,
      pivotScreen: { x: 100, y: 200 },
      pivotDocument: { x: 50, y: 60 },
    });

    const t = camera.getImageTransform();
    expect(t.offsetX).toBe(10);
    expect(t.offsetY).toBe(20);
    expect(t.rotation).toBe(30);
    expect(t.scale).toBe(1.5);
    expect(t.pivotScreen).toEqual({ x: 100, y: 200 });
    expect(t.pivotDocument).toEqual({ x: 50, y: 60 });
  });

  it("getImageTransform returns a copy (mutating result does not affect camera)", () => {
    camera.setImageTransform({ offsetX: 5 });
    const t = camera.getImageTransform();
    t.offsetX = 999;
    expect(camera.getImageTransform().offsetX).toBe(5);
  });

  it("setImageTransform defaults unspecified fields to identity", () => {
    camera.setImageTransform({ offsetX: 10 });

    const t = camera.getImageTransform();
    expect(t.offsetX).toBe(10);
    expect(t.offsetY).toBe(0);
    expect(t.rotation).toBe(0);
    expect(t.scale).toBe(1.0);
    expect(t.pivotScreen).toBeNull();
    expect(t.pivotDocument).toBeNull();
  });

  it("resetImageTransform returns state to identity (default)", () => {
    camera.setImageTransform({
      offsetX: 10,
      offsetY: 20,
      rotation: 30,
      scale: 1.5,
      pivotScreen: { x: 100, y: 200 },
      pivotDocument: { x: 50, y: 60 },
    });
    camera.resetImageTransform();

    const t = camera.getImageTransform();
    expect(t.offsetX).toBe(0);
    expect(t.offsetY).toBe(0);
    expect(t.rotation).toBe(0);
    expect(t.scale).toBe(1.0);
    expect(t.pivotScreen).toBeNull();
    expect(t.pivotDocument).toBeNull();
  });

  it("getViewProjectionMatrix with identity image transform matches camera-only matrix", () => {
    camera.setState({ x: 100, y: 50, zoom: 2.0 });

    const m1 = camera.getViewProjectionMatrix(800, 600);

    camera.setImageTransform({});
    const m2 = camera.getViewProjectionMatrix(800, 600);

    expect(m2).toEqual(m1);
  });

  it("getViewProjectionMatrix with null pivots matches camera-only matrix", () => {
    camera.setState({ x: 100, y: 50, zoom: 2.0 });

    const m1 = camera.getViewProjectionMatrix(800, 600);

    camera.setImageTransform({
      offsetX: 0,
      offsetY: 0,
      rotation: 0,
      scale: 1.0,
      pivotScreen: null,
      pivotDocument: null,
    });
    const m2 = camera.getViewProjectionMatrix(800, 600);

    expect(m2).toEqual(m1);
  });

  it("getViewProjectionMatrix with image transform offset shifts matrix translation by offset in screen pixels", () => {
    camera.setState({ x: 0, y: 0, zoom: 1.0 });

    const m1 = camera.getViewProjectionMatrix(800, 600);
    expect(m1[12]).toBeCloseTo(-1, 5);

    camera.setImageTransform({
      offsetX: 50,
      offsetY: 0,
      pivotScreen: null,
      pivotDocument: null,
    });
    const m2 = camera.getViewProjectionMatrix(800, 600);

    expect(m2[12]).toBeCloseTo(-0.875, 5);
    expect(m2[13]).toBeCloseTo(m1[13], 5);
  });

  it("getViewProjectionMatrix with image transform scale + pivot: pivot maps to NDC center, scaled point maps correctly", () => {
    camera.setState({ x: 0, y: 0, zoom: 1.0 });
    camera.setImageTransform({
      offsetX: 0,
      offsetY: 0,
      rotation: 0,
      scale: 2.0,
      pivotScreen: { x: 500, y: 500 },
      pivotDocument: { x: 500, y: 500 },
    });

    const m = camera.getViewProjectionMatrix(1000, 1000);
    expect(m[0]).toBeCloseTo(0.004, 5);
    expect(m[1]).toBeCloseTo(0, 5);
    expect(m[4]).toBeCloseTo(0, 5);
    expect(m[5]).toBeCloseTo(-0.004, 5);
    expect(m[12]).toBeCloseTo(-2, 5);
    expect(m[13]).toBeCloseTo(2, 5);

    // Pivot maps to NDC origin
    expect(m[0] * 500 + m[4] * 500 + m[12]).toBeCloseTo(0, 5);
    expect(m[1] * 500 + m[5] * 500 + m[13]).toBeCloseTo(0, 5);

    // Doc (700, 500): 200 right of pivot
    // After scale=2: (400, 0) from pivot, +pivot_screen (500,500) = (900, 500)
    // NDC: (900*2/1000 - 1, -500*2/1000 + 1) = (0.8, 0)
    expect(m[0] * 700 + m[4] * 500 + m[12]).toBeCloseTo(0.8, 5);
    expect(m[1] * 700 + m[5] * 500 + m[13]).toBeCloseTo(0, 5);
  });

  it("getViewProjectionMatrix with image transform rotation + pivot: doc point rotates around pivot", () => {
    camera.setState({ x: 0, y: 0, zoom: 1.0 });
    camera.setImageTransform({
      offsetX: 0,
      offsetY: 0,
      rotation: 90,
      scale: 1.0,
      pivotScreen: { x: 500, y: 500 },
      pivotDocument: { x: 500, y: 500 },
    });

    const m = camera.getViewProjectionMatrix(1000, 1000);

    // cos(90°) = 0, sin(90°) = 1
    expect(m[0]).toBeCloseTo(0, 5);
    expect(m[1]).toBeCloseTo(-0.002, 5);
    expect(m[4]).toBeCloseTo(-0.002, 5);
    expect(m[5]).toBeCloseTo(0, 5);
    expect(m[12]).toBeCloseTo(1, 5);
    expect(m[13]).toBeCloseTo(1, 5);

    // Pivot maps to NDC center
    expect(m[0] * 500 + m[4] * 500 + m[12]).toBeCloseTo(0, 5);
    expect(m[1] * 500 + m[5] * 500 + m[13]).toBeCloseTo(0, 5);

    // Doc (700, 500): 200 right of pivot
    // After T(-pivot_doc): (200, 0)
    // After R(90°): (cos*200 - sin*0, sin*200 + cos*0) = (0, 200)
    // After T(pivot_screen): (500, 700)
    // NDC: (500*2/1000 - 1, -700*2/1000 + 1) = (0, -0.4)
    expect(m[0] * 700 + m[4] * 500 + m[12]).toBeCloseTo(0, 5);
    expect(m[1] * 700 + m[5] * 500 + m[13]).toBeCloseTo(-0.4, 5);
  });

  it("getViewProjectionMatrix with image transform + camera pan: pivotDocument maps to NDC(pivotScreen)", () => {
    // Camera with non-zero pan and non-unity zoom. The pivot must land at
    // its screen position regardless of the camera's pan offset. This is
    // the user-facing invariant: in modern crop, the image's pivot doc
    // point should appear at the frame's center (pivotScreen).
    camera.setState({ x: 100, y: 50, zoom: 0.8 });

    // Pivot at screen (500, 400) in a 800x600 viewport.
    // doc = (screen - pan) / zoom = (400/0.8, 350/0.8) = (500, 437.5)
    camera.setImageTransform({
      offsetX: 0,
      offsetY: 0,
      rotation: 0,
      scale: 1.0,
      pivotScreen: { x: 500, y: 400 },
      pivotDocument: { x: 500, y: 437.5 },
    });

    const m = camera.getViewProjectionMatrix(800, 600);

    // Invariant: M * pivotDocument must equal NDC(pivotScreen).
    const expectedNdcX = (2 * 500) / 800 - 1; // 0.25
    const expectedNdcY = 1 - (2 * 400) / 600; // -1/3
    expect(m[0] * 500 + m[4] * 437.5 + m[12]).toBeCloseTo(expectedNdcX, 5);
    expect(m[1] * 500 + m[5] * 437.5 + m[13]).toBeCloseTo(expectedNdcY, 5);

    // Sanity: a doc point to the right of the pivot lands to the right
    // of the pivot on screen (and 1/0.8 = 1.25x screen units because zoom=0.8
    // means a doc unit is 0.8 screen units away from its origin).
    // doc (700, 437.5) = pivot + (200, 0) → screen (660, 400) → NDC.x = 0.65
    expect(m[0] * 700 + m[4] * 437.5 + m[12]).toBeCloseTo(0.65, 5);
  });

  it("getViewProjectionMatrix with image transform + pan: offsetX/Y do NOT double-shift pivot", () => {
    // offsetX/Y in the pivot mode should not be added on top of pivotScreen
    // (otherwise the image would be displaced relative to the pivot). This
    // test sets non-zero offsetX/Y to verify they are ignored in pivot mode.
    camera.setState({ x: 100, y: 50, zoom: 1.0 });

    camera.setImageTransform({
      offsetX: 20,
      offsetY: 10,
      rotation: 0,
      scale: 1.0,
      pivotScreen: { x: 500, y: 400 },
      pivotDocument: { x: 400, y: 350 },
    });

    const m = camera.getViewProjectionMatrix(800, 600);

    // Pivot must map to NDC(pivotScreen). The presence of offsetX/Y must
    // not shift the pivot.
    const expectedNdcX = (2 * 500) / 800 - 1; // 0.25
    const expectedNdcY = 1 - (2 * 400) / 600; // -1/3
    expect(m[0] * 400 + m[4] * 350 + m[12]).toBeCloseTo(expectedNdcX, 5);
    expect(m[1] * 400 + m[5] * 350 + m[13]).toBeCloseTo(expectedNdcY, 5);
  });

  it("getViewProjectionMatrix with image transform rotation + camera pan: pivotDocument maps to NDC(pivotScreen)", () => {
    // Rotation around a pivot must preserve the pivot's screen position
    // even when the camera has pan. This is the core invariant for any
    // modern crop / rotate-without-moving-center operation.
    camera.setState({ x: 50, y: 30, zoom: 1.0 });

    camera.setImageTransform({
      offsetX: 0,
      offsetY: 0,
      rotation: 45,
      scale: 1.0,
      pivotScreen: { x: 600, y: 400 },
      pivotDocument: { x: 550, y: 370 },
    });

    const m = camera.getViewProjectionMatrix(1000, 600);

    // Pivot at screen (600, 400): NDC = (0.2, -1/3)
    const expectedNdcX = (2 * 600) / 1000 - 1; // 0.2
    const expectedNdcY = 1 - (2 * 400) / 600; // -1/3
    expect(m[0] * 550 + m[4] * 370 + m[12]).toBeCloseTo(expectedNdcX, 5);
    expect(m[1] * 550 + m[5] * 370 + m[13]).toBeCloseTo(expectedNdcY, 5);

    // Other doc points should rotate around the pivot. Doc (700, 370) is
    // 150 to the right of the pivot; after R(45°), it's 150*(cosR, sinR) ≈
    // (106.07, 106.07) from the pivot. Screen pivot + offset = (706.07, 506.07).
    // NDC: (2*706.07/1000 - 1, 1 - 2*506.07/600) ≈ (0.4121, -0.6869)
    const expectedRightNdcX = 2 * 706.066 / 1000 - 1;
    const expectedRightNdcY = 1 - (2 * 506.066) / 600;
    expect(m[0] * 700 + m[4] * 370 + m[12]).toBeCloseTo(expectedRightNdcX, 4);
    expect(m[1] * 700 + m[5] * 370 + m[13]).toBeCloseTo(expectedRightNdcY, 4);
  });

  it("getViewProjectionMatrix composes image transform with camera pan", () => {
    camera.setState({ x: 100, y: 50, zoom: 1.0 });
    camera.setImageTransform({
      offsetX: 20,
      offsetY: 0,
      rotation: 45,
      scale: 1.5,
      pivotScreen: { x: 500, y: 500 },
      pivotDocument: { x: 400, y: 400 },
    });

    const m = camera.getViewProjectionMatrix(1000, 1000);

    // zs = 1 * 1.5 = 1.5
    const expected_m0 = (2 * 1.5 * Math.cos(Math.PI / 4)) / 1000;
    const expected_m1 = -(2 * 1.5 * Math.sin(Math.PI / 4)) / 1000;
    const expected_m4 = -(2 * 1.5 * Math.sin(Math.PI / 4)) / 1000;
    const expected_m5 = -(2 * 1.5 * Math.cos(Math.PI / 4)) / 1000;
    expect(m[0]).toBeCloseTo(expected_m0, 5);
    expect(m[1]).toBeCloseTo(expected_m1, 5);
    expect(m[4]).toBeCloseTo(expected_m4, 5);
    expect(m[5]).toBeCloseTo(expected_m5, 5);

    // m[12] = -1 + (2/1000) * (1.5 * (-cos(45°)*400 + sin(45°)*400) + 500)
    //       = -1 + 0.002 * (1.5 * 0 + 500) = -1 + 1.0 = 0
    // Note: pivotScreen is the authoritative screen position, so the
    // camera pan (x=100) is NOT added on top of it. See the regression
    // tests above for the pivot invariant.
    const expected_m12 = -1 + 0.002 * (1.5 * (-Math.cos(Math.PI / 4) * 400 + Math.sin(Math.PI / 4) * 400) + 500);
    expect(m[12]).toBeCloseTo(expected_m12, 5);

    // m[13] = 1 + (2/1000) * (1.5 * (sin(45°)*400 + cos(45°)*400) - 500)
    //       = 1 + 0.002 * (1.5 * 565.685 - 500) = 1 + 0.002 * 348.528 ≈ 1.697
    // Note: camera pan (y=50) is NOT added on top of pivotScreen.y.
    const expected_m13 = 1 + 0.002 * (1.5 * (Math.sin(Math.PI / 4) * 400 + Math.cos(Math.PI / 4) * 400) - 500);
    expect(m[13]).toBeCloseTo(expected_m13, 5);

    // Pivot invariant: pivotDocument (400, 400) must map to NDC(pivotScreen = (500, 500))
    // = (0, 0). This is what makes the image's rotation center stay at the
    // frame center on screen, regardless of camera pan.
    expect(m[0] * 400 + m[4] * 400 + m[12]).toBeCloseTo(0, 5);
    expect(m[1] * 400 + m[5] * 400 + m[13]).toBeCloseTo(0, 5);
  });

  it("resetImageTransform causes getViewProjectionMatrix to return camera-only matrix", () => {
    camera.setState({ x: 100, y: 50, zoom: 2.0 });

    camera.setImageTransform({
      scale: 1.5,
      rotation: 30,
      pivotScreen: { x: 400, y: 300 },
      pivotDocument: { x: 400, y: 300 },
    });

    camera.resetImageTransform();
    const m = camera.getViewProjectionMatrix(800, 600);

    expect(m[0]).toBeCloseTo(0.005, 5);
    expect(m[12]).toBeCloseTo(-0.75, 5);
  });

  it("reuses the pre-allocated matrix buffer without leaking elements between paths (review #26)", () => {
    camera.setState({ x: 0, y: 0, zoom: 1.0 });

    // Pivot path writes rotation elements (m[1], m[4])…
    camera.setImageTransform({
      offsetX: 0,
      offsetY: 0,
      rotation: Math.PI / 2,
      scale: 1.0,
      pivotScreen: { x: 100, y: 100 },
      pivotDocument: { x: 100, y: 100 },
    });
    const withPivot = camera.getViewProjectionMatrix(800, 600);
    expect(withPivot[1]).not.toBe(0);

    // …then the no-pivot path must not see stale values from the previous call.
    camera.setImageTransform({});
    const withoutPivot = camera.getViewProjectionMatrix(800, 600);
    expect(withoutPivot[1]).toBe(0);
    expect(withoutPivot[4]).toBe(0);
    expect(withoutPivot[14]).toBe(0);
  });
});

describe("coords.screenToDocument vs camera.screenToDocument equivalency", () => {
  it("produces identical results across zoom levels and pan offsets", () => {
    const camera = new ViewportCamera();
    const canvasRect = new DOMRect(50, 30, 1200, 800);

    const scenarios = [
      { x: 0, y: 0, zoom: 1.0 },
      { x: 100, y: 200, zoom: 0.5 },
      { x: -50, y: -100, zoom: 2.0 },
      { x: 300, y: 150, zoom: 0.25 },
    ];

    for (const state of scenarios) {
      camera.setState(state);

      // Test points across the viewport
      const testPoints = [
        { clientX: 100, clientY: 100 },
        { clientX: 400, clientY: 300 },
        { clientX: 800, clientY: 500 },
        { clientX: 600, clientY: 400 },
      ];

      for (const pt of testPoints) {
        const coordsResult = screenToDocument(
          pt.clientX,
          pt.clientY,
          canvasRect,
          { panX: state.x, panY: state.y, zoom: state.zoom, rotation: 0 }
        );

        const cameraResult = camera.screenToDocument(
          pt.clientX - canvasRect.left,
          pt.clientY - canvasRect.top
        );

        expect(coordsResult.x).toBeCloseTo(cameraResult.x, 10);
        expect(coordsResult.y).toBeCloseTo(cameraResult.y, 10);
      }
    }
  });
});
