import { Show, createEffect, createSignal, on } from "solid-js";
import { Portal } from "solid-js/web";
import { useEditor } from "../shell/EditorContext";
import { For } from "solid-js";
import { Icon } from "../icons";
import { Tooltip } from "../Tooltip";
import { DesktopDialog, DesktopDialogButton, desktopDialogFieldClass } from "./DesktopDialog";
import { getEffectiveMaxDim } from "@/engine/types";
import { showToast } from "../Toast";
import { type Unit, UNITS, formatUnit, unitToPx, pxToUnit } from "@/lib/units";
import { useI18n } from "@/i18n/I18nProvider";
import { isFacadeEnabled, isNativeAuthority } from "@/lib/protocol/facadeRegistry";
import { routeResizeCanvas } from "../canvasRouting";

export function ResizeCanvasModal() {
  const { t } = useI18n();
  const {
    showResizeDialog,
    setShowResizeDialog,
    workspace,
    renderer,
    scheduler,
    docWidth,
    docHeight,
    viewportWidth,
    viewportHeight,
    syncViewport,
  } = useEditor();

  // Always stored in pixels
  const [wPx, setWPx] = createSignal(800);
  const [hPx, setHPx] = createSignal(600);
  const [unit, setUnit] = createSignal<Unit>("px");
  const [aspectLocked, setAspectLocked] = createSignal(true);
  const [aspect, setAspect] = createSignal(1);

  let wRef!: HTMLInputElement;
  let hRef!: HTMLInputElement;

  // Sync DOM display when unit or px values change
  createEffect(on([unit, wPx], () => {
    if (wRef) wRef.value = formatUnit(wPx(), unit());
  }));
  createEffect(on([unit, hPx], () => {
    if (hRef) hRef.value = formatUnit(hPx(), unit());
  }));

  const openDialog = () => {
    const cw = docWidth();
    const ch = docHeight();
    setWPx(cw);
    setHPx(ch);
    setAspect(cw / ch);
    setAspectLocked(true);
    setUnit("px");
  };

  createEffect(on(showResizeDialog, (visible) => {
    if (visible) openDialog();
  }));

  const commitW = () => {
    const raw = parseFloat(wRef.value);
    if (isNaN(raw) || raw <= 0) return;
    const max = getEffectiveMaxDim();
    const nw = unitToPx(raw, unit());
    if (nw > max) {
      showToast(`Maximum dimension is ${max}px`, "warn");
      setWPx(max);
      if (aspectLocked()) setHPx(Math.max(1, Math.round(max / aspect())));
      return;
    }
    setWPx(nw);
    if (aspectLocked()) {
      setHPx(Math.max(1, Math.round(nw / aspect())));
    }
  };

  const commitH = () => {
    const raw = parseFloat(hRef.value);
    if (isNaN(raw) || raw <= 0) return;
    const max = getEffectiveMaxDim();
    const nh = unitToPx(raw, unit());
    if (nh > max) {
      showToast(`Maximum dimension is ${max}px`, "warn");
      setHPx(max);
      if (aspectLocked()) setWPx(Math.max(1, Math.round(max * aspect())));
      return;
    }
    setHPx(nh);
    if (aspectLocked()) {
      setWPx(Math.max(1, Math.round(nh * aspect())));
    }
  };

  const handleApply = async () => {
    const newW = wPx();
    const newH = hPx();
    if (newW < 1 || newH < 1) return;
    const max = getEffectiveMaxDim();
    if (newW > max || newH > max) {
      showToast(`Maximum dimension is ${max}px`, "error");
      return;
    }

    const engine = workspace.getActiveEngine();
    const history = workspace.getActiveHistory();
    if (!engine || !history) return;

    if (newW === docWidth() && newH === docHeight()) {
      setShowResizeDialog(false);
      return;
    }

    // Routed path: the native resize owns the document size and its undo entry,
    // so there is no TS history commit. The route re-uploads layer textures; the
    // viewport resize stays here. "legacy" falls through to the default path.
    if (isFacadeEnabled() && isNativeAuthority()) {
      const status = await routeResizeCanvas(engine, history, renderer, scheduler, newW, newH);
      if (status === "error") {
        showToast("Could not resize canvas", "error");
        return;
      }
      if (status === "applied") {
        const dpr = window.devicePixelRatio || 1;
        renderer.resizeToViewport(viewportWidth(), viewportHeight(), dpr);
        syncViewport();
        setShowResizeDialog(false);
        return;
      }
    }

    // Default path (flag off, or the route deferred to legacy) — unchanged.
    history.commit(engine.snapshot(), "Resize Canvas");
    engine.resizeCanvas(newW, newH);

    const dpr = window.devicePixelRatio || 1;
    renderer.resizeToViewport(viewportWidth(), viewportHeight(), dpr);

    for (const layer of engine.getLayers()) {
      if (layer.imageBitmap) {
        renderer.uploadImage(layer.id, layer.imageBitmap);
      }
    }

    scheduler.requestRender();
    syncViewport();
    setShowResizeDialog(false);
  };

  const handleCancel = () => {
    setShowResizeDialog(false);
  };

  return (
    <Show when={showResizeDialog()}>
      <Portal mount={document.body}>
        <DesktopDialog
          title={t("dialogs.resizeCanvas.title", "Resize Canvas")}
          kind="resize-canvas"
          manageFocus
          onDismiss={handleCancel}
          actions={<>
            <DesktopDialogButton onClick={handleCancel}>{t("common.cancel", "Cancel")}</DesktopDialogButton>
            <DesktopDialogButton variant="primary" onClick={handleApply}>{t("dialogs.resizeCanvas.resize", "Resize")}</DesktopDialogButton>
          </>}
        >
          <div class="flex flex-col gap-2.5">
            <p class="mb-1 text-[11px] text-editor-text-dim">
              {t("dialogs.resizeCanvas.description", "Set the canvas dimensions. Existing layer content keeps its top-left position.")}
            </p>
            <div class="flex items-center gap-2">
              <label for="resize-canvas-width" class="w-[52px] text-[11px] font-medium text-editor-text-dim">{t("dialogs.resizeCanvas.width", "Width")}</label>
              <input
                id="resize-canvas-width"
                ref={wRef!}
                type="number"
                data-dialog-initial-focus
                class={`${desktopDialogFieldClass} flex-1 min-w-0 [appearance:textfield] [&::-webkit-inner-spin-button]:appearance-none [&::-webkit-outer-spin-button]:appearance-none`}
                value={formatUnit(wPx(), unit())}
                onBlur={commitW}
                onKeyDown={(e) => { if (e.key === "Enter") commitW(); }}
                min="0.01"
                step={unit() === "px" ? "1" : "0.01"}
              />
              <div class="w-[60px] shrink-0">
                <select
                  class={`${desktopDialogFieldClass} text-center`}
                  value={unit()}
                  onChange={(e) => setUnit(e.currentTarget.value as Unit)}
                >
                  <For each={UNITS}>{(u) => <option value={u}>{u}</option>}</For>
                </select>
              </div>
            </div>

            <div class="flex h-6 items-center pl-[52px]">
              <Tooltip content={aspectLocked() ? t("dialogs.resizeCanvas.unlockAspect", "Unlock aspect ratio") : t("dialogs.resizeCanvas.lockAspect", "Lock aspect ratio")}>
                <button
                  type="button"
                  onClick={() => setAspectLocked(!aspectLocked())}
                  aria-pressed={aspectLocked()}
                  class={`flex h-6 items-center gap-1.5 rounded-[6px] px-1.5 text-[11px] outline-none transition-all duration-75 focus-visible:outline focus-visible:outline-1 focus-visible:outline-offset-2 focus-visible:outline-editor-accent/70 ${
                    aspectLocked()
                      ? "border border-editor-accent/40 bg-editor-accent/10 text-editor-accent shadow-[inset_0_1px_2px_rgba(225,90,23,0.15)]"
                      : "border border-transparent bg-transparent text-editor-text-dim hover:bg-white/[0.05] hover:text-editor-text"
                  }`}
                >
                  <Icon name={aspectLocked() ? "link" : "unlink"} class="size-3.5" strokeWidth={1.75} />
                  {t("dialogs.resizeCanvas.keepProportions", "Keep proportions")}
                </button>
              </Tooltip>
            </div>

            <div class="flex items-center gap-2">
              <label for="resize-canvas-height" class="w-[52px] text-[11px] font-medium text-editor-text-dim">{t("dialogs.resizeCanvas.height", "Height")}</label>
              <input
                id="resize-canvas-height"
                ref={hRef!}
                type="number"
                class={`${desktopDialogFieldClass} flex-1 min-w-0 [appearance:textfield] [&::-webkit-inner-spin-button]:appearance-none [&::-webkit-outer-spin-button]:appearance-none`}
                value={formatUnit(hPx(), unit())}
                onBlur={commitH}
                onKeyDown={(e) => { if (e.key === "Enter") commitH(); }}
                min="0.01"
                step={unit() === "px" ? "1" : "0.01"}
              />
              <div class="w-[60px] shrink-0">
                <select
                  class={`${desktopDialogFieldClass} text-center`}
                  value={unit()}
                  onChange={(e) => setUnit(e.currentTarget.value as Unit)}
                >
                  <For each={UNITS}>{(u) => <option value={u}>{u}</option>}</For>
                </select>
              </div>
            </div>
          </div>
        </DesktopDialog>
      </Portal>
    </Show>
  );
}
