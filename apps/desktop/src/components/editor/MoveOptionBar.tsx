import { Show, createMemo } from "solid-js";
import { Icon } from "./icons";
import { NumField, EditableNumField } from "./primitives";
import { clsx } from "clsx";
import { Tooltip } from "./Tooltip";
import { useEditor } from "./shell/EditorContext";
import { ToggleBtn, Divider, ToolPill, MoreDropdown, OptionCheckbox } from "./shell/OptionBarShared";

export function MoveOptionBar() {
  const {
    workspace,
    activeTool,
    layers,
    selectedLayerId,
    scheduler,
    moveAutoSelect,
    setMoveAutoSelect,
    moveSnapEnabled,
    setMoveSnapEnabled,
    showTransformControls,
    setShowTransformControls,
    hoveredLayerId,
    docWidth,
    docHeight,
  } = useEditor();

  const hoveredLayer = () => {
    const id = hoveredLayerId();
    if (!id) return null;
    return layers().find(l => l.id === id) || null;
  };

  const activeLayer = () => {
    const id = selectedLayerId();
    if (!id) return null;
    return layers().find(l => l.id === id) || null;
  };

  const activeLayerSafe = () => {
    const id = selectedLayerId();
    if (!id) return null;
    const engine = workspace.getActiveEngine();
    if (!engine) return null;
    return engine.getLayer(id) || null;
  };

  const isLocked = () => {
    const l = activeLayerSafe();
    return l ? l.locked : false;
  };

  const handleFlip = (axis: "h" | "v") => {
    if (isLocked()) return;
    const engine = workspace.getActiveEngine();
    const id = selectedLayerId();
    if (engine && id) {
      const history = workspace.getActiveHistory();
      history?.commit(engine.snapshot(), "Flip Layer");
      engine.flipLayer(id, axis);
      scheduler.requestRender();
    }
  };

  const handleResetTransform = () => {
    if (isLocked()) return;
    const engine = workspace.getActiveEngine();
    const id = selectedLayerId();
    if (engine && id) {
      const layer = engine.getLayer(id);
      // Skip if already at default — prevents ghost undo entries on
      // repeated Reset clicks (@regression 2026-06-18 follow-up).
      if (
        layer &&
        layer.transform.x === 0 &&
        layer.transform.y === 0 &&
        layer.transform.scaleX === 1 &&
        layer.transform.scaleY === 1 &&
        layer.transform.rotation === 0 &&
        !layer.transform.flipH &&
        !layer.transform.flipV
      ) {
        return;
      }
      const history = workspace.getActiveHistory();
      history?.commit(engine.snapshot(), "Reset Layer Transform");
      engine.transformLayer(id, {
        x: 0,
        y: 0,
        scaleX: 1.0,
        scaleY: 1.0,
        rotation: 0,
        flipH: false,
        flipV: false
      });
      scheduler.requestRender();
    }
  };

  const handlePositionField = (axis: "x" | "y") => (val: number) => {
    const engine = workspace.getActiveEngine();
    const id = selectedLayerId();
    if (engine && id) {
      const layer = engine.getLayer(id);
      if (!layer || layer.locked) return;
      // Skip no-op submits (user blurred without changing). Prevents ghost
      // undo entries that make undo feel "stuck" — pressing undo would
      // appear to do nothing because the snapshot matches current state.
      if (layer.transform[axis] === val) return;
      const history = workspace.getActiveHistory();
      history?.commit(engine.snapshot(), "Transform Layer");
      const next = { ...layer.transform };
      next[axis] = val;
      engine.transformLayer(id, next);
      scheduler.requestRender();
    }
  };

  const handleRotateField = (val: number) => {
    const engine = workspace.getActiveEngine();
    const id = selectedLayerId();
    if (engine && id) {
      const layer = engine.getLayer(id);
      if (!layer || layer.locked) return;
      if (layer.transform.rotation === val) return;
      const history = workspace.getActiveHistory();
      history?.commit(engine.snapshot(), "Transform Layer");
      engine.transformLayer(id, { ...layer.transform, rotation: val });
      scheduler.requestRender();
    }
  };

  const handleAlign = (type: "left" | "center-h" | "right" | "top" | "center-v" | "bottom") => {
    const engine = workspace.getActiveEngine();
    const id = selectedLayerId();
    if (engine && id) {
      const layer = engine.getLayer(id);
      if (!layer || layer.locked) return;
      const next = { ...layer.transform };
      
      const layerW = Math.round(layer.width * layer.transform.scaleX);
      const layerH = Math.round(layer.height * layer.transform.scaleY);
      const docW = docWidth();
      const docH = docHeight();

      switch (type) {
        case "left":
          next.x = 0;
          break;
        case "center-h":
          next.x = Math.round((docW - layerW) / 2);
          break;
        case "right":
          next.x = docW - layerW;
          break;
        case "top":
          next.y = 0;
          break;
        case "center-v":
          next.y = Math.round((docH - layerH) / 2);
          break;
        case "bottom":
          next.y = docH - layerH;
          break;
      }
      // Skip if alignment is a no-op (layer already at the requested edge).
      if (next.x === layer.transform.x && next.y === layer.transform.y) return;
      const history = workspace.getActiveHistory();
      history?.commit(engine.snapshot(), "Align Layer");
      engine.transformLayer(id, next);
      scheduler.requestRender();
    }
  };

  return (
    <>
      <ToolPill icon="cursor" label="Move" />

      <Divider />

      <Tooltip content="Auto-Select Layer on Hover">
        <OptionCheckbox
          checked={moveAutoSelect()}
          onChange={setMoveAutoSelect}
          label="Auto-Select"
        />
      </Tooltip>

      <Tooltip content="Enable Snapping">
        <OptionCheckbox
          checked={moveSnapEnabled()}
          onChange={setMoveSnapEnabled}
          label="Snap"
        />
      </Tooltip>

      <Divider />

      <Tooltip content="Show transform bounding box and handles around selected layer">
        <OptionCheckbox
          checked={showTransformControls()}
          onChange={setShowTransformControls}
          label="Transform Controls"
        />
      </Tooltip>

      <Show when={moveAutoSelect() && hoveredLayer()}>
        {(hl) => (
          <div class="flex h-[24px] shrink-0 items-center gap-1.5 rounded-[4px] bg-editor-field border border-editor-field-border px-2 text-[11px] text-editor-text-dim max-w-[120px] overflow-hidden">
            <span class="text-[9px] text-editor-accent font-bold uppercase tracking-wider @max-[900px]:hidden">Target:</span>
            <span class="text-editor-text font-medium truncate">{hl().name}</span>
          </div>
        )}
      </Show>

      <Show when={selectedLayerId()}>
        <Divider />

        <Show when={isLocked()}>
          <div class="flex h-[24px] shrink-0 items-center gap-1 rounded-[3px] border border-editor-accent/20 bg-editor-accent/5 px-2 text-[11px] text-editor-accent/70">
            <Icon name="lock" class="size-3" strokeWidth={1.5} />
            Locked
          </div>
          <Divider />
        </Show>

        <Show when={activeLayer()}>
          {(layer) => {
            const d = isLocked();
            return (
              <>
                <div class="flex shrink-0 items-center gap-1">
                  <EditableNumField
                    label="X"
                    labelClass="@max-[900px]:hidden"
                    suffix="px"
                    value={layer().transform.x}
                    disabled={d}
                    onSubmit={handlePositionField("x")}
                    class="w-[62px]"
                  />
                  <EditableNumField
                    label="Y"
                    labelClass="@max-[900px]:hidden"
                    suffix="px"
                    value={layer().transform.y}
                    disabled={d}
                    onSubmit={handlePositionField("y")}
                    class="w-[62px]"
                  />
                </div>

                <div class="flex shrink-0 items-center gap-1">
                  <NumField label="W" labelClass="@max-[900px]:hidden" value={`${Math.round(layer().width * layer().transform.scaleX)}`} suffix="px" class="w-[70px]" />
                  <NumField label="H" labelClass="@max-[900px]:hidden" value={`${Math.round(layer().height * layer().transform.scaleY)}`} suffix="px" class="w-[70px]" />
                </div>

                <EditableNumField
                  label="R"
                  labelClass="@max-[900px]:hidden"
                  value={layer().transform.rotation}
                  suffix="°"
                  disabled={d}
                  onSubmit={handleRotateField}
                  class="w-[58px]"
                />
              </>
            );
          }}
        </Show>

        {/* Main Bar Controls (hidden on narrow container) */}
        <div class="hidden @min-[880px]:flex items-center gap-1.5 shrink-0">
          <Divider />

          <div class={clsx("flex shrink-0 items-center gap-1 text-editor-icon", isLocked() && "opacity-30 pointer-events-none")}>
            <span class="text-[11px] text-editor-text-dim @max-[900px]:hidden">Align</span>
            <Tooltip content="Align Left">
              <button onClick={() => handleAlign("left")} class="rounded-[3px] p-0.5 hover:text-editor-text" aria-label="Align left">
                <Icon name="align-left" class="size-4" strokeWidth={1.5} />
              </button>
            </Tooltip>
            <Tooltip content="Align Horizontal Center">
              <button onClick={() => handleAlign("center-h")} class="rounded-[3px] p-0.5 hover:text-editor-text" aria-label="Align center horizontal">
                <Icon name="align-h" class="size-4" strokeWidth={1.5} />
              </button>
            </Tooltip>
            <Tooltip content="Align Right">
              <button onClick={() => handleAlign("right")} class="rounded-[3px] p-0.5 hover:text-editor-text" aria-label="Align right">
                <Icon name="align-right" class="size-4" strokeWidth={1.5} />
              </button>
            </Tooltip>
            <Tooltip content="Align Top">
              <button onClick={() => handleAlign("top")} class="rounded-[3px] p-0.5 hover:text-editor-text" aria-label="Align top">
                <Icon name="align-top" class="size-4" strokeWidth={1.5} />
              </button>
            </Tooltip>
            <Tooltip content="Align Vertical Center">
              <button onClick={() => handleAlign("center-v")} class="rounded-[3px] p-0.5 hover:text-editor-text" aria-label="Align center vertical">
                <Icon name="align-v" class="size-4" strokeWidth={1.5} />
              </button>
            </Tooltip>
            <Tooltip content="Align Bottom">
              <button onClick={() => handleAlign("bottom")} class="rounded-[3px] p-0.5 hover:text-editor-text" aria-label="Align bottom">
                <Icon name="align-bottom" class="size-4" strokeWidth={1.5} />
              </button>
            </Tooltip>
          </div>

          <Divider />

          <div class={clsx("flex shrink-0 items-center gap-1 text-editor-icon", isLocked() && "opacity-30 pointer-events-none")}>
            <span class="text-[11px] text-editor-text-dim @max-[900px]:hidden">Flip</span>
            <Tooltip content="Flip Horizontal">
              <button onClick={() => handleFlip("h")} class="rounded-[3px] p-0.5 hover:text-editor-text" aria-label="Flip horizontal">
                <Icon name="flip-h" class="size-4" strokeWidth={1.5} />
              </button>
            </Tooltip>
            <Tooltip content="Flip Vertical">
              <button onClick={() => handleFlip("v")} class="rounded-[3px] p-0.5 hover:text-editor-text" aria-label="Flip vertical">
                <Icon name="flip-v" class="size-4" strokeWidth={1.5} />
              </button>
            </Tooltip>
          </div>

          <Divider />

          <Tooltip content="Reset Transform">
            <button onClick={handleResetTransform} disabled={isLocked()} class={clsx(
              "flex h-[24px] shrink-0 items-center rounded-[3px] border border-transparent px-2 text-[11px]",
              isLocked()
                ? "text-editor-text-dim/30 cursor-default"
                : "text-editor-text-dim hover:border-editor-field-border hover:text-editor-text",
            )}>
              Reset
            </button>
          </Tooltip>
        </div>

        {/* Overflow dropdown for narrow container */}
        <MoreDropdown>
          <div class={clsx("flex flex-col gap-1.5 text-editor-icon", isLocked() && "opacity-30 pointer-events-none")}>
            <span class="text-[10px] font-bold text-editor-text-dim uppercase tracking-wider">Align</span>
            <div class="flex items-center gap-2 bg-editor-field/30 p-1.5 rounded-[4px] border border-editor-field-border">
              <Tooltip content="Align Left">
                <button onClick={() => handleAlign("left")} class="rounded-[3px] p-0.5 hover:text-editor-text" aria-label="Align left">
                  <Icon name="align-left" class="size-4" strokeWidth={1.5} />
                </button>
              </Tooltip>
              <Tooltip content="Align Horizontal Center">
                <button onClick={() => handleAlign("center-h")} class="rounded-[3px] p-0.5 hover:text-editor-text" aria-label="Align center horizontal">
                  <Icon name="align-h" class="size-4" strokeWidth={1.5} />
                </button>
              </Tooltip>
              <Tooltip content="Align Right">
                <button onClick={() => handleAlign("right")} class="rounded-[3px] p-0.5 hover:text-editor-text" aria-label="Align right">
                  <Icon name="align-right" class="size-4" strokeWidth={1.5} />
                </button>
              </Tooltip>
              <Tooltip content="Align Top">
                <button onClick={() => handleAlign("top")} class="rounded-[3px] p-0.5 hover:text-editor-text" aria-label="Align top">
                  <Icon name="align-top" class="size-4" strokeWidth={1.5} />
                </button>
              </Tooltip>
              <Tooltip content="Align Vertical Center">
                <button onClick={() => handleAlign("center-v")} class="rounded-[3px] p-0.5 hover:text-editor-text" aria-label="Align center vertical">
                  <Icon name="align-v" class="size-4" strokeWidth={1.5} />
                </button>
              </Tooltip>
              <Tooltip content="Align Bottom">
                <button onClick={() => handleAlign("bottom")} class="rounded-[3px] p-0.5 hover:text-editor-text" aria-label="Align bottom">
                  <Icon name="align-bottom" class="size-4" strokeWidth={1.5} />
                </button>
              </Tooltip>
            </div>
          </div>

          <div class={clsx("flex flex-col gap-1.5 text-editor-icon mt-1", isLocked() && "opacity-30 pointer-events-none")}>
            <span class="text-[10px] font-bold text-editor-text-dim uppercase tracking-wider">Flip</span>
            <div class="flex items-center gap-2 bg-editor-field/30 p-1.5 rounded-[4px] border border-editor-field-border">
              <Tooltip content="Flip Horizontal">
                <button onClick={() => handleFlip("h")} class="rounded-[3px] p-0.5 hover:text-editor-text" aria-label="Flip horizontal">
                  <Icon name="flip-h" class="size-4" strokeWidth={1.5} />
                </button>
              </Tooltip>
              <Tooltip content="Flip Vertical">
                <button onClick={() => handleFlip("v")} class="rounded-[3px] p-0.5 hover:text-editor-text" aria-label="Flip vertical">
                  <Icon name="flip-v" class="size-4" strokeWidth={1.5} />
                </button>
              </Tooltip>
            </div>
          </div>

          <div class="h-px bg-editor-divider my-1" />

          <Tooltip content="Reset Transform">
            <button
              onClick={handleResetTransform}
              disabled={isLocked()}
              class={clsx(
                "flex h-[24px] w-full items-center justify-center rounded-[3px] border px-2 text-[11px] font-medium transition-colors",
                isLocked()
                  ? "border-transparent text-editor-text-dim/30 cursor-default"
                  : "border-editor-field-border bg-editor-field text-editor-text hover:bg-editor-field/85",
              )}
            >
              Reset Transform
            </button>
          </Tooltip>
        </MoreDropdown>
      </Show>
    </>
  );
}
