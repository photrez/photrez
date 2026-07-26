import { Show, createEffect, createSignal, onCleanup } from "solid-js";
import { Portal } from "solid-js/web";
import { useEditor } from "../shell/EditorContext";
import { encodeComposite } from "../exportDocument";
import { printDocument } from "../printDocument";
import { DesktopDialog } from "./DesktopDialog";
import { PrintPaperViewport } from "../print/PrintPaperViewport";
import { PrintInspector } from "../print/PrintInspector";
import { usePrintSettings } from "../print/usePrintSettings";

export function PrintDialog() {
  const {
    showPrintDialog,
    setShowPrintDialog,
    workspace,
    docWidth,
    docHeight,
  } = useEditor();

  // Rust-backed print state (single source of truth — BUG-08 fix)
  const {
    options,
    setOptions,
    loading,
    isPendingSetPaper,
    setPaper,
    toggleOrientation,
    setMarginMm,
    setScaleToFit,
    setScalePercent,
    setCenterImage,
    setTopOffsetMm,
    setLeftOffsetMm,
    setCopies,
    setUnit,
    setShowPaperWhite,
    setPrinter,
    openPrinterProperties,
  } = usePrintSettings("dialog");

  console.log("[PRINT:PrintDialog] RENDER — options().orientation:", options().orientation, "showPrintDialog:", showPrintDialog());

  // Effect to verify signal propagation (runs independently of component re-eval)
  createEffect(() => {
    console.log("[PRINT:PrintDialog] EFFECT — options().orientation:", options().orientation);
  });

  const [previewUrl, setPreviewUrl] = createSignal<string | null>(null);
  const [previewLoading, setPreviewLoading] = createSignal(false);
  const [printing, setPrinting] = createSignal(false);

  // ── Effect: Regenerate preview when relevant options change ──
  // Debounced at 150ms to avoid re-encoding on every keystroke.
  createEffect(() => {
    if (!showPrintDialog()) return;

    const opts = options();
    const engine = workspace.getActiveEngine();
    if (!engine) return;

    let cancelled = false;
    onCleanup(() => { cancelled = true; });

    const timer = setTimeout(() => {
      if (cancelled) return;
      setPreviewLoading(true);

      (async () => {
        try {
          const bytes = await encodeComposite(engine, "jpeg", 85);
          if (cancelled) return;
          const blob = new Blob([bytes as BlobPart], { type: "image/jpeg" });
          const old = previewUrl();
          if (old) URL.revokeObjectURL(old);
          setPreviewUrl(URL.createObjectURL(blob));
        } catch {
          // Preview is optional
        } finally {
          if (!cancelled) setPreviewLoading(false);
        }
      })();
    }, 150);

    onCleanup(() => clearTimeout(timer));
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
      await printDocument(engine, docName());
      setShowPrintDialog(false);
    } catch {
      // Error toast shown by printDocument
    } finally {
      setPrinting(false);
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
          <div class="flex flex-1 min-h-0 w-full gap-0 overflow-hidden">
            <PrintPaperViewport
              options={options}
              setCenterImage={setCenterImage}
              setLeftOffsetMm={setLeftOffsetMm}
              setTopOffsetMm={setTopOffsetMm}
              previewUrl={previewUrl()}
              previewLoading={previewLoading()}
              docWidthPx={docWidth()}
              docHeightPx={docHeight()}
              docName={docName()}
            />
            <PrintInspector
              docWidthPx={docWidth()}
              docHeightPx={docHeight()}
              options={options}
              setOptions={setOptions}
              loading={loading()}
              isPendingSetPaper={isPendingSetPaper}
              setPaper={setPaper}
              toggleOrientation={toggleOrientation}
              setMarginMm={setMarginMm}
              setScaleToFit={setScaleToFit}
              setScalePercent={setScalePercent}
              setCenterImage={setCenterImage}
              setTopOffsetMm={setTopOffsetMm}
              setLeftOffsetMm={setLeftOffsetMm}
              setCopies={setCopies}
              setUnit={setUnit}
              setShowPaperWhite={setShowPaperWhite}
              setPrinter={setPrinter}
              openPrinterProperties={openPrinterProperties}
            />
          </div>

          <div class="flex h-[44px] shrink-0 items-center justify-between border-t border-editor-divider bg-editor-topbar px-4 select-none">
            <label class="flex items-center gap-2 text-[11px] font-medium text-editor-text cursor-pointer">
              <input
                type="checkbox"
                class="size-3.5 rounded border-editor-field-border accent-[#E15A17] text-editor-accent focus:ring-0 cursor-pointer"
                checked={options().showPaperWhite}
                onChange={(e) => {
                  setShowPaperWhite(e.currentTarget.checked);
                }}
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
