import { describe, expect, it } from "vitest";
import type { LayerTransformSession } from "../editorState";
import type { ToolCleanupContext } from "../toolLifecycle";
import { runToolSwitchCleanup, TOOL_CLEANUP_HANDLERS } from "../toolLifecycle";

const TOOL_IDS = ["move", "selection", "crop", "eyedropper", "brush", "eraser", "paintBucket", "gradient", "shape", "text"] as const;

function createCleanupContext() {
  const calls = {
    hoverHandle: [] as Array<string | null>,
    hoverPos: [] as Array<{ x: number; y: number } | null>,
    layerTransformSession: [] as Array<LayerTransformSession | null>,
    selectionEditMode: [] as boolean[],
  };

  const context: ToolCleanupContext = {
    setHoverHandle(value) {
      const next = typeof value === "function" ? value("nw") : value;
      calls.hoverHandle.push(next);
      return next;
    },
    setHoverPos(value) {
      const next = typeof value === "function" ? value({ x: 10, y: 20 }) : value;
      calls.hoverPos.push(next);
      return next;
    },
    setLayerTransformSession(value) {
      const next = typeof value === "function" ? value(null) : value;
      calls.layerTransformSession.push(next);
      return next;
    },
    setSelectionEditMode(value) {
      const next = typeof value === "function" ? value(true) : value;
      calls.selectionEditMode.push(next);
      return next;
    },
  };

  return { calls, context };
}

describe("tool cleanup lifecycle registry", () => {
  it("declares cleanup handlers for every ToolId", () => {
    expect(Object.keys(TOOL_CLEANUP_HANDLERS).sort()).toEqual([...TOOL_IDS].sort());
  });

  it("clears shared transient state when switching away from a tool", () => {
    const { calls, context } = createCleanupContext();

    runToolSwitchCleanup("move", "crop", context);

    expect(calls.hoverHandle).toEqual([null]);
    expect(calls.hoverPos).toEqual([null]);
    expect(calls.layerTransformSession).toEqual([null]);
    expect(calls.selectionEditMode).toEqual([false]);
  });

  it("does nothing when the active tool does not change", () => {
    const { calls, context } = createCleanupContext();

    runToolSwitchCleanup("move", "move", context);

    expect(calls.hoverHandle).toEqual([]);
    expect(calls.hoverPos).toEqual([]);
    expect(calls.layerTransformSession).toEqual([]);
    expect(calls.selectionEditMode).toEqual([]);
  });
});
