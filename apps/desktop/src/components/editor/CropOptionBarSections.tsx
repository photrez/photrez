// SPDX-License-Identifier: AGPL-3.0-or-later
// Section sub-components for CropOptionBar.
// Extracted from CropOptionBar.tsx (report #20 phase 3) — behavior must stay
// identical to the inline JSX it replaces. Each component is presentational:
// all state lives in CropOptionBar and is passed via accessor props.

import { For, Show } from "solid-js";
import { EditableNumField, Slider } from "./primitives";
import { OptionCheckbox, Divider, MoreDropdown } from "./shell/OptionBarShared";
import { Tooltip } from "./Tooltip";
import { Icon } from "./icons";
import { CROP_PRESETS } from "@/viewport/cropPresets";

// ─── Type guards (moved from CropOptionBar) ───

// known union. Avoids three separate `as any` casts scattered through the
// component — one place to extend when new unit/mode values appear.
export function isCropSizeUnit(v: string): v is "px" | "cm" | "mm" | "in" {
  return v === "px" || v === "cm" || v === "mm" || v === "in";
}
export function isCropGuideMode(
  v: string,
): v is "none" | "thirds" | "grid" | "diagonal" | "golden" {
  return (
    v === "none" ||
    v === "thirds" ||
    v === "grid" ||
    v === "diagonal" ||
    v === "golden"
  );
}

// ─── Rotation controls (inline + menu layouts) ───

export function CropStraightenControl(props: {
  rotation: () => number;
  onCommitRotation: (v: number) => void;
  onResetRotation: () => void;
  layout: "inline" | "menu";
}) {
  const rotation = () => props.rotation();
  if (props.layout === "menu") {
    return (
      <div class="flex flex-col gap-1.5">
        <span class="text-[10px] font-bold text-editor-text-dim uppercase tracking-wider">
          Straighten
        </span>
        <div class="flex items-center gap-2">
          <div class="relative flex h-[18px] flex-1 items-center">
            <Slider
              percent={Math.min(100, Math.max(0, (rotation() + 45) / 90) * 100)}
              value={rotation()}
              type="straighten"
            />
            <input
              type="range"
              min="-45"
              max="45"
              step="0.1"
              value={rotation()}
              onInput={(e) =>
                props.onCommitRotation(parseFloat(e.currentTarget.value))
              }
              onDblClick={props.onResetRotation}
              class="absolute inset-0 h-[18px] w-full cursor-pointer opacity-0"
            />
          </div>
          <span class="w-[34px] text-right text-[10px] tabular-nums text-editor-text">
            {rotation().toFixed(1)}°
          </span>
        </div>
      </div>
    );
  }
  return (
    <div class="flex items-center gap-1.5">
      <Tooltip content="Straighten / rotate">
        <span class="text-[10px] text-editor-text-dim select-none uppercase tracking-wide">
          Straighten
        </span>
      </Tooltip>
      <div class="relative flex h-[18px] w-[88px] items-center">
        <Slider
          percent={Math.min(100, Math.max(0, (rotation() + 45) / 90) * 100)}
          value={rotation()}
          type="straighten"
        />
        <input
          type="range"
          min="-45"
          max="45"
          step="0.1"
          value={rotation()}
          onInput={(e) =>
            props.onCommitRotation(parseFloat(e.currentTarget.value))
          }
          onDblClick={props.onResetRotation}
          class="absolute inset-0 h-[18px] w-full cursor-pointer opacity-0"
        />
      </div>
      <span class="w-[34px] text-right text-[10px] tabular-nums text-editor-text">
        {rotation().toFixed(1)}°
      </span>
    </div>
  );
}

export function CropRotateButtons(props: {
  onRotate: (delta: number) => void;
  layout: "inline" | "menu";
}) {
  if (props.layout === "menu") {
    return (
      <div class="flex flex-col gap-1.5 mt-1.5">
        <span class="text-[10px] font-bold text-editor-text-dim uppercase tracking-wider">
          Rotate
        </span>
        <div class="flex items-center h-[26px] bg-editor-field rounded-[4px] border border-editor-field-border p-[1px]">
          <Tooltip content="Rotate 90° CCW">
            <button
              onClick={() => props.onRotate(-90)}
              class="flex-1 flex items-center justify-center h-full rounded-[2px] text-editor-icon hover:bg-editor-hover hover:text-white transition-colors"
              aria-label="Rotate 90 degrees counter-clockwise"
            >
              <Icon name="rotate-ccw" class="size-[14px]" strokeWidth={2} />
            </button>
          </Tooltip>
          <div class="w-px h-3.5 bg-editor-field-border mx-[1px]" />
          <Tooltip content="Rotate 90° CW">
            <button
              onClick={() => props.onRotate(90)}
              class="flex-1 flex items-center justify-center h-full rounded-[2px] text-editor-icon hover:bg-editor-hover hover:text-white transition-colors"
              aria-label="Rotate 90 degrees clockwise"
            >
              <Icon name="rotate-cw" class="size-[14px]" strokeWidth={2} />
            </button>
          </Tooltip>
        </div>
      </div>
    );
  }
  return (
    <div class="flex items-center gap-1">
      <Tooltip content="Rotate 90° CCW">
        <button
          onClick={() => props.onRotate(-90)}
          class="flex size-[24px] shrink-0 items-center justify-center rounded-[3px] border border-transparent text-editor-icon hover:border-editor-field-border hover:text-editor-text transition-colors"
          aria-label="Rotate 90 degrees counter-clockwise"
        >
          <Icon name="rotate-ccw" class="size-4" strokeWidth={1.5} />
        </button>
      </Tooltip>
      <Tooltip content="Rotate 90° CW">
        <button
          onClick={() => props.onRotate(90)}
          class="flex size-[24px] shrink-0 items-center justify-center rounded-[3px] border border-transparent text-editor-icon hover:border-editor-field-border hover:text-editor-text transition-colors"
          aria-label="Rotate 90 degrees clockwise"
        >
          <Icon name="rotate-cw" class="size-4" strokeWidth={1.5} />
        </button>
      </Tooltip>
    </div>
  );
}

export function CropGuideSelect(props: {
  guideMode: () => string;
  onGuideModeChange: (v: string) => void;
  label: () => string;
  layout: "inline" | "menu";
}) {
  const options = [
    { value: "none", label: "None" },
    { value: "thirds", label: "Thirds" },
    { value: "grid", label: "Grid" },
    { value: "diagonal", label: "Diagonal" },
    { value: "golden", label: "Golden" },
  ];
  const select = (
    <select
      value={props.guideMode()}
      onChange={(e) => {
        const v = e.currentTarget.value;
        if (isCropGuideMode(v)) props.onGuideModeChange(v);
      }}
      class="absolute inset-0 h-full w-full opacity-0 cursor-pointer text-[11px]"
    >
      <For each={options}>
        {(opt) => (
          <option value={opt.value} class="bg-editor-panel text-editor-text">
            {opt.label}
          </option>
        )}
      </For>
    </select>
  );
  if (props.layout === "menu") {
    return (
      <div class="flex flex-col gap-1.5 mt-1.5">
        <span class="text-[10px] font-bold text-editor-text-dim uppercase tracking-wider">
          Guide
        </span>
        <div class="relative flex h-[24px] w-full items-center rounded-[3px] border border-editor-field-border bg-editor-field px-2 hover:border-editor-field-border/80 transition-all cursor-pointer focus-ring-within">
          <span class="text-[11px] text-editor-text mr-4 select-none">
            {props.label()}
          </span>
          <div class="ml-auto pointer-events-none text-editor-text-dim">
            <Icon name="chevron-down" class="size-3" strokeWidth={1.5} />
          </div>
          {select}
        </div>
      </div>
    );
  }
  return (
    <div class="relative flex h-[24px] shrink-0 items-center rounded-[3px] border border-editor-field-border bg-editor-field px-2 hover:border-editor-field-border/80 transition-all cursor-pointer focus-ring-within">
      <span class="text-[11px] text-editor-text mr-4 select-none">
        {props.label()}
      </span>
      <div class="ml-auto pointer-events-none text-editor-text-dim">
        <Icon name="chevron-down" class="size-3" strokeWidth={1.5} />
      </div>
      {select}
    </div>
  );
}

export function CropClassicToggle(props: {
  checked: boolean;
  onChange: (v: boolean) => void;
  layout: "inline" | "menu";
}) {
  const checkbox = (
    <OptionCheckbox
      checked={props.checked}
      onChange={props.onChange}
      label="Classic"
      class={
        props.layout === "menu"
          ? "w-full bg-editor-field/50 border-editor-field-border"
          : undefined
      }
    />
  );
  if (props.layout === "menu") {
    return (
      <Tooltip content="Classic crop: draw a rectangle on the image (resizes canvas)">
        <div class="flex items-center -mx-1.5 px-1.5">{checkbox}</div>
      </Tooltip>
    );
  }
  return (
    <Tooltip content="Classic crop: draw a rectangle on the image (resizes canvas)">
      {checkbox}
    </Tooltip>
  );
}

// ─── Ratio picker (dropdown + lock + recents + presets) ───

export function CropRatioPicker(props: {
  ratioLabel: () => string;
  open: () => boolean;
  setOpen: (v: boolean) => void;
  cropMode: () => string;
  recents: () => { w: number; h: number }[];
  onLockShape: () => void;
  onFree: () => void;
  onCustom: () => void;
  onSize: () => void;
  onRecent: (r: { w: number; h: number }) => void;
  isCustomActive: () => boolean;
  isActivePill: (p: { w: number; h: number }) => boolean;
  onPillClick: (p: { w: number; h: number }) => void;
}) {
  return (
    <div class="relative">
      <button
        type="button"
        onClick={() => props.setOpen(!props.open())}
        class="flex h-[24px] shrink-0 items-center gap-1.5 rounded-[3px] border border-editor-field-border bg-editor-field px-2 text-[11px] text-editor-text hover:border-editor-accent transition-colors cursor-pointer whitespace-nowrap"
      >
        <span>Ratio: {props.ratioLabel()}</span>
        <Icon name="chevron-down" class="size-3 text-editor-text-dim shrink-0" />
      </button>

      <Show when={props.open()}>
        <div class="absolute top-full left-0 z-50 mt-1 flex flex-col rounded-[4px] border border-editor-field-border bg-editor-panel py-1 shadow-lg max-h-[300px] overflow-y-auto min-w-[150px]">
          <div
            class="fixed inset-0 z-[-1]"
            onClick={() => props.setOpen(false)}
          />

          {/* Lock Current Shape Option */}
          <button
            type="button"
            disabled={props.cropMode() !== "free"}
            class={`flex items-center gap-2 px-3 py-1.5 text-[11px] text-left hover:bg-editor-field/60 disabled:opacity-40 disabled:pointer-events-none text-editor-text`}
            onClick={() => {
              props.onLockShape();
              props.setOpen(false);
            }}
          >
            <Icon name="lock" class="size-3" strokeWidth={1.5} />
            <span>Lock Current Shape</span>
          </button>

          <div class="h-px bg-editor-divider my-1" />

          {/* Recents list if available */}
          <Show when={props.recents().length > 0}>
            <div class="px-3 py-0.5 text-[9px] font-bold text-editor-text-dim uppercase tracking-wider">
              Recents
            </div>
            <For each={props.recents()}>
              {(r) => (
                <button
                  type="button"
                  class="flex items-center justify-between px-3 py-1 text-[11px] text-editor-text hover:bg-editor-field/60 text-left w-full"
                  onClick={() => {
                    props.onRecent(r);
                    props.setOpen(false);
                  }}
                >
                  <span>
                    {r.w}:{r.h}
                  </span>
                </button>
              )}
            </For>
            <div class="h-px bg-editor-divider my-1" />
          </Show>

          {/* Standard Options */}
          <button
            type="button"
            class={`flex items-center px-3 py-1.5 text-[11px] hover:bg-editor-field/60 text-left w-full ${props.cropMode() === "free" ? "text-editor-accent font-medium" : "text-editor-text"}`}
            onClick={() => {
              props.onFree();
              props.setOpen(false);
            }}
          >
            Free
          </button>
          <button
            type="button"
            class={`flex items-center px-3 py-1.5 text-[11px] hover:bg-editor-field/60 text-left w-full ${props.isCustomActive() ? "text-editor-accent font-medium" : "text-editor-text"}`}
            onClick={() => {
              props.onCustom();
              props.setOpen(false);
            }}
          >
            Custom...
          </button>
          <button
            type="button"
            class={`flex items-center px-3 py-1.5 text-[11px] hover:bg-editor-field/60 text-left w-full ${props.cropMode() === "size" ? "text-editor-accent font-medium" : "text-editor-text"}`}
            onClick={() => {
              props.onSize();
              props.setOpen(false);
            }}
          >
            Size
          </button>

          <div class="h-px bg-editor-divider my-1" />

          <div class="px-3 py-0.5 text-[9px] font-bold text-editor-text-dim uppercase tracking-wider">
            Presets
          </div>
          <For each={CROP_PRESETS}>
            {(preset) => (
              <button
                type="button"
                class={`flex items-center px-3 py-1.5 text-[11px] hover:bg-editor-field/60 text-left w-full ${props.isActivePill(preset.aspect) ? "text-editor-accent font-medium" : "text-editor-text"}`}
                onClick={() => {
                  props.onPillClick(preset.aspect);
                  props.setOpen(false);
                }}
              >
                {preset.label}
              </button>
            )}
          </For>
        </div>
      </Show>
    </div>
  );
}

// ─── W/H inputs (ratio mode) + swap ───

export function CropRatioInputs(props: {
  w: () => number;
  h: () => number;
  onWSubmit: (v: number) => void;
  onHSubmit: (v: number) => void;
  onSwap: () => void;
}) {
  return (
    <div class="flex shrink-0 items-center gap-1">
      <EditableNumField
        label="W"
        value={props.w()}
        onSubmit={props.onWSubmit}
        class="w-[62px]"
      />
      <SwapButton onSwap={props.onSwap} />
      <EditableNumField
        label="H"
        value={props.h()}
        onSubmit={props.onHSubmit}
        class="w-[62px]"
      />
    </div>
  );
}

export function CropSizeInputs(props: {
  w: () => number;
  h: () => number;
  unit: () => string;
  onWSubmit: (v: number) => void;
  onHSubmit: (v: number) => void;
  onSwap: () => void;
  onUnitChange: (v: string) => void;
}) {
  return (
    <div class="flex shrink-0 items-center gap-1.5">
      <EditableNumField
        label="W"
        suffix={props.unit()}
        value={props.w()}
        onSubmit={props.onWSubmit}
        class="w-[68px]"
      />
      <SwapButton onSwap={props.onSwap} />
      <EditableNumField
        label="H"
        suffix={props.unit()}
        value={props.h()}
        onSubmit={props.onHSubmit}
        class="w-[68px]"
      />

      {/* Unit Selector */}
      <div class="relative flex h-[24px] shrink-0 items-center rounded-[3px] border border-editor-field-border bg-editor-field px-2 hover:border-editor-field-border/80 transition-all cursor-pointer focus-ring-within">
        <span class="text-[11px] text-editor-text mr-4 select-none">
          {props.unit()}
        </span>
        <div class="ml-auto pointer-events-none text-editor-text-dim">
          <Icon name="chevron-down" class="size-3" strokeWidth={1.5} />
        </div>
        <select
          value={props.unit()}
          onChange={(e) => {
            const v = e.currentTarget.value;
            if (isCropSizeUnit(v)) props.onUnitChange(v);
          }}
          class="absolute inset-0 h-full w-full opacity-0 cursor-pointer text-[11px]"
        >
          <option value="px" class="bg-editor-panel text-editor-text">
            px
          </option>
          <option value="cm" class="bg-editor-panel text-editor-text">
            cm
          </option>
          <option value="mm" class="bg-editor-panel text-editor-text">
            mm
          </option>
          <option value="in" class="bg-editor-panel text-editor-text">
            in
          </option>
        </select>
      </div>
    </div>
  );
}

function SwapButton(props: { onSwap: () => void }) {
  return (
    <Tooltip content="Swap Width/Height">
      <button
        type="button"
        onClick={props.onSwap}
        class="flex size-[20px] shrink-0 items-center justify-center rounded-[3px] border border-transparent text-editor-icon hover:border-editor-field-border hover:text-editor-text transition-colors cursor-pointer"
        aria-label="Swap width and height"
      >
        <Icon name="swap" class="size-3.5" strokeWidth={1.5} />
      </button>
    </Tooltip>
  );
}

// ─── Fill controls (delete / fill BG + color) ───

export function CropFillControls(props: {
  deletePixels: () => boolean;
  onDeletePixelsChange: (v: boolean) => void;
  fillEnabled: () => boolean;
  onFillEnabledChange: (v: boolean) => void;
  fillSource: () => string;
  fillColor: () => string;
  onPickColor: (v: string) => void;
  onUseBackground: () => void;
}) {
  return (
    <>
      {/* Delete pixels toggle (always visible, checkbox) */}
      <Tooltip
        content={
          props.deletePixels()
            ? "Delete Cropped Pixels (Destructive)"
            : "Keep Cropped Pixels (Non-Destructive)"
        }
      >
        <OptionCheckbox
          checked={props.deletePixels()}
          onChange={props.onDeletePixelsChange}
          label="Delete"
        />
      </Tooltip>

      {/* Fill BG toggle (always visible, checkbox) + color */}
      <Tooltip
        content={
          props.fillEnabled()
            ? "Fill empty crop areas"
            : "Leave empty crop areas transparent"
        }
      >
        <OptionCheckbox
          checked={props.fillEnabled()}
          onChange={props.onFillEnabledChange}
          label="Fill BG"
        />
      </Tooltip>

      <Show when={props.fillEnabled()}>
        <div
          class="flex h-[24px] shrink-0 items-center gap-1 rounded-[3px] border border-editor-field-border bg-editor-field px-1"
          data-crop-fill-source={props.fillSource()}
        >
          <Tooltip content="Crop fill color">
            <input
              data-crop-fill-color
              type="color"
              value={props.fillColor()}
              onInput={(e) => props.onPickColor(e.currentTarget.value)}
              class="h-[18px] w-[22px] cursor-pointer rounded-[2px] border border-editor-field-border bg-transparent p-0"
            />
          </Tooltip>
          <Tooltip content="Use Background Color">
            <button
              data-crop-fill-use-bg
              type="button"
              onClick={props.onUseBackground}
              class="h-[18px] rounded-[2px] px-1.5 text-[10px] text-editor-text-dim hover:bg-editor-hover hover:text-editor-text"
            >
              Use BG
            </button>
          </Tooltip>
        </div>
      </Show>
    </>
  );
}

export { Divider, MoreDropdown };
