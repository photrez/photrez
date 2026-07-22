import { Show, createEffect, createSignal, onCleanup, onMount, untrack } from "solid-js";
import { Portal } from "solid-js/web";
import { useEditor } from "../shell/EditorContext";
import { encodeComposite } from "../exportDocument";
import { printDocument } from "../printDocument";
import { DesktopDialog } from "./DesktopDialog";
import { PrintPaperViewport } from "../print/PrintPaperViewport";
import { PrintInspector } from "../print/PrintInspector";
import type { PrintOptions } from "../print/printTypes";
import { DEFAULT_PRINT_OPTIONS } from "../print/printTypes";
import { calculateScaleToFit } from "../print/printGeometry";

export function PrintDialog() {
  const {
    showPrintDialog,
    setShowPrintDialog,
    workspace,
    docWidth,
    docHeight,
  } = useEditor();

  const [previewUrl, setPreviewUrl] = createSignal<string | null>(null);
  const [printing, setPrinting] = createSignal(false);
  const [options, setOptions] = createSignal<PrintOptions>({
    ...DEFAULT_PRINT_OPTIONS,
  });

  // Calculate initial Scale to Fit and encode preview whenever dialog opens
  createEffect(() => {
    if (!showPrintDialog()) return;
    const engine = workspace.getActiveEngine();
    if (!engine) return;

    // Read current options untracked to avoid infinite signal loop
    const currentOpts = untrack(options);
    const fit = calculateScaleToFit(
      docWidth(),
      docHeight(),
      currentOpts.paperWidthMm,
      currentOpts.paperHeightMm,
      currentOpts.marginMm
    );
    setOptions((prev) => ({
      ...prev,
      scaleToFit: true,
      centerImage: true,
      scalePercent: fit.scalePercent,
      leftOffsetMm: fit.leftOffsetMm,
      topOffsetMm: fit.topOffsetMm,
    }));

    (async () => {
      try {
        // Fast JPEG preview for paper canvas viewport
        const bytes = await encodeComposite(engine, "jpeg", 85);
        const blob = new Blob([bytes as BlobPart], { type: "image/jpeg" });
        const old = previewUrl();
        if (old) URL.revokeObjectURL(old);
        setPreviewUrl(URL.createObjectURL(blob));
      } catch {
        // Preview is optional — silent fail
      }
    })();
  });

  onCleanup(() => {
    const url = previewUrl();
    if (url) URL.revokeObjectURL(url);
  });

  const handlePrint = async () => {
    const engine = workspace.getActiveEngine();
    if (!engine) return;
    setPrinting(true);
    try {
      await printDocument(engine, options());
    } finally {
      setPrinting(false);
      setShowPrintDialog(false);
    }
  };

  const activeDoc = () => workspace.getActiveSession();
  const docName = () => activeDoc()?.displayName || "Untitled";

  return (
    <Show when={showPrintDialog()}>
      <Portal mount={document.body}>
        <DesktopDialog
          title="Photrez Print Settings"
          kind="print"
          manageFocus
          dismissible={!printing()}
          onDismiss={() => setShowPrintDialog(false)}
          onBackdropPointerDown={() => {
            if (!printing()) setShowPrintDialog(false);
          }}
          widthClass="max-w-[1040px] w-[1040px] h-[660px]"
          bodyClass="p-0 h-[624px] flex flex-col justify-between"
        >
          {/* Pro-Suite 2-Column Print Layout (Flex 1 grow) */}
          <div class="flex flex-1 min-h-0 w-full gap-0 overflow-hidden">
            {/* Left Pane: Interactive SVG Paper Viewport */}
            <PrintPaperViewport
              options={options()}
              setOptions={setOptions}
              previewUrl={previewUrl()}
              docWidthPx={docWidth()}
              docHeightPx={docHeight()}
              docName={docName()}
            />

            {/* Right Pane: Print Inspector Dock (Continuous Surface) */}
            <PrintInspector
              options={options()}
              setOptions={setOptions}
              docWidthPx={docWidth()}
              docHeightPx={docHeight()}
            />
          </div>

          {/* Full-Width Footer Action Bar (100% Span) */}
          <div class="flex h-[44px] shrink-0 items-center justify-between border-t border-editor-divider bg-editor-topbar px-4 select-none">
            <label class="flex items-center gap-2 text-[11px] font-medium text-editor-text cursor-pointer">
              <input
                type="checkbox"
                class="size-3.5 rounded border-editor-field-border accent-[#E15A17] text-editor-accent focus:ring-0 cursor-pointer"
                checked={options().showPaperWhite}
                onChange={(e) =>
                  setOptions((prev) => ({
                    ...prev,
                    showPaperWhite: e.currentTarget.checked,
                  }))
                }
              />
              Show Paper White
            </label>

            <div class="flex items-center gap-2">
              <button
                type="button"
                class="h-[28px] rounded-[4px] border border-editor-field-border bg-editor-field px-4 text-[11.5px] font-medium text-editor-text hover:bg-editor-hover active:bg-editor-active transition-colors"
                onClick={() => setShowPrintDialog(false)}
                disabled={printing()}
              >
                Cancel
              </button>
              <button
                type="button"
                class="h-[28px] flex items-center gap-1.5 rounded-[4px] bg-editor-accent px-5 text-[11.5px] font-semibold text-white shadow-xs hover:brightness-110 active:brightness-95 disabled:opacity-50 transition-all"
                onClick={handlePrint}
                disabled={printing()}
              >
                <Show when={printing()}>
                  <span class="inline-block size-3 animate-spin rounded-full border-2 border-white/30 border-t-white" />
                </Show>
                {printing() ? "Preparing..." : "Print"}
              </button>
            </div>
          </div>
        </DesktopDialog>
      </Portal>
    </Show>
  );
}
