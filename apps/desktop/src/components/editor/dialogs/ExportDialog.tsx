import { For, Show, createSignal, onCleanup, onMount } from "solid-js";
import { Portal } from "solid-js/web";
import { useEditor } from "../shell/EditorContext";
import { exportActiveDocument } from "../exportDocument";
import { DesktopDialog, DesktopDialogButton } from "./DesktopDialog";
import { Slider } from "../primitives";
import { Icon } from "../icons";
import { tick } from "@/lib/dom";

type ExportFormat = "png" | "jpeg" | "webp";

const FORMATS: { id: ExportFormat; label: string; extensions: string[] }[] = [
  { id: "png", label: "PNG", extensions: ["png"] },
  { id: "jpeg", label: "JPEG", extensions: ["jpg", "jpeg"] },
  { id: "webp", label: "WebP", extensions: ["webp"] },
];

export function ExportDialog() {
  const {
    showExportDialog,
    setShowExportDialog,
    workspace,
    scheduler,
    syncViewport,
    documents,
    activeDocumentId,
    docWidth,
    docHeight,
  } = useEditor();

  const [format, setFormat] = createSignal<ExportFormat>("png");
  const [quality, setQuality] = createSignal(90);
  const [exporting, setExporting] = createSignal(false);
  const [donePath, setDonePath] = createSignal<string | null>(null);
  const [error, setError] = createSignal<string | null>(null);

  const hasQuality = () => format() !== "png";

  const activeDoc = () => documents().find(d => d.id === activeDocumentId());
  const docName = () => activeDoc()?.displayName || "Untitled";
  const docWidthVal = () => docWidth();
  const docHeightVal = () => docHeight();

  const [isOpen, setIsOpen] = createSignal(false);
  let dropdownContainerRef!: HTMLDivElement;

  onMount(() => {
    const handleOutsideClick = (e: MouseEvent) => {
      if (isOpen() && dropdownContainerRef && !dropdownContainerRef.contains(e.target as Node)) {
        setIsOpen(false);
      }
    };
    document.addEventListener("click", handleOutsideClick);
    onCleanup(() => document.removeEventListener("click", handleOutsideClick));
  });

  const formatDescription = () => {
    switch (format()) {
      case "png":
        return "Lossless compression. Ideal for graphics, text, and transparent backgrounds.";
      case "jpeg":
        return "Standard lossy compression. Best for photos and general web sharing.";
      case "webp":
        return "Modern image format. Superior compression and quality with transparency support.";
      default:
        return "";
    }
  };

  const qualityDescription = () => {
    const q = quality();
    if (q >= 90) return "Very High quality, larger file size.";
    if (q >= 70) return "High quality, optimized size (recommended).";
    return "Medium quality, smaller file size, visible artifacts.";
  };

  const handleExport = async () => {
    const engine = workspace.getActiveEngine();
    if (!engine) return;

    const session = workspace.getActiveSession();
    const name = session?.displayName || "Untitled";

    setExporting(true);
    setError(null);
    setDonePath(null);

    // yield so the spinner renders before the blocking encode
    await tick();

    try {
      const path = await exportActiveDocument(engine, name, format(), quality());
      if (path) {
        setDonePath(path);
        scheduler.requestRender();
        syncViewport();
      }
    } catch (e) {
      setError(String(e));
    } finally {
      setExporting(false);
    }
  };

  const handleClose = () => {
    setShowExportDialog(false);
    setDonePath(null);
    setError(null);
  };

  return (
    <Show when={showExportDialog()}>
      <Portal mount={document.body}>
        <DesktopDialog
          title="Export Image"
          kind="export"
          manageFocus
          dismissible={!exporting()}
          onDismiss={handleClose}
          onBackdropPointerDown={() => { if (!exporting()) handleClose(); }}
          actions={<>
            <DesktopDialogButton onClick={handleClose} disabled={exporting()}>
              {donePath() ? "Close" : "Cancel"}
            </DesktopDialogButton>
            <Show when={!donePath()}>
              <DesktopDialogButton variant="primary" onClick={handleExport} disabled={exporting()}>
                <Show when={exporting()}>
                  <span aria-hidden="true" style="animation: spin 1s linear infinite;" class="mr-1.5 inline-block size-3 rounded-full border-2 border-editor-bg/30 border-t-editor-bg" />
                </Show>
                {exporting() ? "Exporting..." : "Export"}
              </DesktopDialogButton>
            </Show>
          </>}
        >
          <Show when={donePath()}>
            <div role="status" class="mb-3 rounded-[6px] border border-success/30 bg-success/10 px-2.5 py-2 text-[11px] text-success">
              Saved: {donePath()?.split(/[/\\]/).pop()}
            </div>
          </Show>

          <Show when={error()}>
            <div role="alert" class="mb-3 rounded-[6px] border border-danger/30 bg-danger/10 px-2.5 py-2 text-[11px] text-danger">
              {error()}
            </div>
          </Show>

          <div class="flex flex-col gap-4">
            {/* Active Document Info Card */}
            <div class="flex items-center gap-3 rounded-[6px] border border-editor-divider bg-editor-field p-2.5">
              <div class="flex size-[36px] shrink-0 items-center justify-center rounded-[4px] border border-black/45 bg-white/[0.04] text-editor-accent">
                <Icon name="image" class="size-5 text-editor-accent" strokeWidth={1.5} />
              </div>
              <div class="min-w-0 flex-1">
                <div class="truncate text-[12px] font-semibold text-editor-text leading-tight" title={docName()}>
                  {docName()}
                </div>
                <div class="truncate text-[11px] text-editor-text-dim leading-snug mt-0.5">
                  {docWidthVal()} × {docHeightVal()} px · RGB 8-bit
                </div>
              </div>
            </div>

            {/* Format Selector */}
            <div class="relative" ref={dropdownContainerRef}>
              <label for="export-format-select" class="mb-1.5 block text-[11px] font-semibold text-editor-text-dim uppercase tracking-wider">Format</label>
              <button
                type="button"
                onClick={() => setIsOpen(!isOpen())}
                class="flex h-[30px] w-full items-center justify-between rounded-[4px] border border-editor-field-border bg-editor-field px-2.5 hover:bg-editor-field-border/60 focus:border-editor-accent focus:ring-1 focus:ring-editor-accent/30 transition-colors cursor-pointer outline-none"
                aria-haspopup="listbox"
                aria-expanded={isOpen()}
              >
                <div class="flex items-center gap-2">
                  <span class="flex items-center justify-center text-[9px] font-bold px-1.5 py-0.5 rounded bg-editor-canvas border border-editor-field-border/40 text-editor-text-dim leading-none">
                    {format().toUpperCase()}
                  </span>
                  <span class="text-[12px] text-editor-text font-medium">
                    {FORMATS.find(f => f.id === format())?.label}
                  </span>
                </div>
                <Icon
                  name="chevron-down"
                  class={`size-3.5 text-editor-text-dim transition-transform duration-150 ${isOpen() ? "rotate-180" : ""}`}
                  strokeWidth={1.75}
                />
              </button>

              {/* Hidden select for compatibility with tests & native features */}
              <select
                id="export-format-select"
                value={format()}
                onChange={(e) => setFormat(e.currentTarget.value as ExportFormat)}
                class="absolute opacity-0 pointer-events-none size-0"
                data-dialog-initial-focus
              >
                <For each={FORMATS}>{(f) => (
                  <option value={f.id}>{f.label}</option>
                )}</For>
              </select>

              {/* Polished custom dropdown menu */}
              <Show when={isOpen()}>
                <div class="absolute left-0 right-0 top-[52px] z-[80] rounded-[6px] border border-editor-divider bg-editor-panel py-1 text-[12px] text-editor-text shadow-xl">
                  <For each={FORMATS}>{(f) => (
                    <button
                      type="button"
                      class={`flex h-7 w-full items-center justify-between px-3 text-left outline-none hover:bg-editor-field/70 cursor-pointer ${
                        format() === f.id ? "bg-white/[0.04] text-editor-accent font-semibold" : ""
                      }`}
                      onClick={() => {
                        setFormat(f.id);
                        setIsOpen(false);
                      }}
                    >
                      <span>{f.label}</span>
                      <Show when={format() === f.id}>
                        <Icon name="chevron-right" class="size-3.5 text-editor-accent" strokeWidth={1.75} />
                      </Show>
                    </button>
                  )}</For>
                </div>
              </Show>

              <p class="mt-2 text-[11px] text-editor-text-dim leading-relaxed">
                {formatDescription()}
              </p>
            </div>

            {/* Quality Slider Section */}
            <Show when={hasQuality()}>
              <div class="border-t border-editor-divider pt-4">
                <div class="mb-1.5 flex items-center justify-between text-[11px] text-editor-text-dim">
                  <label for="export-quality" class="font-semibold uppercase tracking-wider">Quality</label>
                  <span class="font-sans tabular-nums text-editor-text font-bold">{quality()}%</span>
                </div>
                <div class="relative flex items-center h-[14px]">
                  <Slider
                    percent={quality()}
                    type="zoom"
                  />
                  <input
                    id="export-quality"
                    type="range"
                    min={1}
                    max={100}
                    value={quality()}
                    onInput={(e) => setQuality(parseInt(e.currentTarget.value, 10))}
                    class="absolute inset-0 w-full h-[14px] opacity-0 cursor-pointer focus-visible:outline focus-visible:outline-1 focus-visible:outline-offset-2 focus-visible:outline-editor-accent"
                  />
                </div>
                <p class="mt-2 text-[11px] text-editor-text-dim leading-relaxed">
                  {qualityDescription()}
                </p>
              </div>
            </Show>
          </div>
        </DesktopDialog>
      </Portal>
    </Show>
  );
}
