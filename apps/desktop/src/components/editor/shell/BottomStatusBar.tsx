import { Show, createMemo, createSignal, onCleanup, onMount } from "solid-js";
import { clsx } from "clsx";
import { Icon } from "../icons";
import { useEditor } from "./EditorContext";
import { useI18n } from "@/i18n/I18nProvider";
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
  text: "Click to type, drag for a text box. Ctrl+Enter to commit.",
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
    selectedLayerIds,
    activeDocumentId,
    layerTransformSession,
    rightDockPanel,
    setRightDockPanel,
    setRightDockOpen,
    gradientDragLine,
    scheduler,
  } = useEditor();
  const { t } = useI18n();

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
    if (!activeId) return t("status.noActiveLayer");
    return layers().find(l => l.id === activeId)?.name || "Layer";
  };

  const getToolDisplayName = () => {
    const tool = activeTool();
    switch (tool) {
      case "move": return t("tools.move", "Move Tool");
      case "selection": return t("tools.rectSelect", "Selection Tool");
      case "crop": return t("tools.crop", "Crop Tool");
      case "eyedropper": return t("tools.eyedropper", "Eyedropper Tool");
      case "brush": return t("tools.brush", "Brush Tool");
      case "eraser": return t("tools.eraser", "Eraser Tool");
      case "paintBucket": return t("tools.paintBucket", "Paint Bucket Tool");
      case "gradient": return t("tools.gradient", "Gradient Tool");
      case "shape": return t("tools.shape", "Shape Tool");
      case "text": return t("tools.text", "Text Tool");
      default: return t("tools.select", "Select Tool");
    }
  };

  const activeLayer = () => layers().find((layer) => layer.id === activeLayerId()) ?? null;

  const paintBlockReason = () => {
    const layer = activeLayer();
    if (!layer) return t("status.toolTips.noActiveLayer", "No active layer selected");
    if (layer.locked) return t("status.toolTips.layerLocked", "Layer locked");
    if (!layer.visible) return t("status.toolTips.layerHidden", "Layer hidden");
    if (activeTool() === "eraser" && layer.lockTransparency) return t("status.toolTips.transparencyProtected", "Transparent pixels protected");
    return null;
  };

  const statusText = () => {
    if (activeTool() === "brush" || activeTool() === "eraser") {
      const reason = paintBlockReason();
      if (reason) return reason;
    }
    if (gradientDragLine()) {
      const g = gradientDragLine()!;
      return t("status.gradientVector", { distance: g.distance, angle: g.angle });
    }
    if (layerTransformSession()) {
      return t("status.toolTips.transforming", "Transforming layer. Drag handles to scale/rotate. Hold Shift to constrain aspect ratio.");
    }
    const tipKey = `status.toolTips.${activeTool()}`;
    return t(tipKey, TOOL_DESCRIPTIONS[activeTool()] || t("status.ready"));
  };

  return (
    <footer class="flex h-[24px] shrink-0 items-center justify-between border-t border-editor-divider bg-editor-panel-bg px-3 text-[10.5px] text-editor-text-dim select-none">
      <div class="flex items-center gap-3">
        <Show when={activeDocumentId()}>
          <span>
            {t("status.canvas")}: <strong class="text-editor-text">{docWidth()} × {docHeight()} px</strong>
          </span>
          <span class="border-l border-editor-divider pl-3">
            {t("status.zoom")}: <strong class="text-editor-text">{Math.round(zoom() * 100)}%</strong>
          </span>
          <span class="border-l border-editor-divider pl-3">
            {t("status.active")}: <strong class="text-editor-text">{getToolDisplayName()}</strong>
          </span>
          <span class="border-l border-editor-divider pl-3">
            <span class="text-editor-text/60">{statusText()}</span>
          </span>
          <span class="border-l border-editor-divider pl-3">
            <Show
              when={typeof selectedLayerIds === "function" && selectedLayerIds().length > 1}
              fallback={<>{t("status.selectedLayer")}: <strong class="text-editor-text">{activeLayerName()}</strong></>}
            >
              {t("status.selectedLayers", { count: selectedLayerIds().length })}
            </Show>
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
                <span class="text-editor-text/60">{t("common.saving")}</span>
              </Show>
              <Show when={autosaveStatus() === "saved"}>
                <span class="inline-block size-2 rounded-full bg-green-400" />
                <span class="text-editor-text/60">{t("common.saved")}</span>
              </Show>
              <Show when={autosaveStatus() === "error"}>
                <span class="inline-block size-2 rounded-full bg-red-400" />
                <span class="text-red-400" title={autosaveError() ?? ""}>{t("common.saveFailed")}</span>
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
                    title={t("status.cancelSave", "Cancel save")}
                    aria-label={t("status.cancelSave", "Cancel save")}
                  >
                    <Icon name="x" class="size-3" />
                  </button>
                </Show>
              </Show>
              <Show when={saveProgress().phase === "done"}>
                <span class="inline-block size-2 rounded-full bg-green-400" />
                <span class="text-editor-text/60">{t("common.saved")}</span>
              </Show>
              <Show when={saveProgress().phase === "error" || saveProgress().phase === "cancelled"}>
                <span class="inline-block size-2 rounded-full bg-red-400" />
                <span class="text-red-400">{saveProgress().phase === "cancelled" ? t("common.cancelled") : t("common.saveFailed")}</span>
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
          <span>{t("status.history")}</span>
        </button>
      </div>
    </footer>
  );
}
