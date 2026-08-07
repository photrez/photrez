import { Show, createMemo, createSignal, onCleanup, onMount } from "solid-js";
import { clsx } from "clsx";
import { Icon } from "../icons";
import { useEditor } from "./EditorContext";
import { getPaintToolBlockReason } from "../brushToolState";
import { autosaveStatus, autosaveError, autosaveTimestamp } from "../autoSave";
import { saveProgress } from "../saveState";
import type { FrameMetrics } from "@/renderer/scheduler";

const TOOL_DESCRIPTIONS: Record<string, string> = {
  move: "Drag to move layer. Hold Shift for constrained movement.",
  selection: "Click and drag to create rectangular selection.",
  crop: "Click and drag to define crop area. Enter to apply, Esc to cancel.",
  eyedropper: "Click to sample color from canvas.",
  brush: "Click and drag to paint. Hold Alt for eyedropper.",
  eraser: "Click and drag to erase. Hold Alt for eyedropper.",
  paintBucket: "Click to flood fill matching pixels with foreground color.",
  gradient: "Click and drag to apply linear or radial gradient. Hold Shift for 45° angle lock.",
  shape: "Shape Tool",
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
    gradientDragLine,
    scheduler,
  } = useEditor();

  // ── Dev-mode frame timing (avg render ms per 2s window) ──
  // Production builds skip this entirely (import.meta.env.DEV is false).
  const [frameStats, setFrameStats] = createSignal<FrameMetrics | null>(null);
  onMount(() => {
    if (!import.meta.env.DEV) return;
    const timer = setInterval(() => {
      setFrameStats(scheduler.getFrameMetrics());
      scheduler.resetFrameMetrics();
    }, 2000);
    onCleanup(() => clearInterval(timer));
  });
  const devFrameStats = () => (import.meta.env.DEV ? frameStats() : null);

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
      case "paintBucket": return "Paint Bucket Tool";
      case "gradient": return "Gradient Tool";
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
    if (gradientDragLine()) {
      const g = gradientDragLine()!;
      return `Gradient vector: ${g.distance} px, ${g.angle}° (Hold Shift for 45° angle lock)`;
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
          {/* Dev-mode render timing — production builds keep the bar clean */}
          <Show when={devFrameStats() && devFrameStats()!.frames > 0}>
            <span class="border-l border-editor-divider pl-3">
              Frame: <strong class="text-editor-text">{devFrameStats()!.avgMs.toFixed(1)}ms avg</strong>
              <span class="text-editor-text/40"> max {devFrameStats()!.maxMs.toFixed(1)}ms</span>
            </span>
          </Show>
          {/* Autosave status indicator */}
          <Show when={autosaveStatus() !== "idle"}>
            <span class="border-l border-editor-divider pl-3 flex items-center gap-1">
              <Show when={autosaveStatus() === "saving"}>
                <span class="inline-block size-2 rounded-full bg-yellow-400 animate-pulse" />
                <span class="text-editor-text/60">Saving…</span>
              </Show>
              <Show when={autosaveStatus() === "saved"}>
                <span class="inline-block size-2 rounded-full bg-green-400" />
                <span class="text-editor-text/60">Saved</span>
              </Show>
              <Show when={autosaveStatus() === "error"}>
                <span class="inline-block size-2 rounded-full bg-red-400" />
                <span class="text-red-400" title={autosaveError() ?? ""}>Save failed</span>
              </Show>
            </span>
          </Show>
          {/* Manual save progress indicator — replaces old blocking overlay */}
          <Show when={saveProgress().phase !== "idle"}>
            <span class="border-l border-editor-divider pl-3 flex items-center gap-1">
              <Show when={saveProgress().phase === "encoding" || saveProgress().phase === "writing"}>
                <span class="inline-block size-2 rounded-full bg-yellow-400 animate-pulse" />
                <span class="text-editor-text/60">{saveProgress().label}</span>
                <Show when={saveProgress().cancel}>
                  <button
                    type="button"
                    onClick={() => saveProgress().cancel?.()}
                    class="ml-0.5 text-editor-text-dim hover:text-editor-text transition-colors"
                    title="Cancel save"
                    aria-label="Cancel save"
                  >
                    <Icon name="x" class="size-3" />
                  </button>
                </Show>
              </Show>
              <Show when={saveProgress().phase === "done"}>
                <span class="inline-block size-2 rounded-full bg-green-400" />
                <span class="text-editor-text/60">Saved</span>
              </Show>
              <Show when={saveProgress().phase === "error" || saveProgress().phase === "cancelled"}>
                <span class="inline-block size-2 rounded-full bg-red-400" />
                <span class="text-red-400">{saveProgress().phase === "cancelled" ? "Cancelled" : "Save failed"}</span>
              </Show>
            </span>
          </Show>
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
