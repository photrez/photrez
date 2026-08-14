// SPDX-License-Identifier: AGPL-3.0-or-later
import { Show, For, createSignal, createMemo } from "solid-js";
import { clsx } from "clsx";
import { useEditor } from "./shell/EditorContext";
import { useDialog } from "./dialogs/DialogProvider";
import { ToolPill, Divider } from "./shell/OptionBarShared";
import { Tooltip } from "./Tooltip";
import { Icon, type IconName } from "./icons";
import type { LayerNode, ShapeParams, ShapeKind } from "@/engine/types";

type ShapeLayer = LayerNode & { type: "shape"; shapeParams: ShapeParams };

/** Field-by-field no-op check: skips commit when the edit is a no-op (avoids
 *  ghost undo entries on repeated range/color input events). Mirrors
 *  MoveOptionBar's transform no-op guard. */
function shallowEqualParams(cur: ShapeParams, next: Partial<ShapeParams>): boolean {
  if (next.kind !== undefined && next.kind !== cur.kind) return false;
  if (next.radius !== undefined && next.radius !== cur.radius) return false;
  if (next.arrowHead !== undefined && next.arrowHead !== cur.arrowHead) return false;
  if (next.stroke) {
    if (next.stroke.enabled !== undefined && next.stroke.enabled !== cur.stroke.enabled) return false;
    if (next.stroke.color !== undefined && next.stroke.color !== cur.stroke.color) return false;
    if (next.stroke.width !== undefined && next.stroke.width !== cur.stroke.width) return false;
  }
  if (next.fill) {
    if (next.fill.kind !== undefined && next.fill.kind !== cur.fill.kind) return false;
    if (next.fill.color !== undefined && next.fill.color !== cur.fill.color) return false;
  }
  return true;
}

const SHAPE_PRESETS: { kind: ShapeKind; arrow: boolean; icon: IconName; label: string; content: string }[] = [
  { kind: "rect", arrow: false, icon: "rectangle", label: "Rectangle", content: "Solid/filled rectangle" },
  { kind: "ellipse", arrow: false, icon: "circle", label: "Ellipse", content: "Solid/filled circle or ellipse" },
  { kind: "triangle", arrow: false, icon: "triangle", label: "Triangle", content: "Solid/filled triangle" },
  { kind: "star", arrow: false, icon: "star", label: "Star (5-Point)", content: "Solid/filled 5-point star" },
  { kind: "block-arrow", arrow: false, icon: "block-arrow", label: "Block Arrow", content: "Filled 2D block arrow" },
  { kind: "heart", arrow: false, icon: "heart", label: "Heart", content: "Solid/filled heart shape" },
  { kind: "diamond", arrow: false, icon: "diamond", label: "Diamond", content: "Solid/filled diamond shape" },
  { kind: "speech-bubble", arrow: false, icon: "speech-bubble", label: "Speech Bubble", content: "Solid/filled speech bubble" },
  { kind: "hexagon", arrow: false, icon: "hexagon", label: "Hexagon", content: "Solid/filled 6-sided hexagon" },
  { kind: "line", arrow: false, icon: "line", label: "Line", content: "Straight line" },
  { kind: "line", arrow: true, icon: "arrowUpRight", label: "Arrow", content: "Line with arrow head" },
];

/**
 * Shape option bar — TWO modes:
 * - draw mode (shape tool active, no shape node selected): writes the shape
 *   signals used for the NEXT created shape (deterministic session defaults).
 * - edit mode (a shape layer is selected, any active tool): binds live
 *   params and calls engine.updateShapeParams (commit BEFORE mutation).
 * Research pain point: controls must NOT disappear once the shape tool is
 * deselected, otherwise existing shapes are uneditable.
 */
export function ShapeOptionBar() {
  const {
    workspace,
    scheduler,
    renderer,
    layers,
    selectedLayerId,
    fgColor, setFgColor,
    shapeKind, setShapeKind,
    shapeFillEnabled, setShapeFillEnabled,
    shapeStrokeEnabled, setShapeStrokeEnabled,
    shapeStrokeColor, setShapeStrokeColor,
    shapeStrokeWidth, setShapeStrokeWidth,
    shapeRadius, setShapeRadius,
    shapeArrowHead, setShapeArrowHead,
    colorPickerOpen, setColorPickerOpen,
    colorPickerTarget, setColorPickerTarget,
  } = useEditor();
  const dialogs = useDialog();

  // Read through the reactive `layers()` signal: setupWorkspaceSync re-publishes
  // the layer array on every engine change (updateShapeParams → notifyChange),
  // so the label/kind derive happens on an actual signal — not a stale live read.
  const selectedShape = (): ShapeLayer | null => {
    const id = selectedLayerId();
    if (!id) return null;
    const layer = layers().find((l) => l.id === id);
    // guard: the layer could be a raster/group, or lack shapeParams
    return layer && layer.type === "shape" && layer.shapeParams ? (layer as ShapeLayer) : null;
  };

  const isEditMode = () => !!selectedShape();
  const shape = () => selectedShape()!;

  const kind = () => (isEditMode() ? shape().shapeParams.kind : shapeKind());
  const fillEnabled = () => (isEditMode() ? shape().shapeParams.fill.kind === "solid" : shapeFillEnabled());
  const fillColor = () => (isEditMode() ? shape().shapeParams.fill.color : fgColor());
  const strokeEnabled = () => (isEditMode() ? shape().shapeParams.stroke.enabled : shapeStrokeEnabled());
  const strokeColor = () => (isEditMode() ? shape().shapeParams.stroke.color : shapeStrokeColor());
  const strokeWidthValue = () => (isEditMode() ? shape().shapeParams.stroke.width : shapeStrokeWidth());
  const radius = () => (isEditMode() ? shape().shapeParams.radius : shapeRadius());
  const arrowHead = () => (isEditMode() ? shape().shapeParams.arrowHead : shapeArrowHead());

  const applyEdit = (patch: Partial<ShapeParams>) => {
    const engine = workspace.getActiveEngine();
    const history = workspace.getActiveHistory();
    const layer = selectedShape();
    if (!layer || !engine || !history) return;
    const next = { ...layer.shapeParams, ...patch };
    // No-op guard: skip commit when the patch doesn't change the current
    // params — prevents ghost undo entries from range/color per-tick input
    // events and from clicking a control at its current value.
    if (shallowEqualParams(layer.shapeParams, next)) return;
    // commit BEFORE mutation (AGENTS.md wiring rule)
    history.commit(engine.snapshot(), "Edit Shape");
    engine.updateShapeParams(layer.id, next);
    const bitmap = typeof engine.getLayerImageBitmap === "function" ? engine.getLayerImageBitmap(layer.id) : null;
    if (bitmap) renderer?.uploadImage(layer.id, bitmap);
    scheduler?.requestRender();
  };

  const selectKind = (next: ShapeKind, arrow: boolean) => {
    setShapeKind(next);
    if (next === "line") setShapeArrowHead(arrow);
    if (isEditMode()) {
      applyEdit({ kind: next, arrowHead: next === "line" ? arrow : false });
    }
  };

  const setFill = (on: boolean) => {
    // Audit 3.1: never allow both fill and stroke disabled — that yields an
    // invisible shape (still hittable, confuses the layer list). Block turning
    // fill off while stroke is already off; the last paint stays on.
    if (!on && !strokeEnabled()) return;
    setShapeFillEnabled(on);
    if (isEditMode()) applyEdit({ fill: { ...shape().shapeParams.fill, kind: on ? "solid" : "none" } });
  };

  const setFillColor = (c: string) => {
    setFgColor(c);
    if (isEditMode()) applyEdit({ fill: { ...shape().shapeParams.fill, color: c, kind: "solid" } });
  };

  const handleOpenFillColorPicker = async () => {
    setColorPickerOpen(true);
    setColorPickerTarget("foreground");
    const chosen = await dialogs.colorPicker({
      title: "Shape Fill Color",
      initialColor: fillColor(),
      target: "foreground",
      onChange: (c) => setFillColor(c),
    });
    if (chosen) {
      setFillColor(chosen);
    }
    setColorPickerOpen(false);
  };

  const setStroke = (on: boolean) => {
    // Audit 3.1: never allow both fill and stroke disabled — see setFill guard.
    if (!on && !fillEnabled()) return;
    setShapeStrokeEnabled(on);
    if (isEditMode()) applyEdit({ stroke: { ...shape().shapeParams.stroke, enabled: on } });
  };

  const setColor = (color: string) => {
    setShapeStrokeColor(color);
    if (isEditMode()) applyEdit({ stroke: { ...shape().shapeParams.stroke, color, enabled: true } });
  };

  const handleOpenStrokeColorPicker = async () => {
    setColorPickerOpen(true);
    setColorPickerTarget("foreground");
    const chosen = await dialogs.colorPicker({
      title: "Shape Stroke Color",
      initialColor: strokeColor(),
      target: "foreground",
      onChange: (c) => setColor(c),
    });
    if (chosen) {
      setColor(chosen);
    }
    setColorPickerOpen(false);
  };

  const setWidth = (width: number) => {
    const v = Math.max(1, width);
    setShapeStrokeWidth(v);
    if (isEditMode()) applyEdit({ stroke: { ...shape().shapeParams.stroke, width: v } });
  };

  const setRadius = (value: number) => {
    const v = Math.max(0, Math.min(100, value));
    setShapeRadius(v);
    if (isEditMode()) applyEdit({ radius: v });
  };

  const setArrow = (on: boolean) => {
    setShapeArrowHead(on);
    if (isEditMode()) applyEdit({ arrowHead: on });
  };

  const [strokePopoverOpen, setStrokePopoverOpen] = createSignal(false);
  const [shapePickerOpen, setShapePickerOpen] = createSignal(false);

  const activePreset = createMemo(() => {
    const k = kind();
    const ah = arrowHead();
    return SHAPE_PRESETS.find((p) => p.kind === k && (p.kind !== "line" || p.arrow === ah)) ?? SHAPE_PRESETS[0];
  });

  return (
    <div data-shape-option-bar class="flex items-center gap-1.5 text-[11px] select-none">
      <ToolPill icon="rectangle" label="Shape" />

      <Divider />

      {/* Shape Selector Dropdown */}
      <div class="relative shrink-0 select-none">
        <button
          type="button"
          aria-label="Select shape"
          aria-expanded={shapePickerOpen()}
          onClick={() => setShapePickerOpen(!shapePickerOpen())}
          class="flex h-[24px] cursor-pointer items-center gap-1.5 rounded-[4px] border border-editor-field-border bg-editor-field px-2 text-[11px] font-medium text-white transition-colors hover:border-[#4B515D]"
        >
          <Icon name={activePreset().icon} class="size-3.5 text-editor-accent" strokeWidth={1.6} />
          <span class="font-semibold">{activePreset().label}</span>
          <Icon name="chevron-down" class="size-3 text-[#A1A1AA]" strokeWidth={1.6} />
        </button>

        <Show when={shapePickerOpen()}>
          <div
            class="fixed inset-0 z-50"
            onClick={() => setShapePickerOpen(false)}
          />
          <div class="absolute left-0 top-full z-51 mt-1 w-[160px] max-h-[260px] overflow-y-auto rounded-[6px] border border-editor-field-border bg-editor-bg p-1 shadow-xl">
            <For each={SHAPE_PRESETS}>
              {(preset) => (
                <button
                  type="button"
                  aria-label={preset.label}
                  onClick={() => {
                    selectKind(preset.kind, preset.arrow);
                    setShapePickerOpen(false);
                  }}
                  class={clsx(
                    "flex w-full cursor-pointer items-center gap-2 rounded-[4px] px-2 py-1.5 text-left text-[11px] font-medium transition-colors select-none",
                    activePreset().label === preset.label
                      ? "bg-editor-accent/20 text-white font-semibold"
                      : "text-[#A1A1AA] hover:bg-editor-field hover:text-white"
                  )}
                >
                  <Icon name={preset.icon} class={clsx("size-3.5", activePreset().label === preset.label ? "text-editor-accent" : "text-[#A1A1AA]")} strokeWidth={1.6} />
                  <span>{preset.label}</span>
                </button>
              )}
            </For>
          </div>
        </Show>
      </div>

      <Divider />

      {/* Fill Controls */}
      <div class="flex items-center gap-1 select-none">
        <button
          type="button"
          aria-label="Fill"
          aria-pressed={fillEnabled()}
          onClick={() => setFill(!fillEnabled())}
          class={clsx(
            "flex h-[24px] items-center gap-1 rounded-[4px] border px-2 text-[11px] font-medium transition-colors cursor-pointer",
            fillEnabled()
              ? "border-editor-accent bg-editor-accent/20 text-white font-semibold shadow-xs"
              : "border-editor-field-border bg-editor-field text-[#A1A1AA] hover:border-[#4B515D] hover:text-white"
          )}
        >
          <span>Fill</span>
        </button>
        <Show when={fillEnabled()}>
          <Tooltip content="Fill color" placement="top">
            <button
              type="button"
              aria-label="Fill color"
              onClick={handleOpenFillColorPicker}
              class="size-[22px] shrink-0 cursor-pointer rounded-[3px] border border-editor-field-border p-0 transition-transform hover:scale-105 ring-1 ring-white/20"
              style={{ "background-color": fillColor() }}
            />
          </Tooltip>
        </Show>
      </div>

      <Divider />

      {/* Stroke Pill & Popover (Matching TextOptionBar) */}
      <div class="relative flex items-center select-none" data-shape-stroke>
        <Tooltip content="Stroke outline options" placement="top">
          <button
            type="button"
            aria-label="Stroke options"
            aria-expanded={strokePopoverOpen()}
            onClick={() => setStrokePopoverOpen(!strokePopoverOpen())}
            class={clsx(
              "group flex h-[24px] w-[116px] shrink-0 items-center justify-between gap-1.5 rounded-[4px] border px-2 text-[11px] font-medium transition-colors duration-75 select-none cursor-pointer",
              strokeEnabled() && strokeWidthValue() > 0
                ? "border-editor-accent bg-editor-accent/20 text-white shadow-xs font-semibold"
                : "border-editor-field-border bg-editor-field text-[#A1A1AA] hover:border-[#4B515D] hover:bg-editor-field/80 hover:text-white",
            )}
          >
            <Show
              when={strokeEnabled() && strokeWidthValue() > 0}
              fallback={
                <div class="flex items-center gap-1.5">
                  <span class="size-2.5 shrink-0 rounded-full border border-[#363B44] bg-[#2A2E37]" />
                  <span class="text-[#A1A1AA] group-hover:text-white font-medium transition-colors">Stroke:</span>
                  <span class="inline-block min-w-[34px] font-mono text-[#A1A1AA] font-medium text-left">Off</span>
                </div>
              }
            >
              <div class="flex items-center gap-1.5">
                <span
                  class="size-2.5 shrink-0 rounded-full border border-black/50 ring-1 ring-white/30 shadow-2xs"
                  style={{ background: strokeColor() }}
                />
                <span class="text-[#A1A1AA] font-medium">Stroke:</span>
                <span class="inline-block min-w-[34px] font-mono text-white font-bold text-left">{strokeWidthValue()}px</span>
              </div>
            </Show>
            <Icon name="chevron-down" class="size-3 text-[#A1A1AA] group-hover:text-white shrink-0 transition-colors" strokeWidth={1.75} />
          </button>
        </Tooltip>

        <Show when={strokePopoverOpen()}>
          <div
            class="absolute left-0 top-full mt-1.5 z-50 w-56 rounded-[6px] border border-[#363B44] bg-[#1D2026] p-3 shadow-2xl select-none"
            onPointerDown={(e) => e.stopPropagation()}
          >
            <div class="flex items-center justify-between pb-2 mb-2 border-b border-[#2D323C]">
              <span class="text-[11px] font-semibold text-white">Stroke Outline</span>
              <label class="flex items-center gap-1.5 cursor-pointer">
                <input
                  type="checkbox"
                  aria-label="Stroke"
                  checked={strokeEnabled()}
                  onChange={(e) => setStroke(e.currentTarget.checked)}
                  class="accent-editor-accent cursor-pointer"
                />
                <span class="text-[11px] font-medium text-white">{strokeEnabled() ? "On" : "Off"}</span>
              </label>
            </div>

            <div
              class="relative cursor-pointer"
              onClick={() => {
                if (!strokeEnabled()) setStroke(true);
              }}
            >
              <div
                class={clsx(
                  "flex flex-col gap-2.5 transition-all duration-150",
                  !strokeEnabled() && "opacity-40 filter grayscale pointer-events-none"
                )}
              >
                <div class="flex items-center justify-between">
                  <span class="text-[10px] text-[#A1A1AA] font-medium">Color</span>
                  <button
                    type="button"
                    aria-label="Stroke color"
                    onClick={handleOpenStrokeColorPicker}
                    class="flex items-center gap-2 rounded-[4px] border border-editor-field-border bg-editor-field px-2 py-1 transition-colors hover:border-[#4B515D] cursor-pointer"
                  >
                    <span
                      class="size-3 rounded-full border border-black/50 ring-1 ring-white/30"
                      style={{ background: strokeColor() }}
                    />
                    <span class="font-mono text-[10px] text-white font-medium">{strokeColor().toUpperCase()}</span>
                  </button>
                </div>

                <div class="flex items-center justify-between">
                  <span class="text-[10px] text-[#A1A1AA] font-medium">Width</span>
                  <div class="flex h-[24px] items-center gap-1 rounded-[4px] border border-editor-field-border bg-editor-field px-1.5 focus-within:border-editor-accent focus-within:ring-1 focus-within:ring-editor-accent/70 hover:border-[#4B515D]">
                    <input
                      type="number"
                      aria-label="Stroke width"
                      min={1}
                      max={200}
                      value={strokeWidthValue()}
                      onInput={(e) => setWidth(Number(e.currentTarget.value))}
                      class="w-[36px] bg-transparent text-center font-mono text-[11px] font-semibold text-white outline-none"
                    />
                    <span class="text-[10px] font-medium text-[#A1A1AA] select-none">px</span>
                  </div>
                </div>
              </div>
            </div>
          </div>
          <div class="fixed inset-0 z-40" onClick={() => setStrokePopoverOpen(false)} />
        </Show>
      </div>

      <Show when={kind() === "rect"}>
        <Divider />
        <div class="flex h-[24px] items-center gap-1 rounded-[4px] border border-editor-field-border bg-editor-field px-1.5 focus-within:border-editor-accent focus-within:ring-1 focus-within:ring-editor-accent/70 hover:border-[#4B515D]">
          <span class="text-[10px] font-medium text-[#A1A1AA] select-none">Radius</span>
          <input
            type="number"
            aria-label="Corner radius"
            min={0}
            max={500}
            value={radius()}
            onInput={(e) => setRadius(Number(e.currentTarget.value))}
            class="w-[34px] bg-transparent text-center font-mono text-[11px] font-semibold text-white outline-none"
          />
          <span class="text-[10px] font-medium text-[#A1A1AA] select-none">px</span>
        </div>
      </Show>

      <Show when={kind() === "line"}>
        <Divider />
        <button
          type="button"
          aria-label="Arrow head"
          aria-pressed={arrowHead()}
          onClick={() => setArrow(!arrowHead())}
          class={clsx(
            "flex h-[24px] items-center gap-1 rounded-[4px] border px-2 text-[11px] font-medium transition-colors cursor-pointer",
            arrowHead()
              ? "border-editor-accent bg-editor-accent/20 text-white font-semibold shadow-xs"
              : "border-editor-field-border bg-editor-field text-[#A1A1AA] hover:border-[#4B515D] hover:text-white"
          )}
        >
          <span>Arrow</span>
        </button>
      </Show>
    </div>
  );
}