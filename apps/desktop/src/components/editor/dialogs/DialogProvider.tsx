import {
  Show,
  createContext,
  createEffect,
  createSignal,
  createMemo,
  onCleanup,
  useContext,
  type JSX,
  type ParentProps,
} from "solid-js";
import { Portal } from "solid-js/web";
import { DesktopDialog, DesktopDialogButton, desktopDialogFieldClass } from "./DesktopDialog";
import { Slider } from "../primitives";
import { useEditor } from "../shell/EditorContext";
import { NewDocumentDialogContent } from "./NewDocumentDialog";
import { AboutDialog } from "./AboutDialog";

export interface ConfirmDialogOptions {
  title: string;
  message: string;
  confirmLabel?: string;
  cancelLabel?: string;
  tone?: "default" | "danger";
}

export interface AlertDialogOptions {
  title: string;
  message: string;
  confirmLabel?: string;
}

export interface QualityDialogOptions {
  title: string;
  format: "jpeg" | "webp";
  defaultQuality: number;
}

export interface ConfirmWithCheckboxOptions {
  title: string;
  message: string;
  checkboxLabel: string;
  checkboxChecked?: boolean;
  confirmLabel?: string;
  cancelLabel?: string;
  tone?: "default" | "danger";
}

export interface ConfirmWithCheckboxResult {
  confirmed: boolean;
  checked: boolean;
}

export interface ConfirmSaveOptions {
  title: string;
  message: string;
  saveLabel?: string;
  discardLabel?: string;
  cancelLabel?: string;
}

export type ConfirmSaveResult = "save" | "discard" | "cancel";

export interface ColorPickerDialogOptions {
  title: string;
  initialColor: string;
  /** Which swatch the canvas pick writes into. */
  target?: "foreground" | "background";
  onChange?: (color: string) => void;
}

export interface NewDocumentDialogOptions {
  title?: string;
}

export interface NewDocumentResult {
  name: string;
  width: number;
  height: number;
  backgroundColor: "white" | "transparent";
}

export interface DialogContextValue {
  confirm: (options: ConfirmDialogOptions) => Promise<boolean>;
  alert: (options: AlertDialogOptions) => Promise<void>;
  quality: (options: QualityDialogOptions) => Promise<number | null>;
  confirmWithCheckbox: (options: ConfirmWithCheckboxOptions) => Promise<ConfirmWithCheckboxResult>;
  confirmSave: (options: ConfirmSaveOptions) => Promise<ConfirmSaveResult>;
  colorPicker: (options: ColorPickerDialogOptions) => Promise<string | null>;
  newDocument: (options?: NewDocumentDialogOptions) => Promise<NewDocumentResult | null>;
  about: () => Promise<void>;
}

export type DialogRequest =
  | { kind: "confirm"; options: ConfirmDialogOptions; resolve: (result: boolean) => void }
  | { kind: "alert"; options: AlertDialogOptions; resolve: () => void }
  | { kind: "quality"; options: QualityDialogOptions; resolve: (result: number | null) => void }
  | { kind: "confirm-checkbox"; options: ConfirmWithCheckboxOptions; resolve: (result: ConfirmWithCheckboxResult) => void }
  | { kind: "confirm-save"; options: ConfirmSaveOptions; resolve: (result: ConfirmSaveResult) => void }
  | { kind: "color-picker"; options: ColorPickerDialogOptions; resolve: (result: string | null) => void }
  | { kind: "new-document"; options: NewDocumentDialogOptions; resolve: (result: NewDocumentResult | null) => void }
  | { kind: "about"; resolve: () => void };

const isDangerRequest = (request: DialogRequest) => (
  request.kind === "confirm" && request.options.tone === "danger"
);

const cancelLabelFor = (request: DialogRequest) => (
  request.kind === "confirm" ? request.options.cancelLabel ?? "Cancel" : "Cancel"
);

const DialogContext = createContext<DialogContextValue>();

export function useDialog(): DialogContextValue {
  const value = useContext(DialogContext);
  if (!value) throw new Error("useDialog must be used within DialogProvider");
  return value;
}

export function DialogProvider(props: ParentProps) {
  const [current, setCurrent] = createSignal<DialogRequest | null>(null);
  const queue: DialogRequest[] = [];
  let dialogRef!: HTMLDivElement;
  let cancelRef!: HTMLButtonElement;
  let confirmRef!: HTMLButtonElement;
  let restoreFocusTo: HTMLElement | null = null;

  const showNext = () => {
    if (current() || queue.length === 0) return;
    restoreFocusTo = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    setCurrent(queue.shift()!);
  };

  const confirm = (options: ConfirmDialogOptions) => new Promise<boolean>((resolve) => {
    queue.push({ kind: "confirm", options, resolve });
    showNext();
  });

  const alert = (options: AlertDialogOptions) => new Promise<void>((resolve) => {
    queue.push({ kind: "alert", options, resolve });
    showNext();
  });

  const quality = (options: QualityDialogOptions) => new Promise<number | null>((resolve) => {
    queue.push({ kind: "quality", options, resolve });
    showNext();
  });

  const confirmWithCheckbox = (options: ConfirmWithCheckboxOptions) => new Promise<ConfirmWithCheckboxResult>((resolve) => {
    queue.push({ kind: "confirm-checkbox", options, resolve });
    showNext();
  });

  const confirmSave = (options: ConfirmSaveOptions) => new Promise<ConfirmSaveResult>((resolve) => {
    queue.push({ kind: "confirm-save", options, resolve });
    showNext();
  });

  const colorPicker = (options: ColorPickerDialogOptions) => new Promise<string | null>((resolve) => {
    queue.push({ kind: "color-picker", options, resolve });
    showNext();
  });

  const newDocument = (options?: NewDocumentDialogOptions) => new Promise<NewDocumentResult | null>((resolve) => {
    queue.push({ kind: "new-document", options: options ?? {}, resolve });
    showNext();
  });

  const about = () => new Promise<void>((resolve) => {
    queue.push({ kind: "about", resolve });
    showNext();
  });

  const complete = (accepted: boolean) => {
    const request = current();
    if (!request) return;
    if (request.kind === "confirm") request.resolve(accepted);
    else if (request.kind === "confirm-checkbox") request.resolve({ confirmed: false, checked: false });
    else if (request.kind === "confirm-save") request.resolve("cancel");
    else if (request.kind === "quality") request.resolve(null);
    else if (request.kind === "color-picker") request.resolve(null);
    else if (request.kind === "new-document") request.resolve(null);
    else request.resolve();
    setCurrent(null);
    queueMicrotask(() => {
      restoreFocusTo?.focus();
      restoreFocusTo = null;
      showNext();
    });
  };

  createEffect(() => {
    const request = current();
    if (!request) return;
    queueMicrotask(() => {
      if (request.kind === "confirm" || request.kind === "confirm-checkbox") cancelRef?.focus();
      else confirmRef?.focus();
    });
  });

  const handleKeyDown: JSX.EventHandler<HTMLDivElement, KeyboardEvent> = (event) => {
    event.stopPropagation();
    const request = current();
    if (!request) return;
    if (event.key === "Escape") {
      event.preventDefault();
      complete(false);
      return;
    }
    if (event.key !== "Tab") return;
    const focusable = Array.from(
      dialogRef.querySelectorAll<HTMLButtonElement>('button:not([aria-label="Close"]):not(:disabled)'),
    );
    if (focusable.length === 0) return;
    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  };

  onCleanup(() => {
    const request = current();
    if (request?.kind === "confirm") request.resolve(false);
    else if (request?.kind === "confirm-checkbox") request.resolve({ confirmed: false, checked: false });
    else if (request?.kind === "confirm-save") request.resolve("cancel");
    else if (request?.kind === "quality") request.resolve(null);
    else if (request?.kind === "color-picker") request.resolve(null);
    else if (request?.kind === "new-document") request.resolve(null);
    else request?.resolve();
    for (const queued of queue.splice(0)) {
      if (queued.kind === "confirm") queued.resolve(false);
      else if (queued.kind === "confirm-checkbox") queued.resolve({ confirmed: false, checked: false });
      else if (queued.kind === "confirm-save") queued.resolve("cancel");
      else if (queued.kind === "quality") queued.resolve(null);
      else if (queued.kind === "color-picker") queued.resolve(null);
      else if (queued.kind === "new-document") queued.resolve(null);
      else queued.resolve();
    }
  });

  function hexToRgb(hex: string): { r: number; g: number; b: number } {
    let h = hex.replace("#", "");
    if (h.length === 3) {
      h = h[0] + h[0] + h[1] + h[1] + h[2] + h[2];
    }
    const num = parseInt(h, 16);
    return {
      r: (num >> 16) & 255,
      g: (num >> 8) & 255,
      b: num & 255
    };
  }

  function rgbToHex(r: number, g: number, b: number): string {
    const toHex = (c: number) => {
      const s = Math.max(0, Math.min(255, Math.round(c))).toString(16);
      return s.length === 1 ? "0" + s : s;
    };
    return `#${toHex(r)}${toHex(g)}${toHex(b)}`;
  }

  function rgbToHsv(r: number, g: number, b: number): { h: number; s: number; v: number } {
    r /= 255;
    g /= 255;
    b /= 255;
    const max = Math.max(r, g, b);
    const min = Math.min(r, g, b);
    const d = max - min;
    let h = 0;
    const s = max === 0 ? 0 : d / max;
    const v = max;

    if (max !== min) {
      switch (max) {
        case r:
          h = (g - b) / d + (g < b ? 6 : 0);
          break;
        case g:
          h = (b - r) / d + 2;
          break;
        case b:
          h = (r - g) / d + 4;
          break;
      }
      h /= 6;
    }
    return {
      h: Math.round(h * 360),
      s: Math.round(s * 100),
      v: Math.round(v * 100)
    };
  }

  function hsvToRgb(h: number, s: number, v: number): { r: number; g: number; b: number } {
    s /= 100;
    v /= 100;
    let r = 0, g = 0, b = 0;
    const i = Math.floor(h / 60) % 6;
    const f = h / 60 - Math.floor(h / 60);
    const p = v * (1 - s);
    const q = v * (1 - f * s);
    const t = v * (1 - (1 - f) * s);

    switch (i) {
      case 0: r = v; g = t; b = p; break;
      case 1: r = q; g = v; b = p; break;
      case 2: r = p; g = v; b = t; break;
      case 3: r = p; g = q; b = v; break;
      case 4: r = t; g = p; b = v; break;
      case 5: r = v; g = p; b = q; break;
    }

    return {
      r: Math.round(r * 255),
      g: Math.round(g * 255),
      b: Math.round(b * 255)
    };
  }

  function ColorPickerDialogContent(props: { request: Extract<DialogRequest, { kind: "color-picker" }> }) {
    const initialHex = props.request.options.initialColor;
    const initialRgb = hexToRgb(initialHex);
    const initialHsv = rgbToHsv(initialRgb.r, initialRgb.g, initialRgb.b);

    const [h, setH] = createSignal(initialHsv.h);
    const [s, setS] = createSignal(initialHsv.s);
    const [v, setV] = createSignal(initialHsv.v);

    const { fgColor, bgColor, colorPickerOpen, colorPickerTarget, setColorPickerOpen } = useEditor();
    const target = () => props.request.options.target ?? "foreground";
    onCleanup(() => setColorPickerOpen(false));

    const rgb = createMemo(() => hsvToRgb(h(), s(), v()));
    const currentHex = createMemo(() => rgbToHex(rgb().r, rgb().g, rgb().b));

    createEffect(() => {
      const hex = currentHex();
      props.request.options.onChange?.(hex);
    });

    const updateRgb = (newR: number, newG: number, newB: number) => {
      const hsv = rgbToHsv(newR, newG, newB);
      setH(hsv.h);
      setS(hsv.s);
      setV(hsv.v);
    };

    const updateRgbFromHex = (hex: string) => {
      const rgb = hexToRgb(hex);
      const hsv = rgbToHsv(rgb.r, rgb.g, rgb.b);
      setH(hsv.h);
      setS(hsv.s);
      setV(hsv.v);
    };

    // While the picker is open (non-modal), the canvas writes the sampled
    // color into fg/bg. Mirror it into the dialog's internal HSV so the
    // swatch + HSB/RGB/hex fields show the live picked color.
    createEffect(() => {
      if (!colorPickerOpen() || colorPickerTarget() !== target()) return;
      const hex = target() === "foreground" ? fgColor() : bgColor();
      updateRgbFromHex(hex);
    });

    const handleCancel = () => {
      complete(false);
    };

    const handleOk = () => {
      props.request.resolve(currentHex());
      setCurrent(null);
      queueMicrotask(() => {
        restoreFocusTo?.focus();
        restoreFocusTo = null;
        showNext();
      });
    };

    let hsvBoxRef!: HTMLDivElement;
    let hueSliderRef!: HTMLDivElement;

    const handleHsvPointer = (e: PointerEvent) => {
      const rect = hsvBoxRef.getBoundingClientRect();
      const x = Math.max(0, Math.min(rect.width, e.clientX - rect.left));
      const y = Math.max(0, Math.min(rect.height, e.clientY - rect.top));
      setS(Math.round((x / rect.width) * 100));
      setV(Math.round((1 - y / rect.height) * 100));
    };

    const handleHsvPointerDown = (e: PointerEvent) => {
      hsvBoxRef.setPointerCapture(e.pointerId);
      handleHsvPointer(e);
    };

    const handleHsvPointerMove = (e: PointerEvent) => {
      if (hsvBoxRef.hasPointerCapture(e.pointerId)) {
        handleHsvPointer(e);
      }
    };

    const handleHsvPointerUp = (e: PointerEvent) => {
      if (hsvBoxRef.hasPointerCapture(e.pointerId)) {
        hsvBoxRef.releasePointerCapture(e.pointerId);
      }
    };

    const handleHuePointer = (e: PointerEvent) => {
      const rect = hueSliderRef.getBoundingClientRect();
      const y = Math.max(0, Math.min(rect.height, e.clientY - rect.top));
      setH(Math.round((y / rect.height) * 360));
    };

    const handleHuePointerDown = (e: PointerEvent) => {
      hueSliderRef.setPointerCapture(e.pointerId);
      handleHuePointer(e);
    };

    const handleHuePointerMove = (e: PointerEvent) => {
      if (hueSliderRef.hasPointerCapture(e.pointerId)) {
        handleHuePointer(e);
      }
    };

    const handleHuePointerUp = (e: PointerEvent) => {
      if (hueSliderRef.hasPointerCapture(e.pointerId)) {
        hueSliderRef.releasePointerCapture(e.pointerId);
      }
    };

    createEffect(() => {
      props.request.options.onChange?.(currentHex());
    });

    return (
      <DesktopDialog
        dialogRef={(element) => { dialogRef = element; }}
        role="dialog"
        title={props.request.options.title}
        kind="color-picker"
        tone="default"
        widthClass="w-fit max-w-[calc(100vw-24px)]"
        bodyClass="py-4 px-5 min-h-[220px]"
        modal={false}
        onDismiss={handleCancel}
        onKeyDown={handleKeyDown}
      >
        <div class="flex gap-4 select-none">
          {/* Column 1: Saturation/Value Box */}
          <div
            ref={hsvBoxRef}
            onPointerDown={handleHsvPointerDown}
            onPointerMove={handleHsvPointerMove}
            onPointerUp={handleHsvPointerUp}
            class="relative size-[200px] shrink-0 cursor-crosshair rounded-[4px] border border-editor-field-border overflow-hidden shadow-sm"
            style={{
              "background-color": `hsl(${h()}, 100%, 50%)`,
              "background-image": "linear-gradient(to top, #000, transparent), linear-gradient(to right, #fff, transparent)"
            }}
          >
            {/* Circular Indicator */}
            <div
              class="absolute size-3 -translate-x-1/2 -translate-y-1/2 rounded-full border border-white outline outline-1 outline-black/50 pointer-events-none"
              style={{
                left: `${s()}%`,
                top: `${100 - v()}%`
              }}
            />
          </div>

          {/* Column 2: Hue Spectrum Track */}
          <div
            ref={hueSliderRef}
            onPointerDown={handleHuePointerDown}
            onPointerMove={handleHuePointerMove}
            onPointerUp={handleHuePointerUp}
            class="relative w-[18px] h-[200px] shrink-0 cursor-ns-resize rounded-[4px] border border-editor-field-border overflow-hidden shadow-sm"
            style={{
              "background-image": "linear-gradient(to bottom, #f00 0%, #ff0 17%, #0f0 33%, #0ff 50%, #00f 67%, #f0f 83%, #f0f 100%)"
            }}
          >
            {/* Hue indicator bar */}
            <div
              class="absolute left-0 right-0 h-1.5 -translate-y-1/2 border-y border-white outline outline-1 outline-black/50 pointer-events-none bg-black/10"
              style={{
                top: `${(h() / 360) * 100}%`
              }}
            />
          </div>

          {/* Right Section Container */}
          <div class="flex flex-col w-[180px] shrink-0 gap-3.5 select-none">
            {/* Top Row: Swatch (Left) & OK/Cancel (Right) */}
            <div class="flex items-start justify-between">
              {/* Swatch Preview Container */}
              <div class="flex flex-col items-center w-[86px] select-none">
                <span class="text-[9px] font-medium text-editor-text-dim leading-none mb-1">new</span>
                <div class="flex flex-col w-12 h-12 rounded-[4px] border border-editor-field-border overflow-hidden shadow-sm shrink-0">
                  <div class="flex-1" style={{ "background-color": currentHex() }} title="New Color" />
                  <div class="flex-1" style={{ "background-color": initialHex }} title="Current Color" />
                </div>
                <span class="text-[9px] font-medium text-editor-text-dim leading-none mt-1">current</span>
              </div>

              {/* OK & Cancel buttons */}
              <div class="flex flex-col gap-1.5 w-[86px] items-end">
                <DesktopDialogButton
                  ref={(element) => { confirmRef = element; }}
                  variant="primary"
                  class="w-[84px]"
                  onClick={handleOk}
                >
                  OK
                </DesktopDialogButton>
                <DesktopDialogButton
                  ref={(element) => { cancelRef = element; }}
                  class="w-[84px]"
                  onClick={handleCancel}
                >
                  Cancel
                </DesktopDialogButton>
              </div>
            </div>

            {/* Bottom Row: Perfect Alignment Columns */}
            <div class="flex items-start justify-between">
              {/* Left Column: HSB Inputs */}
              <div class="flex flex-col gap-1.5 items-center w-[86px]">
                <div class="flex items-center gap-1 w-full justify-between">
                  <span class="text-[11px] font-bold text-editor-text-dim w-3 text-right">H:</span>
                  <input
                    type="number"
                    min={0}
                    max={360}
                    value={h()}
                    onInput={(e) => setH(Math.max(0, Math.min(360, parseInt(e.currentTarget.value, 10) || 0)))}
                    class={`${desktopDialogFieldClass} text-center w-[54px] h-7`}
                  />
                  <span class="text-[11px] text-editor-text-dim w-3 text-left">°</span>
                </div>
                <div class="flex items-center gap-1 w-full justify-between">
                  <span class="text-[11px] font-bold text-editor-text-dim w-3 text-right">S:</span>
                  <input
                    type="number"
                    min={0}
                    max={100}
                    value={s()}
                    onInput={(e) => setS(Math.max(0, Math.min(100, parseInt(e.currentTarget.value, 10) || 0)))}
                    class={`${desktopDialogFieldClass} text-center w-[54px] h-7`}
                  />
                  <span class="text-[11px] text-editor-text-dim w-3 text-left">%</span>
                </div>
                <div class="flex items-center gap-1 w-full justify-between">
                  <span class="text-[11px] font-bold text-editor-text-dim w-3 text-right">B:</span>
                  <input
                    type="number"
                    min={0}
                    max={100}
                    value={v()}
                    onInput={(e) => setV(Math.max(0, Math.min(100, parseInt(e.currentTarget.value, 10) || 0)))}
                    class={`${desktopDialogFieldClass} text-center w-[54px] h-7`}
                  />
                  <span class="text-[11px] text-editor-text-dim w-3 text-left">%</span>
                </div>
              </div>

              {/* Right Column: RGB Inputs */}
              <div class="flex flex-col gap-1.5 items-center w-[86px]">
                <div class="flex items-center gap-1 w-full justify-between">
                  <span class="text-[11px] font-bold text-editor-text-dim w-3 text-right">R:</span>
                  <input
                    type="number"
                    min={0}
                    max={255}
                    value={rgb().r}
                    onInput={(e) => updateRgb(Math.max(0, Math.min(255, parseInt(e.currentTarget.value, 10) || 0)), rgb().g, rgb().b)}
                    class={`${desktopDialogFieldClass} text-center w-[54px] h-7`}
                  />
                  <span class="w-3" />
                </div>
                <div class="flex items-center gap-1 w-full justify-between">
                  <span class="text-[11px] font-bold text-editor-text-dim w-3 text-right">G:</span>
                  <input
                    type="number"
                    min={0}
                    max={255}
                    value={rgb().g}
                    onInput={(e) => updateRgb(rgb().r, Math.max(0, Math.min(255, parseInt(e.currentTarget.value, 10) || 0)), rgb().b)}
                    class={`${desktopDialogFieldClass} text-center w-[54px] h-7`}
                  />
                  <span class="w-3" />
                </div>
                <div class="flex items-center gap-1 w-full justify-between">
                  <span class="text-[11px] font-bold text-editor-text-dim w-3 text-right">B:</span>
                  <input
                    type="number"
                    min={0}
                    max={255}
                    value={rgb().b}
                    onInput={(e) => updateRgb(rgb().r, rgb().g, Math.max(0, Math.min(255, parseInt(e.currentTarget.value, 10) || 0)))}
                    class={`${desktopDialogFieldClass} text-center w-[54px] h-7`}
                  />
                  <span class="w-3" />
                </div>
              </div>
            </div>

            {/* Hex input row (Centered) */}
            <div class="flex items-center justify-center gap-1.5 w-full mt-0.5">
              <span class="text-[11px] font-bold text-editor-text-dim text-right">#</span>
              <input
                type="text"
                value={currentHex().replace("#", "")}
                onInput={(e) => {
                  const val = e.currentTarget.value.trim();
                  if (val.match(/^[0-9a-fA-F]{3}$/) || val.match(/^[0-9a-fA-F]{6}$/)) {
                    const parsed = hexToRgb(val);
                    updateRgb(parsed.r, parsed.g, parsed.b);
                  }
                }}
                class={`${desktopDialogFieldClass} text-center w-[64px] h-7 text-[11px] uppercase`}
                placeholder="ffffff"
              />
            </div>
          </div>
        </div>
      </DesktopDialog>
    );
  }

  function QualityDialogContent(props: { request: Extract<DialogRequest, { kind: "quality" }> }) {
    const [sliderValue, setSliderValue] = createSignal(
      props.request.options.defaultQuality
    );
    const handleCancel = () => {
      complete(false);
    };
    const handleSave = () => {
      props.request.resolve(sliderValue());
      setCurrent(null);
      queueMicrotask(() => {
        restoreFocusTo?.focus();
        restoreFocusTo = null;
        showNext();
      });
    };
    return (
      <DesktopDialog
        dialogRef={(element) => { dialogRef = element; }}
        role="dialog"
        title={props.request.options.title}
        kind="quality"
        tone="default"
        bodyClass="min-h-[68px]"
        onDismiss={handleCancel}
        onKeyDown={handleKeyDown}
        actions={<>
          <DesktopDialogButton
            ref={(element) => { cancelRef = element; }}
            data-dialog-cancel
            onClick={handleCancel}
          >
            Cancel
          </DesktopDialogButton>
          <DesktopDialogButton
            ref={(element) => { confirmRef = element; }}
            data-dialog-confirm
            variant="secondary"
            onClick={handleSave}
          >
            Save
          </DesktopDialogButton>
        </>}
      >
        <div class="flex flex-col gap-4 p-2">
          <div class="flex items-center justify-between text-[11px] text-editor-text-dim">
            <label class="font-semibold uppercase tracking-wider">Quality</label>
            <span class="font-sans tabular-nums text-editor-text font-bold">{sliderValue()}%</span>
          </div>
          <div class="relative flex items-center h-[14px]">
            <Slider percent={sliderValue()} type="zoom" />
            <input
              type="range"
              min={1}
              max={100}
              value={sliderValue()}
              onInput={(e) => setSliderValue(parseInt(e.currentTarget.value, 10))}
              class="absolute inset-0 w-full h-[14px] opacity-0 cursor-pointer focus-visible:outline focus-visible:outline-1 focus-visible:outline-offset-2 focus-visible:outline-editor-accent"
            />
          </div>
          <p class="text-[11px] text-editor-text-dim leading-relaxed">
            {props.request.options.format === "jpeg"
              ? "Higher quality = larger file size. 90-95% recommended for photos."
              : "Higher quality = larger file size. WebP offers good compression at 80-90%."}
          </p>
        </div>
      </DesktopDialog>
    );
  }

  function ConfirmCheckboxDialogContent(props: { request: Extract<DialogRequest, { kind: "confirm-checkbox" }> }) {
    const [checked, setChecked] = createSignal(props.request.options.checkboxChecked ?? true);
    const handleCancel = () => {
      complete(false);
    };
    const handleConfirm = () => {
      props.request.resolve({ confirmed: true, checked: checked() });
      setCurrent(null);
      queueMicrotask(() => {
        restoreFocusTo?.focus();
        restoreFocusTo = null;
        showNext();
      });
    };
    return (
      <DesktopDialog
        dialogRef={(element) => { dialogRef = element; }}
        role={props.request.options.tone === "danger" ? "alertdialog" : "dialog"}
        title={props.request.options.title}
        kind="confirm-checkbox"
        tone={props.request.options.tone ?? "default"}
        bodyClass="min-h-[68px] whitespace-pre-line"
        onDismiss={handleCancel}
        onKeyDown={handleKeyDown}
        actions={<>
          <DesktopDialogButton
            ref={(element) => { cancelRef = element; }}
            data-dialog-cancel
            onClick={handleCancel}
          >
            {props.request.options.cancelLabel ?? "Cancel"}
          </DesktopDialogButton>
          <DesktopDialogButton
            ref={(element) => { confirmRef = element; }}
            data-dialog-confirm
            variant={props.request.options.tone === "danger" ? "primary" : "secondary"}
            onClick={handleConfirm}
          >
            {props.request.options.confirmLabel ?? "OK"}
          </DesktopDialogButton>
        </>}
      >
        <p class="mb-4">{props.request.options.message}</p>
        <label class="flex items-start gap-2 cursor-pointer select-none">
          <input
            type="checkbox"
            checked={checked()}
            onChange={(e) => setChecked(e.currentTarget.checked)}
            class="mt-0.5 accent-editor-accent"
          />
          <span class="text-[12px] text-editor-text leading-relaxed">
            {props.request.options.checkboxLabel}
          </span>
        </label>
      </DesktopDialog>
    );
  }

  function ConfirmSaveDialogContent(props: { request: Extract<DialogRequest, { kind: "confirm-save" }> }) {
    const handleCancel = () => {
      props.request.resolve("cancel");
      setCurrent(null);
      queueMicrotask(() => {
        restoreFocusTo?.focus();
        restoreFocusTo = null;
        showNext();
      });
    };
    const handleDiscard = () => {
      props.request.resolve("discard");
      setCurrent(null);
      queueMicrotask(() => {
        restoreFocusTo?.focus();
        restoreFocusTo = null;
        showNext();
      });
    };
    const handleSave = () => {
      props.request.resolve("save");
      setCurrent(null);
      queueMicrotask(() => {
        restoreFocusTo?.focus();
        restoreFocusTo = null;
        showNext();
      });
    };
    return (
      <DesktopDialog
        dialogRef={(element) => { dialogRef = element; }}
        role="alertdialog"
        title={props.request.options.title}
        kind="confirm-save"
        tone="default"
        bodyClass="min-h-[68px] whitespace-pre-line"
        onDismiss={handleCancel}
        onKeyDown={handleKeyDown}
        actions={<>
          <DesktopDialogButton
            ref={(element) => { cancelRef = element; }}
            data-dialog-cancel
            onClick={handleCancel}
          >
            {props.request.options.cancelLabel ?? "Cancel"}
          </DesktopDialogButton>
          <DesktopDialogButton
            data-dialog-skip
            variant="secondary"
            onClick={handleDiscard}
          >
            {props.request.options.discardLabel ?? "Don't Save"}
          </DesktopDialogButton>
          <DesktopDialogButton
            ref={(element) => { confirmRef = element; }}
            data-dialog-confirm
            variant="primary"
            onClick={handleSave}
          >
            {props.request.options.saveLabel ?? "Save"}
          </DesktopDialogButton>
        </>}
      >
        <p class="break-words [word-break:break-word]">{props.request.options.message}</p>
      </DesktopDialog>
    );
  }

  const value: DialogContextValue = { confirm, alert, quality, confirmWithCheckbox, confirmSave, colorPicker, newDocument, about };

  return (
    <DialogContext.Provider value={value}>
      {props.children}
      <Show when={current()}>
        {(request) => {
          const r = request();
          return (
            <Portal mount={document.body}>
              {(r.kind === "confirm" || r.kind === "alert") && (
                <DesktopDialog
                  dialogRef={(element) => { dialogRef = element; }}
                  role={isDangerRequest(r) ? "alertdialog" : "dialog"}
                  title={r.options.title}
                  kind={r.kind}
                  tone={isDangerRequest(r) ? "danger" : "default"}
                  bodyClass="min-h-[68px] whitespace-pre-line"
                  onDismiss={() => complete(false)}
                  onKeyDown={handleKeyDown}
                  actions={<>
                    {r.kind === "confirm" && (
                      <DesktopDialogButton
                        ref={(element) => { cancelRef = element; }}
                        data-dialog-cancel
                        onClick={() => complete(false)}
                      >
                        {cancelLabelFor(r)}
                      </DesktopDialogButton>
                    )}
                    <DesktopDialogButton
                      ref={(element) => { confirmRef = element; }}
                      data-dialog-confirm
                      variant={isDangerRequest(r) ? "primary" : "secondary"}
                      onClick={() => complete(true)}
                    >
                      {(r.options as ConfirmDialogOptions | AlertDialogOptions).confirmLabel ?? "OK"}
                    </DesktopDialogButton>
                  </>}
                >
                  <p class="break-words [word-break:break-word]">
                    {(r.options as ConfirmDialogOptions | AlertDialogOptions).message}
                  </p>
                </DesktopDialog>
              )}
              {r.kind === "quality" && (
                <QualityDialogContent
                  request={r as Extract<DialogRequest, { kind: "quality" }>}
                />
              )}
              {r.kind === "confirm-checkbox" && (
                <ConfirmCheckboxDialogContent
                  request={r as Extract<DialogRequest, { kind: "confirm-checkbox" }>}
                />
              )}
              {r.kind === "confirm-save" && (
                <ConfirmSaveDialogContent
                  request={r as Extract<DialogRequest, { kind: "confirm-save" }>}
                />
              )}
              {r.kind === "color-picker" && (
                <ColorPickerDialogContent
                  request={r as Extract<DialogRequest, { kind: "color-picker" }>}
                />
              )}
              {r.kind === "new-document" && (
                <NewDocumentDialogContent
                  request={r as Extract<DialogRequest, { kind: "new-document" }>}
                  onClose={(result) => {
                    r.resolve(result);
                    setCurrent(null);
                    queueMicrotask(() => {
                      restoreFocusTo?.focus();
                      restoreFocusTo = null;
                      showNext();
                    });
                  }}
                  dialogRef={(element) => { dialogRef = element; }}
                  onKeyDown={handleKeyDown}
                />
              )}
              {r.kind === "about" && (
                <AboutDialog
                  onDismiss={() => {
                    r.resolve();
                    setCurrent(null);
                    queueMicrotask(() => {
                      restoreFocusTo?.focus();
                      restoreFocusTo = null;
                      showNext();
                    });
                  }}
                />
              )}
            </Portal>
          );
        }}
      </Show>
    </DialogContext.Provider>
  );
}
