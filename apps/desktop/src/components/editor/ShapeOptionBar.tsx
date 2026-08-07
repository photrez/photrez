// SPDX-License-Identifier: AGPL-3.0-or-later
import { Show } from "solid-js";
import { clsx } from "clsx";
import { useEditor } from "./shell/EditorContext";
import { ToolPill, Divider } from "./shell/OptionBarShared";
import { Tooltip } from "./Tooltip";
import { Icon } from "./icons";
import type { LayerNode, ShapeParams, ShapeKind } from "@/engine/types";

type ShapeLayer = LayerNode & { type: "shape"; shapeParams: ShapeParams };

const KIND_BTNS: { kind: ShapeKind; arrow: boolean; icon: "rectangle" | "circle" | "line" | "arrowUpRight"; label: string; short: string; content: string }[] = [
  { kind: "rect", arrow: false, icon: "rectangle", label: "Rectangle", short: "Rect", content: "Solid/filled rectangle" },
  { kind: "ellipse", arrow: false, icon: "circle", label: "Ellipse", short: "Ellipse", content: "Solid/filled ellipse" },
  { kind: "line", arrow: false, icon: "line", label: "Line", short: "Line", content: "Straight line" },
  { kind: "line", arrow: true, icon: "arrowUpRight", label: "Arrow", short: "Arrow", content: "Line with arrow head" },
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
    selectedLayerId,
    shapeKind, setShapeKind,
    shapeFillEnabled, setShapeFillEnabled,
    shapeStrokeEnabled, setShapeStrokeEnabled,
    shapeStrokeColor, setShapeStrokeColor,
    shapeStrokeWidth, setShapeStrokeWidth,
    shapeRadius, setShapeRadius,
    shapeArrowHead, setShapeArrowHead,
  } = useEditor();

  const selectedShape = (): ShapeLayer | null => {
    const engine = workspace.getActiveEngine();
    if (!engine) return null;
    const id = selectedLayerId();
    if (!id) return null;
    const layer = engine.getLayer(id);
    // guard: engine could be a partial/skinny mock or the layer is a raster
    return layer && layer.type === "shape" && layer.shapeParams ? (layer as ShapeLayer) : null;
  };

  const isEditMode = () => !!selectedShape();
  const shape = () => selectedShape()!;

  const kind = () => (isEditMode() ? shape().shapeParams.kind : shapeKind());
  const fillEnabled = () => (isEditMode() ? shape().shapeParams.fill.kind === "solid" : shapeFillEnabled());
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
    // commit BEFORE mutation (AGENTS.md wiring rule)
    history.commit(engine.snapshot(), "Edit Shape");
    engine.updateShapeParams(layer.id, { ...layer.shapeParams, ...patch });
  };

  const selectKind = (next: ShapeKind, arrow: boolean) => {
    if (isEditMode()) {
      applyEdit({ kind: next, arrowHead: next === "line" ? arrow : false });
    } else {
      setShapeKind(next);
      if (next === "line") setShapeArrowHead(arrow);
    }
  };

  const setFill = (on: boolean) => {
    if (isEditMode()) applyEdit({ fill: { ...shape().shapeParams.fill, kind: on ? "solid" : "none" } });
    else setShapeFillEnabled(on);
  };

  const setStroke = (on: boolean) => {
    if (isEditMode()) applyEdit({ stroke: { ...shape().shapeParams.stroke, enabled: on } });
    else setShapeStrokeEnabled(on);
  };

  const setColor = (color: string) => {
    if (isEditMode()) applyEdit({ stroke: { ...shape().shapeParams.stroke, color, enabled: true } });
    else setShapeStrokeColor(color);
  };

  const setWidth = (width: number) => {
    const v = Math.max(1, width);
    if (isEditMode()) applyEdit({ stroke: { ...shape().shapeParams.stroke, width: v } });
    else setShapeStrokeWidth(v);
  };

  const setRadius = (value: number) => {
    const v = Math.max(0, Math.min(100, value));
    if (isEditMode()) applyEdit({ radius: v });
    else setShapeRadius(v);
  };

  const setArrow = (on: boolean) => {
    if (isEditMode()) applyEdit({ arrowHead: on });
    else setShapeArrowHead(on);
  };

  const isKindActive = (btn: (typeof KIND_BTNS)[number]) =>
    btn.kind === "line"
      ? kind() === "line" && arrowHead() === btn.arrow
      : kind() === btn.kind;

  const toggleBtnClass = clsx(
    "flex h-[24px] shrink-0 cursor-pointer items-center gap-1.5 rounded-[3px] border border-editor-field-border bg-editor-field px-1.5 select-none",
  );

  return (
    <div data-shape-option-bar class="flex items-center gap-1.5 text-[11px] select-none">
      <ToolPill icon="rectangle" label="Shape" />

      <Divider />

      {/* Kind segmented control */}
      <div class="flex shrink-0 items-center gap-0.5">
        {KIND_BTNS.map((btn) => {
          const active = isKindActive(btn);
          return (
            <Tooltip content={btn.content} placement="top">
              <button
                type="button"
                aria-label={btn.label}
                aria-pressed={active}
                onClick={() => selectKind(btn.kind, btn.arrow)}
                class={clsx(
                  "flex h-[24px] shrink-0 items-center gap-1 rounded-[3px] border px-1.5 text-[11px] font-medium transition-colors",
                  active
                    ? "border-editor-accent/80 bg-editor-accent/15 text-editor-text shadow-sm"
                    : "border-transparent text-editor-text-dim hover:border-editor-field-border hover:bg-editor-field/40 hover:text-editor-text",
                )}
              >
                <Icon name={btn.icon} class="size-3" strokeWidth={1.5} />
                <span class="@max-[900px]:hidden">{btn.short}</span>
              </button>
            </Tooltip>
          );
        })}
      </div>

      <Divider />

      <label class={clsx(toggleBtnClass, "justify-center")}>
        <input type="checkbox" aria-label="Fill" checked={fillEnabled()} onChange={(e) => setFill(e.currentTarget.checked)} class="accent-editor-accent" />
        <span class={fillEnabled() ? "text-editor-text" : "text-editor-text-dim"}>Fill</span>
      </label>

      <label class={clsx(toggleBtnClass, "justify-center")}>
        <input type="checkbox" aria-label="Stroke" checked={strokeEnabled()} onChange={(e) => setStroke(e.currentTarget.checked)} class="accent-editor-accent" />
        <span class={strokeEnabled() ? "text-editor-text" : "text-editor-text-dim"}>Stroke</span>
      </label>

      <Show when={strokeEnabled()}>
        <Divider />

        <input
          type="color"
          aria-label="Stroke color"
          value={strokeColor()}
          onInput={(e) => setColor(e.currentTarget.value)}
          class="size-[22px] shrink-0 cursor-pointer rounded-[3px] border border-editor-field-border bg-transparent p-0"
        />

        <label class={toggleBtnClass}>
          <input
            type="range"
            aria-label="Stroke width"
            min={1}
            max={40}
            value={strokeWidthValue()}
            onInput={(e) => setWidth(Number(e.currentTarget.value))}
            class="w-6 accent-editor-accent"
          />
          <span class="w-6 text-right text-[10px] tabular-nums text-editor-text-dim">{strokeWidthValue()}px</span>
        </label>
      </Show>

      <Show when={kind() === "rect"}>
        <Divider />
        <label class={toggleBtnClass}>
          <span class="text-[10px] font-medium text-editor-text-dim">Radius</span>
          <input
            type="range"
            aria-label="Corner radius"
            min={0}
            max={100}
            value={radius()}
            onInput={(e) => setRadius(Number(e.currentTarget.value))}
            class="w-6 accent-editor-accent"
          />
          <span class="w-6 text-right text-[10px] tabular-nums text-editor-text-dim">{radius()}px</span>
        </label>
      </Show>

      <Show when={kind() === "line"}>
        <Divider />
        <label class={clsx(toggleBtnClass, "justify-center")}>
          <input type="checkbox" aria-label="Arrow head" checked={arrowHead()} onChange={(e) => setArrow(e.currentTarget.checked)} class="accent-editor-accent" />
          <span class={arrowHead() ? "text-editor-text" : "text-editor-text-dim"}>Arrow</span>
        </label>
      </Show>
    </div>
  );
}