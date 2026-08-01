import { createEffect, createSignal, For, on, type JSX } from "solid-js";
import { clsx } from "clsx";
import { DesktopDialog, DesktopDialogButton, desktopDialogFieldClass } from "./DesktopDialog";
import type { DialogRequest, NewDocumentResult } from "./DialogProvider";
import { type Unit, UNITS, formatUnit, unitToPx } from "@/lib/units";

const PRESETS = {
  Paper: [
    { name: "A3", width: 3508, height: 4961 },
    { name: "A4", width: 2480, height: 3508 },
    { name: "A5", width: 1748, height: 2480 },
    { name: "Letter", width: 2550, height: 3300 },
    { name: "Legal", width: 2550, height: 4200 },
    { name: "Foolscap", width: 2550, height: 3900 },
  ],
  "Social Media": [
    { name: "Facebook Post", width: 1200, height: 1200 },
    { name: "Facebook Cover", width: 1640, height: 664 },
    { name: "Instagram Square", width: 1080, height: 1080 },
    { name: "Instagram Portrait", width: 1080, height: 1350 },
    { name: "Instagram Story", width: 1080, height: 1920 },
    { name: "X Post", width: 1600, height: 900 },
    { name: "X Header", width: 1500, height: 500 },
    { name: "LinkedIn Banner", width: 1584, height: 396 },
    { name: "TikTok Video", width: 1080, height: 1920 },
    { name: "YouTube Thumbnail", width: 1280, height: 720 },
  ],
  "Web & Video": [
    { name: "HD", width: 1920, height: 1080 },
    { name: "QHD", width: 2560, height: 1440 },
    { name: "4K", width: 3840, height: 2160 },
  ],
  Photography: [
    { name: "4×6 Landscape", width: 1800, height: 1200 },
    { name: "4×6 Portrait", width: 1200, height: 1800 },
    { name: "5×7 Landscape", width: 2100, height: 1500 },
    { name: "5×7 Portrait", width: 1500, height: 2100 },
    { name: "8×10 Landscape", width: 2400, height: 3000 },
    { name: "8×10 Portrait", width: 3000, height: 2400 },
  ],

};

type Category = keyof typeof PRESETS;
const CATEGORIES = Object.keys(PRESETS) as Category[];

const getOrientationBadge = (w: number, h: number) => {
  if (w > h) return "L";
  if (w < h) return "P";
  return "SQ";
};

export function NewDocumentDialogContent(props: {
  request: Extract<DialogRequest, { kind: "new-document" }>;
  onClose: (result: NewDocumentResult | null) => void;
  dialogRef?: (element: HTMLDivElement) => void;
  onKeyDown?: JSX.EventHandler<HTMLDivElement, KeyboardEvent>;
}) {
  const [activeTab, setActiveTab] = createSignal<Category>(CATEGORIES[0]);
  const [docName, setDocName] = createSignal("New Project");
  // Always stored in pixels
  const [widthPx, setWidthPx] = createSignal(1080);
  const [heightPx, setHeightPx] = createSignal(1080);
  const [unit, setUnit] = createSignal<Unit>("px");
  const [background, setBackground] = createSignal<"transparent" | "white">("white");

  let widthInput!: HTMLInputElement;
  let heightInput!: HTMLInputElement;

  // Sync DOM display value when px or unit changes
  createEffect(on([unit, widthPx], () => {
    if (widthInput) widthInput.value = formatUnit(widthPx(), unit());
  }));
  createEffect(on([unit, heightPx], () => {
    if (heightInput) heightInput.value = formatUnit(heightPx(), unit());
  }));

  const applyPreset = (preset: { name: string, width: number, height: number }) => {
    setDocName(preset.name);
    setWidthPx(preset.width);
    setHeightPx(preset.height);
  };

  const handleCreate = () => {
    props.onClose({
      name: docName(),
      width: widthPx(),
      height: heightPx(),
      backgroundColor: background() as "white" | "transparent",
    });
  };

  const handleCancel = () => {
    props.onClose(null);
  };

  const getCanvasStyle = (w: number, h: number) => {
    const maxDim = 80; // Max size for the preview
    const scale = maxDim / Math.max(w, h);
    return {
      width: `${Math.max(20, Math.round(w * scale))}px`,
      height: `${Math.max(20, Math.round(h * scale))}px`,
    };
  };

  const isPresetSelected = (preset: { name: string, width: number, height: number }) => {
    return docName() === preset.name && widthPx() === preset.width && heightPx() === preset.height;
  };

  return (
    <DesktopDialog
      title={props.request.options.title ?? "New Document"}
      kind="new-document"
      onDismiss={handleCancel}
      widthClass="w-[780px]"
      bodyClass="!p-0 flex flex-col h-[460px] overflow-hidden"
      dialogRef={props.dialogRef}
      onKeyDown={props.onKeyDown}
    >
      {/* MAIN CONTENT */}
      <div class="flex flex-1 min-h-0">
        {/* LEFT PANEL */}
        <div class="flex-1 border-r border-editor-divider bg-editor-panel flex flex-col min-w-0">
          <div class="flex h-[44px] shrink-0 items-center border-b border-editor-divider">
            <For each={CATEGORIES}>
              {(cat) => (
                <button
                  class={clsx(
                    "relative flex h-full flex-1 items-center justify-center px-3 text-[12px] font-medium transition-colors whitespace-nowrap",
                    activeTab() === cat
                      ? "text-editor-text after:absolute after:bottom-0 after:inset-x-0 after:h-[2px] after:bg-editor-accent"
                      : "text-editor-text-dim hover:bg-white/[0.02] hover:text-editor-text"
                  )}
                  onClick={() => setActiveTab(cat)}
                >
                  {cat}
                </button>
              )}
            </For>
          </div>
          <div class="flex-1 overflow-y-auto p-5 custom-scrollbar">
            <div class="grid grid-cols-3 gap-3">
              <For each={PRESETS[activeTab()]}>
                {(preset) => (
                  <button
                    class={clsx(
                      "flex flex-col items-center p-3 rounded-lg border text-center transition-colors focus-visible:outline focus-visible:outline-1 focus-visible:outline-editor-accent/70 h-36",
                      isPresetSelected(preset)
                        ? "border-editor-accent bg-white/[0.08]"
                        : "border-editor-divider bg-black/15 hover:border-editor-accent/50 hover:bg-white/[0.04]"
                    )}
                    onClick={() => applyPreset(preset)}
                    onDblClick={() => {
                      applyPreset(preset);
                      handleCreate();
                    }}
                  >
                    <div class="flex-1 flex items-center justify-center w-full min-h-0">
                      <div class="flex h-20 w-20 items-center justify-center">
                        <div
                          class={clsx(
                            "rounded-[2px] shadow-sm border transition-colors",
                            isPresetSelected(preset)
                              ? "bg-white border-black/40"
                              : "bg-white/90 border-black/20"
                          )}
                          style={getCanvasStyle(preset.width, preset.height)}
                        />
                      </div>
                    </div>
                    <div class="flex flex-col gap-0.5 items-center shrink-0 w-full mt-2">
                      <span class="flex items-center justify-center gap-1.5 text-[12px] font-medium text-editor-text leading-tight truncate w-full px-1 max-w-full">
                        <span class="truncate">{preset.name}</span>
                        <span class="shrink-0 rounded bg-white/10 px-1 py-[1px] text-[9px] font-medium uppercase leading-none text-editor-text-dim">{getOrientationBadge(preset.width, preset.height)}</span>
                      </span>
                      <span class="text-[11px] text-editor-text-dim leading-tight text-center">{preset.width} × {preset.height}</span>
                    </div>
                  </button>
                )}
              </For>
            </div>
          </div>
        </div>

        {/* RIGHT PANEL (Form) */}
        <div class="w-[240px] shrink-0 bg-editor-topbar/30 flex flex-col p-4 gap-4">
          <div class="flex flex-col gap-4 flex-1">
            <div class="flex flex-col gap-1.5">
              <label class="text-[12px] font-medium text-editor-text-dim">Name</label>
              <input
                type="text"
                class={desktopDialogFieldClass}
                value={docName()}
                onInput={(e) => setDocName(e.currentTarget.value)}
              />
            </div>
            <div class="flex gap-2">
              <div class="flex flex-col gap-1.5 flex-1 min-w-0">
                <label class="text-[12px] font-medium text-editor-text-dim">Width</label>
                <input
                  ref={widthInput!}
                  type="number"
                  class={desktopDialogFieldClass}
                  value={formatUnit(widthPx(), unit())}
                  onBlur={(e) => {
                    const raw = e.currentTarget.value;
                    const val = parseFloat(raw);
                    if (isNaN(val) || val <= 0) {
                      setWidthPx(1);
                      e.currentTarget.value = formatUnit(1, unit());
                    } else {
                      setWidthPx(unitToPx(val, unit()));
                    }
                  }}
                  min="0.01"
                  step={unit() === "px" ? "1" : "0.01"}
                />
              </div>
              <div class="flex flex-col gap-1.5 flex-1 min-w-0">
                <label class="text-[12px] font-medium text-editor-text-dim">Height</label>
                <input
                  ref={heightInput!}
                  type="number"
                  class={desktopDialogFieldClass}
                  value={formatUnit(heightPx(), unit())}
                  onBlur={(e) => {
                    const raw = e.currentTarget.value;
                    const val = parseFloat(raw);
                    if (isNaN(val) || val <= 0) {
                      setHeightPx(1);
                      e.currentTarget.value = formatUnit(1, unit());
                    } else {
                      setHeightPx(unitToPx(val, unit()));
                    }
                  }}
                  min="0.01"
                  step={unit() === "px" ? "1" : "0.01"}
                />
              </div>
              <div class="flex flex-col gap-1.5 w-[60px] shrink-0">
                <label class="text-[12px] font-medium text-editor-text-dim">Unit</label>
                <select
                  class={`${desktopDialogFieldClass} text-center`}
                  value={unit()}
                  onChange={(e) => setUnit(e.currentTarget.value as Unit)}
                >
                  <For each={UNITS}>{(u) => <option value={u}>{u}</option>}</For>
                </select>
              </div>
            </div>
            <div class="flex flex-col gap-1.5">
              <label class="text-[12px] font-medium text-editor-text-dim">Background</label>
              <select
                class={desktopDialogFieldClass}
                value={background()}
                onChange={(e) => setBackground(e.currentTarget.value as "transparent" | "white")}
              >
                <option value="transparent">Transparent</option>
                <option value="white">White</option>
              </select>
            </div>
          </div>
        </div>
      </div>

      {/* FOOTER */}
      <div class="h-14 border-t border-editor-divider bg-editor-panel flex items-center justify-end px-5 gap-3 shrink-0">
        <DesktopDialogButton class="w-24 h-8" onClick={handleCancel}>
          Cancel
        </DesktopDialogButton>
        <DesktopDialogButton variant="primary" class="w-24 h-8" data-dialog-confirm onClick={handleCreate}>
          Create
        </DesktopDialogButton>
      </div>
    </DesktopDialog>
  );
}
