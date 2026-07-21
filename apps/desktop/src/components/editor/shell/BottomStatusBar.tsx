import { Show } from "solid-js";
import { clsx } from "clsx";
import { Icon } from "../icons";
import { useEditor } from "./EditorContext";
import { getPaintToolBlockReason } from "../brushToolState";

const TOOL_DESCRIPTIONS: Record<string, string> = {
  move: "Drag to move layer. Hold Shift for constrained movement.",
  selection: "Click and drag to create rectangular selection.",
  crop: "Click and drag to define crop area. Enter to apply, Esc to cancel.",
  eyedropper: "Click to sample color from canvas.",
  brush: "Click and drag to paint. Hold Alt for eyedropper.",
  eraser: "Click and drag to erase. Hold Alt for eyedropper.",
};

export function BottomStatusBar() {
  const {
    workspace,
    activeTool,
    zoom,
    docWidth,
    docHeight,
    layers,
    activeLayerId,
    selectedLayerId,
    activeDocumentId,
    layerTransformSession,
    rightDockPanel,
    setRightDockPanel,
    setRightDockOpen,
  } = useEditor();

  const activeLayerName = () => {
    const activeId = activeTool() === "move" ? selectedLayerId() : activeLayerId();
    if (!activeId) return "No active layer";
    return layers().find(l => l.id === activeId)?.name || "Layer";
  };

  const getToolDisplayName = () => {
    const tool = activeTool();
    switch (tool) {
      case "move": return "Move Tool";
      case "selection": return "Selection Tool";
      case "crop": return "Crop Tool";
      case "eyedropper": return "Eyedropper Tool";
      case "brush": return "Brush Tool";
      case "eraser": return "Eraser Tool";
      default: return "Select Tool";
    }
  };

  const activeLayer = () => layers().find((layer) => layer.id === activeLayerId()) ?? null;

  const paintBlockReason = () => {
    const layer = activeLayer();
    if (!layer) return "No active layer selected";
    return getPaintToolBlockReason(layer, activeTool() === "eraser");
  };

  const statusText = () => {
    if (activeTool() === "brush" || activeTool() === "eraser") {
      const reason = paintBlockReason();
      if (reason) return reason;
    }
    if (layerTransformSession()) {
      return "Transforming layer. Drag handles to scale/rotate. Hold Shift to constrain aspect ratio.";
    }
    return TOOL_DESCRIPTIONS[activeTool()] || "Ready";
  };

  return (
    <footer class="flex h-[24px] shrink-0 items-center justify-between border-t border-editor-divider bg-editor-panel-bg px-3 text-[10.5px] text-editor-text-dim select-none">
      <div class="flex items-center gap-3">
        <Show when={activeDocumentId()}>
          <span>
            Canvas: <strong class="text-editor-text">{docWidth()} × {docHeight()} px</strong>
          </span>
          <span class="border-l border-editor-divider pl-3">
            Zoom: <strong class="text-editor-text">{Math.round(zoom() * 100)}%</strong>
          </span>
          <span class="border-l border-editor-divider pl-3">
            Active: <strong class="text-editor-text">{getToolDisplayName()}</strong>
          </span>
          <span class="border-l border-editor-divider pl-3">
            <span class="text-editor-text/60">{statusText()}</span>
          </span>
          <span class="border-l border-editor-divider pl-3">
            Selected Layer: <strong class="text-editor-text">{activeLayerName()}</strong>
          </span>
        </Show>
      </div>

      <div class={clsx("flex shrink-0 items-center gap-5", !activeDocumentId() && "opacity-50 pointer-events-none")}>
        <button
          type="button"
          data-status-history-trigger
          aria-pressed={rightDockPanel() === "history"}
          aria-label="Open History tab"
          onClick={() => {
            setRightDockPanel("history");
            setRightDockOpen(true);
          }}
          class={clsx(
            "flex items-center gap-1 hover:text-editor-text transition-colors",
            rightDockPanel() === "history" && "text-editor-accent hover:text-editor-accent"
          )}
        >
          <Icon name="history" class="size-3.5" strokeWidth={1.75} />
          <span>History</span>
        </button>
      </div>
    </footer>
  );
}
