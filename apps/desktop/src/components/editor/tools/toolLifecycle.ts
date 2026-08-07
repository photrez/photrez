import { batch } from "solid-js";
import type { Setter } from "solid-js";
import type { LayerTransformSession } from "./editorState";
import type { ToolId } from "./toolTypes";

export interface ToolCleanupContext {
  setHoverHandle: Setter<string | null>;
  setHoverPos: Setter<{ x: number; y: number } | null>;
  setLayerTransformSession: Setter<LayerTransformSession | null>;
  setSelectionEditMode: Setter<boolean>;
}

type ToolCleanupHandler = (context: ToolCleanupContext) => void;

function clearSharedTransientState(context: ToolCleanupContext): void {
  context.setHoverHandle(null);
  context.setHoverPos(null);
  context.setLayerTransformSession(null);
  context.setSelectionEditMode(false);
}

export const TOOL_CLEANUP_HANDLERS = {
  move: [clearSharedTransientState],
  selection: [clearSharedTransientState],
  crop: [clearSharedTransientState],
  eyedropper: [clearSharedTransientState],
  brush: [clearSharedTransientState],
  eraser: [clearSharedTransientState],
  paintBucket: [clearSharedTransientState],
  gradient: [clearSharedTransientState],
  shape: [clearSharedTransientState],
} satisfies Record<ToolId, readonly ToolCleanupHandler[]>;

export function runToolSwitchCleanup(
  previousTool: ToolId,
  nextTool: ToolId,
  context: ToolCleanupContext,
): void {
  if (previousTool === nextTool) return;

  batch(() => {
    const handlers = TOOL_CLEANUP_HANDLERS[previousTool] ?? [clearSharedTransientState];
    for (const cleanup of handlers) {
      cleanup(context);
    }
  });
}
