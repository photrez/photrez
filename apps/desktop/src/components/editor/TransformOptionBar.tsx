import { Show, createSignal } from "solid-js";
import { Icon } from "./icons";
import { NumField, EditableNumField } from "./primitives";
import { clsx } from "clsx";
import { Tooltip } from "./Tooltip";
import { useEditor } from "./shell/EditorContext";
import { ToggleBtn, Divider, ToolPill, MoreDropdown } from "./shell/OptionBarShared";
import { cancelLayerTransformSession, commitLayerTransformSession, resetLayerTransformPreview } from "./transformSession";
import type { Transform2D } from "@/engine/types";
import { isFacadeEnabled, getFacade, setTransformPreview, clearTransformPreview } from "@/lib/protocol/facadeRegistry";
import { isFacadeOwnedLayer } from "@/engine/document";
import { useI18n } from "@/i18n/I18nProvider";

export function TransformOptionBar() {
  const { t } = useI18n();
  const {
    workspace,
    scheduler,
    activeLayerId,
    layerTransformSession,
    setLayerTransformSession,
    constrainRatio,
    setConstrainRatio,
  } = useEditor();

  const [transformTick, setTransformTick] = createSignal(0);

  const session = () => layerTransformSession();
  const engine = () => workspace.getActiveEngine();

  const activeLayer = () => {
    const current = engine();
    const id = activeLayerId();
    return current && id ? current.getLayer(id) || null : null;
  };

  const isLocked = () => {
    const l = activeLayer();
    return l ? l.locked : false;
  };

  const apply = () => {
    const current = engine();
    const currentSession = session();
    // Ticket 2.2: facade-owned layer — one Rust command at apply, projection
    // authoritative; the per-edit preview signal is cleared.
    if (current && currentSession && isFacadeEnabled() && isFacadeOwnedLayer(currentSession.layerId)) {
      const f = getFacade(current.getId());
      const snap = f.commitTransform();
      if (snap) current.applyFacadeSnapshot(snap as never);
      clearTransformPreview();
      setLayerTransformSession(null);
      scheduler.requestRender();
      return;
    }
    const history = workspace.getActiveHistory();
    if (commitLayerTransformSession(session(), current, history)) {
      setLayerTransformSession(null);
      scheduler.requestRender();
    }
  };

  const cancel = () => {
    const current = engine();
    const currentSession = session();
    if (current && currentSession && isFacadeEnabled() && isFacadeOwnedLayer(currentSession.layerId)) {
      getFacade(current.getId()).cancelTransform();
      clearTransformPreview();
      setLayerTransformSession(null);
      scheduler.requestRender();
      return;
    }
    if (cancelLayerTransformSession(session(), current)) {
      setLayerTransformSession(null);
      scheduler.requestRender();
    }
  };

  const resetPreview = () => {
    const current = engine();
    const currentSession = session();
    // Ticket 2.2: facade-owned layer — revert transient preview to the
    // session's original transform (still uncommitted; zero IPC).
    if (current && currentSession && isFacadeEnabled() && isFacadeOwnedLayer(currentSession.layerId)) {
      getFacade(current.getId()).updateTransform({ ...currentSession.originalTransform });
      setTransformPreview({ layerId: currentSession.layerId, transform: { ...currentSession.originalTransform } });
      setTransformTick((t) => t + 1);
      scheduler.requestRender();
      return;
    }
    if (resetLayerTransformPreview(session(), current)) {
      setTransformTick(t => t + 1);
      scheduler.requestRender();
    }
  };

  const updateTransform = (patch: Partial<Transform2D>) => {
    const current = engine();
    const currentSession = session();
    if (!current || !currentSession) return;
    const layer = current.getLayer(currentSession.layerId);
    if (!layer || layer.locked) return;
    // Ticket 2.2: facade-owned layer — transient preview per edit, zero IPC.
    if (isFacadeEnabled() && isFacadeOwnedLayer(currentSession.layerId)) {
      const next = { ...layer.transform, ...patch };
      getFacade(current.getId()).updateTransform(next);
      setTransformPreview({ layerId: layer.id, transform: next });
      setTransformTick((t) => t + 1);
      scheduler.requestRender();
      return;
    }
    current.transformLayer(currentSession.layerId, { ...layer.transform, ...patch });
    setTransformTick((t) => t + 1);
    scheduler.requestRender();
  };

  const handlePositionField = (axis: "x" | "y") => (val: number) => {
    updateTransform({ [axis]: val });
  };

  const handleRotateField = (val: number) => {
    updateTransform({ rotation: val });
  };

  const setPreviewWidth = (nextWidth: number) => {
    const current = engine();
    const currentSession = session();
    if (!current || !currentSession) return;
    const layer = current.getLayer(currentSession.layerId);
    if (!layer || layer.locked || layer.width <= 0) return;
    if (!Number.isFinite(nextWidth) || nextWidth <= 0) return;
    const currentSign = Math.sign(layer.transform.scaleX || 1);
    const nextScaleX = currentSign * (nextWidth / layer.width);
    const next: Partial<Transform2D> = { scaleX: nextScaleX };
    if (constrainRatio() && layer.height > 0) {
      const ratioScale = Math.sign(layer.transform.scaleY || 1) * Math.abs(nextScaleX);
      next.scaleY = ratioScale;
    }
    // Ticket 2.2: facade-owned layer — transient preview, zero IPC.
    if (isFacadeEnabled() && isFacadeOwnedLayer(currentSession.layerId)) {
      const full = { ...layer.transform, ...next };
      getFacade(current.getId()).updateTransform(full);
      setTransformPreview({ layerId: layer.id, transform: full });
      setTransformTick((t) => t + 1);
      scheduler.requestRender();
      return;
    }
    current.transformLayer(currentSession.layerId, next);
    setTransformTick(t => t + 1);
    scheduler.requestRender();
  };

  const setPreviewHeight = (nextHeight: number) => {
    const current = engine();
    const currentSession = session();
    if (!current || !currentSession) return;
    const layer = current.getLayer(currentSession.layerId);
    if (!layer || layer.locked || layer.height <= 0) return;
    if (!Number.isFinite(nextHeight) || nextHeight <= 0) return;
    const currentSign = Math.sign(layer.transform.scaleY || 1);
    const nextScaleY = currentSign * (nextHeight / layer.height);
    const next: Partial<Transform2D> = { scaleY: nextScaleY };
    if (constrainRatio() && layer.width > 0) {
      const ratioScale = Math.sign(layer.transform.scaleX || 1) * Math.abs(nextScaleY);
      next.scaleX = ratioScale;
    }
    // Ticket 2.2: facade-owned layer — transient preview, zero IPC.
    if (isFacadeEnabled() && isFacadeOwnedLayer(currentSession.layerId)) {
      const full = { ...layer.transform, ...next };
      getFacade(current.getId()).updateTransform(full);
      setTransformPreview({ layerId: layer.id, transform: full });
      setTransformTick((t) => t + 1);
      scheduler.requestRender();
      return;
    }
    current.transformLayer(currentSession.layerId, next);
    setTransformTick(t => t + 1);
    scheduler.requestRender();
  };

  return (
    <>
      <ToolPill icon="move" label={t("properties.transform", "Transform")} />

      <Divider />

      <Show when={activeLayer()}>
        {(layer) => {
          const d = isLocked();

          // Reactive getters — depend on transformTick() so SolidJS
          // re-evaluates them when the layer transform is updated.
          const valueX = () => { transformTick(); return layer().transform.x; };
          const valueY = () => { transformTick(); return layer().transform.y; };
          const curW = () => { transformTick(); return Math.round(layer().width * Math.abs(layer().transform.scaleX)); };
          const curH = () => { transformTick(); return Math.round(layer().height * Math.abs(layer().transform.scaleY)); };
          const valueRot = () => { transformTick(); return layer().transform.rotation; };

          return (
            <>
              <div class="flex shrink-0 items-center gap-1">
                <EditableNumField
                  label="X"
                  labelClass="@max-[900px]:hidden"
                  suffix="px"
                  value={valueX()}
                  disabled={d}
                  onSubmit={handlePositionField("x")}
                  class="w-[62px]"
                />
                <EditableNumField
                  label="Y"
                  labelClass="@max-[900px]:hidden"
                  suffix="px"
                  value={valueY()}
                  disabled={d}
                  onSubmit={handlePositionField("y")}
                  class="w-[62px]"
                />
              </div>

              <div class="flex shrink-0 items-center gap-1">
                <EditableNumField
                  label="W"
                  labelClass="@max-[900px]:hidden"
                  suffix="px"
                  value={curW()}
                  disabled={d}
                  onSubmit={setPreviewWidth}
                  class="w-[70px]"
                />
                <EditableNumField
                  label="H"
                  labelClass="@max-[900px]:hidden"
                  suffix="px"
                  value={curH()}
                  disabled={d}
                  onSubmit={setPreviewHeight}
                  class="w-[70px]"
                />
              </div>

              <EditableNumField
                label="R"
                labelClass="@max-[900px]:hidden"
                value={valueRot()}
                suffix="°"
                disabled={d}
                onSubmit={handleRotateField}
                class="w-[58px]"
              />

              {/* Secondary controls — hidden at narrow widths */}
              <div class="hidden @min-[880px]:flex items-center gap-1.5 shrink-0">
                <Show when={session()}>
                  {(s) => (
                  <Tooltip content={t("tools.options.lockAspectRatio", "Lock Aspect Ratio")}>
                    <ToggleBtn
                      active={constrainRatio()}
                      onChange={setConstrainRatio}
                      icon={constrainRatio() ? "link" : "unlink"}
                      label={t("tools.options.aspectRatio", "Ratio")}
                    />
                  </Tooltip>
                  )}
                </Show>

                <Tooltip content={t("tools.options.resetTransformPreview", "Reset preview transform values")}>
                  <button
                    type="button"
                    onClick={resetPreview}
                    disabled={isLocked()}
                    class={clsx(
                      "flex h-[24px] shrink-0 items-center rounded-[4px] border px-2 text-[11px] font-semibold transition-all cursor-pointer select-none",
                      isLocked()
                        ? "border-transparent text-[#A1A1AA]/30 cursor-default"
                        : "border-editor-field-border/60 bg-editor-field/40 text-[#A1A1AA] hover:border-editor-field-border hover:bg-editor-field hover:text-white",
                    )}
                  >
                    {t("properties.reset", "Reset Preview")}
                  </button>
                </Tooltip>
              </div>
            </>
          );
        }}
      </Show>

      {/* Overflow dropdown for narrow container */}
      <Show when={session()}>
        {(s) => (
          <MoreDropdown>
            <div class="flex flex-col gap-1.5">
              <span class="text-[10px] font-bold text-[#A1A1AA] uppercase tracking-wider">{t("tools.options.moreOptions", "Options")}</span>
              <div class="flex items-center gap-2 bg-editor-field/30 p-1.5 rounded-[4px] border border-editor-field-border">
                <Tooltip content={t("tools.options.lockAspectRatio", "Lock Aspect Ratio")}>
                  <ToggleBtn
                    active={constrainRatio()}
                    onChange={setConstrainRatio}
                    icon={constrainRatio() ? "link" : "unlink"}
                    label={t("tools.options.aspectRatio", "Ratio")}
                  />
                </Tooltip>
                <Tooltip content={t("tools.options.resetTransformPreview", "Reset preview transform values")}>
                  <button
                    type="button"
                    onClick={resetPreview}
                    disabled={isLocked()}
                    class={clsx(
                      "flex h-[24px] items-center rounded-[4px] border px-2 text-[11px] font-semibold transition-all cursor-pointer select-none",
                      isLocked()
                        ? "border-transparent text-[#A1A1AA]/30 cursor-default"
                        : "border-editor-field-border/60 bg-editor-field/40 text-[#A1A1AA] hover:border-editor-field-border hover:bg-editor-field hover:text-white",
                    )}
                  >
                    {t("properties.reset", "Reset")}
                  </button>
                </Tooltip>
              </div>
            </div>
          </MoreDropdown>
        )}
      </Show>

      <Divider />

      <Tooltip content={t("tools.options.applyTransform", "Apply transform")} shortcut="Enter">
        <button
          type="button"
          class="h-6 px-2.5 rounded-[4px] border border-editor-accent bg-editor-accent text-white text-[11px] font-bold shadow-xs hover:bg-editor-accent/90 cursor-pointer select-none transition-colors"
          onClick={apply}
        >
          {t("common.apply", "Apply")}
        </button>
      </Tooltip>
      <Tooltip content={t("tools.options.cancelTransform", "Cancel transform")} shortcut="Esc">
        <button
          type="button"
          class="h-6 px-2.5 rounded-[4px] border border-[#363B44] bg-editor-field text-[#A1A1AA] text-[11px] font-semibold hover:border-[#4B515D] hover:text-white cursor-pointer select-none transition-colors"
          onClick={cancel}
        >
          {t("common.cancel", "Cancel")}
        </button>
      </Tooltip>
    </>
  );
}
