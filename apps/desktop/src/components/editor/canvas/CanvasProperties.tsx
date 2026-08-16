import { useEditor } from "../shell/EditorContext";
import { Icon } from "../icons";
import { PropRow, SelectField } from "../primitives";
import { SectionHeader } from "../layers/SectionHeader";
import { useI18n } from "@/i18n/I18nProvider";

export function CanvasProperties() {
  const { t } = useI18n();
  const {
    workspace,
    scheduler,
    zoom,
    docWidth,
    docHeight,
    syncViewport,
    viewportWidth,
    viewportHeight,
    setShowResizeDialog,
    documents,
    activeDocumentId,
  } = useEditor();

  const engine = () => workspace.getActiveEngine();
  const w = () => docWidth();
  const h = () => docHeight();
  const activeDoc = () => documents().find(d => d.id === activeDocumentId());
  const docName = () => activeDoc()?.displayName || "Untitled";

  const handleFitToScreen = () => {
    const e = engine();
    if (!e) return;
    e.fitToScreen(viewportWidth(), viewportHeight());
    syncViewport();
    scheduler.requestRender();
  };

  const handleResizeCanvas = () => {
    setShowResizeDialog(true);
  };

  return (
    <>
      <div class="border-b border-editor-divider px-4 py-3.5">
        <SectionHeader
          icon="image"
          iconClass="text-editor-text-dim"
          label={t("canvasProps.selectedDocument", "Selected Document")}
        />
        <div class="mt-3 flex items-center gap-3 rounded-[4px] border border-editor-divider bg-editor-field p-2.5">
          <div class="flex size-[34px] shrink-0 items-center justify-center rounded-[3px] border border-black/40 bg-white/[0.05] text-editor-text-dim">
            <Icon name="image" class="size-5" strokeWidth={1.5} />
          </div>
          <div class="min-w-0 flex-1">
            <p class="truncate text-[12.5px] font-medium text-editor-text leading-tight" title={docName()}>
              {docName()}
            </p>
            <p class="truncate text-[11px] text-editor-text-dim leading-snug mt-0.5">
              {t("status.canvas", "Canvas")} · {w()} × {h()} px
            </p>
          </div>
        </div>
      </div>

      <div class="border-b border-editor-divider px-4 py-3.5">
        <SectionHeader
          icon="image"
          iconClass="text-editor-text-dim"
          label={t("status.canvas", "Canvas")}
        />

        <div class="mt-3 flex flex-col gap-2.5">
          <PropRow label={t("tools.options.size", "Size")}>
            <button
              type="button"
              onClick={handleResizeCanvas}
              class="flex h-[26px] w-full items-center justify-between rounded-[4px] border border-editor-field-border bg-editor-field px-2.5 text-[12px] text-editor-text transition-colors hover:bg-editor-field-border"
            >
              <span>{`${w()} × ${h()} px`}</span>
              <Icon name="crop" class="size-3.5 text-editor-text-dim" strokeWidth={1.75} />
            </button>
          </PropRow>

          <PropRow label={t("canvasProps.mode", "Mode")}>
            <SelectField value="RGB / 8" class="flex-1" />
          </PropRow>

          <PropRow label={t("canvasProps.profile", "Profile")}>
            <SelectField value="sRGB IEC61966-2.1" class="flex-1" />
          </PropRow>

          <PropRow label={t("status.zoom", "Zoom")}>
            <button
              type="button"
              onClick={handleFitToScreen}
              class="flex h-[26px] w-full items-center justify-between rounded-[4px] border border-editor-field-border bg-editor-field px-2.5 text-[12px] text-editor-text transition-colors hover:bg-editor-field-border"
            >
              <span>{`${Math.round(zoom() * 100)} %`}</span>
              <Icon name="maximize" class="size-3.5 text-editor-text-dim" strokeWidth={1.75} />
            </button>
          </PropRow>
        </div>
      </div>

      <div class="border-b border-editor-divider px-4 py-3.5">
        <SectionHeader
          icon="maximize"
          iconClass="text-editor-text-dim"
          label={t("canvasProps.quickActions", "Quick Actions")}
        />

        <div class="mt-3 flex flex-col gap-2">
          <ActionButton
            icon="maximize"
            label={t("canvasProps.fitToScreen", "Fit to Screen")}
            onClick={handleFitToScreen}
          />
          <ActionButton
            icon="crop"
            label={t("menus.items.resizeCanvas", "Resize Canvas")}
            onClick={handleResizeCanvas}
          />
        </div>
      </div>
    </>
  );
}

function ActionButton(props: {
  icon: "maximize" | "crop";
  label: string;
  onClick: () => void;
}) {
  return (
    <button
      onClick={props.onClick}
      class="flex h-[30px] items-center gap-2 rounded-[4px] border border-editor-field-border bg-editor-field px-2.5 text-[12px] text-editor-text hover:bg-editor-field-border transition-colors"
    >
      <Icon name={props.icon} class="size-3.5 text-editor-text-dim" strokeWidth={1.75} />
      <span>{props.label}</span>
    </button>
  );
}
