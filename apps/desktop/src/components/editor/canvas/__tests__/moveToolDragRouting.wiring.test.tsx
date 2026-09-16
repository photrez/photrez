// apps/desktop/src/components/editor/canvas/__tests__/moveToolDragRouting.wiring.test.tsx
//
// Wiring contract: with the Move tool active, a press on EMPTY canvas is owned
// by the marquee session (useCanvasMarqueeSelect). It must never fall through
// to the shared viewport/input-handler move branch, which calls
// engine.moveLayerSilent on every pointermove and engine.flushChangeNotification
// on pointer-up.
//
// Unlike pointerToolRouting.test.tsx (which mocks `@/viewport/input-handler` to
// spy on dispatch), this test wires the REAL useCanvasPointerTools, the REAL
// input-handler and the REAL marquee session, then drives a full
// pointerdown -> pointermove x3 -> pointerup gesture. Removing the early return
// in useCanvasPointerTools makes the engine spies fire and this test fails.

import { mockUseEditor } from "@/__tests__/mockUseEditor";
import { describe, it, expect, vi, afterEach } from "vitest";
import { createRoot, createSignal } from "solid-js";
import { createMockEditorParams, makePointerEvent } from "@/__tests__/pointerRoutingHarness";
import { useCanvasPointerTools } from "../useCanvasPointerTools";
import { useCanvasMarqueeSelect } from "../useCanvasMarqueeSelect";

vi.mock("../../dialogs/DialogProvider", () => ({
  useDialog: () => ({ confirm: vi.fn().mockResolvedValue(false) }),
}));

describe("move tool drag on empty canvas is owned by the marquee session", () => {
  afterEach(() => vi.restoreAllMocks());

  it("never reaches the input-handler move branch (no moveLayerSilent / flushChangeNotification)", async () => {
    const { signals, mockEngine, dispose } = createMockEditorParams("move");
    // A selected, movable layer exists. The input-handler move branch WOULD run
    // for it (layer-1 is unlocked and not the background), so the zero-call
    // assertions below are meaningful rather than vacuous.
    signals.selectedLayerId = createSignal<string | null>("layer-1")[0];
    signals.selectedLayerIds = createSignal<string[]>(["layer-1"])[0];
    signals.setSelectedLayerIds = vi.fn();

    const moveLayerSilent = vi.fn();
    const flushChangeNotification = vi.fn();
    (mockEngine as any).moveLayerSilent = moveLayerSilent;
    (mockEngine as any).flushChangeNotification = flushChangeNotification;

    mockUseEditor(signals);

    const container = document.createElement("div");
    const { tools, marquee, dispose: disposeTools } = createRoot((rootDispose) => {
      const marquee = useCanvasMarqueeSelect({ isSpacePressed: () => false, isPanning: () => false });
      const tools = useCanvasPointerTools({
        getCanvasContainerRef: () => container,
        getCanvasRef: () => document.createElement("canvas"),
        isSpacePressed: () => false,
        isPanning: () => false,
        isAltPressed: () => false,
        stopMomentum: vi.fn(),
        fitToScreenAndRender: vi.fn(),
        commitBrushStroke: vi.fn(),
        // Production wiring (CanvasViewport.tsx): the press is handed to the
        // marquee session, which returns true when it takes the gesture.
        onStartMarquee: (e) => marquee.handlePointerDown(e, container),
      });
      return { tools, marquee, dispose: rootDispose };
    });

    tools.onCanvasPointerDown(makePointerEvent({ clientX: 10, clientY: 10 }));

    // Empty canvas: no layer hit, so the marquee took the press and the shared
    // dispatch was skipped. The threshold is not exceeded yet.
    expect(marquee.isMarqueeActive()).toBe(false);

    for (let i = 1; i <= 3; i++) {
      const clientX = 10 + i * 40;
      const clientY = 10 + i * 40;
      // The marquee session listens on window; the hook boundary sees the same move.
      window.dispatchEvent(new PointerEvent("pointermove", { clientX, clientY }));
      tools.onCanvasPointerMove(makePointerEvent({ clientX, clientY }));
    }

    expect(marquee.isMarqueeActive()).toBe(true);
    expect(marquee.marqueeRect()).not.toBeNull();

    window.dispatchEvent(new PointerEvent("pointerup", { clientX: 130, clientY: 130 }));
    await tools.onCanvasPointerUp(makePointerEvent({ clientX: 130, clientY: 130 }));

    expect(marquee.isMarqueeActive()).toBe(false);

    // The input-handler move branch never ran end to end.
    expect(moveLayerSilent).not.toHaveBeenCalled();
    expect(flushChangeNotification).not.toHaveBeenCalled();

    disposeTools();
    dispose();
  });
});
