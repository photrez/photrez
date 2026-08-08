import { createSignal, createMemo, For, Show, onCleanup, onMount } from "solid-js";
import { clsx } from "clsx";
import { Icon } from "../icons";
import { useEditor } from "./EditorContext";
import { cancelLayerTransformSession } from "../transformSession";
import type { ToolId } from "../tools/toolTypes";
import { Tooltip } from "../Tooltip";
import { useDialog } from "../dialogs/DialogProvider";
import { TOOL_ITEMS } from "../editorData";

const TOOL_SHORTCUTS: Record<ToolId, string> = {
  move: "V",
  selection: "M",
  crop: "C",
  eyedropper: "I",
  brush: "B",
  eraser: "E",
  paintBucket: "G",
  gradient: "Shift+G",
  shape: "U",
  text: "T",
};

export function LeftToolRail(props: { disabled?: boolean }) {
  const { activeTool, setActiveTool, fgColor, setFgColor, bgColor, setBgColor, scheduler, workspace, layerTransformSession, setLayerTransformSession, colorPickerOpen, setColorPickerOpen, colorPickerTarget, setColorPickerTarget, selectionShape, setSelectionShape } = useEditor();
  const dialogs = useDialog();

  // Tool variant fly-out state: which tool slot has an open popover.
  const [popoverTool, setPopoverTool] = createSignal<string | null>(null);
  let popoverRef: HTMLDivElement | undefined;

  const closePopover = (e: MouseEvent) => {
    if (popoverRef && !popoverRef.contains(e.target as Node)) {
      setPopoverTool(null);
    }
  };

  const cancelActiveTransformSession = () => {
    const engine = workspace.getActiveEngine();
    if (cancelLayerTransformSession(layerTransformSession(), engine)) {
      setLayerTransformSession(null);
      scheduler.requestRender();
    }
  };

  const handleToolChange = (id: ToolId) => {
    if (props.disabled) return;
    if (layerTransformSession() && id !== "move" && id !== "selection") {
      cancelActiveTransformSession();
    }
    setActiveTool(id);
    scheduler.requestRender();
  };

  const handleSwapColors = (e: MouseEvent) => {
    e.stopPropagation();
    if (props.disabled) return;
    const temp = fgColor();
    setFgColor(bgColor());
    setBgColor(temp);
    scheduler.requestRender();
  };

  const handleResetColors = () => {
    if (props.disabled) return;
    setFgColor("#E15A17");
    setBgColor("#FFFFFF");
    scheduler.requestRender();
  };

  const handleOpenColorPicker = async (type: "foreground" | "background") => {
    if (props.disabled) return;
    const initialColor = type === "foreground" ? fgColor() : bgColor();
    const title = type === "foreground" ? "Foreground Color" : "Background Color";
    setColorPickerOpen(true);
    setColorPickerTarget(type);
    const selectedColor = await dialogs.colorPicker({
      title,
      initialColor,
      target: type,
      onChange: (color) => {
        if (type === "foreground") {
          setFgColor(color);
        } else {
          setBgColor(color);
        }
        scheduler.requestRender();
      }
    });
    setColorPickerOpen(false);
    if (selectedColor === null) {
      // Revert back to the initial color if cancelled
      if (type === "foreground") {
        setFgColor(initialColor);
      } else {
        setBgColor(initialColor);
      }
      scheduler.requestRender();
    }
  };

  // Close popover on Escape
  const handlePopoverKeyDown = (e: KeyboardEvent) => {
    if (e.key === "Escape" && popoverTool()) {
      setPopoverTool(null);
    }
  };

  // Keyboard shortcut listener for X (swap) and D (reset)
  const handleKeyDown = (e: KeyboardEvent) => {
    if (document.querySelector('[aria-modal="true"]')) return;
    if (props.disabled) return;
    const active = document.activeElement;
    if (active && (active.tagName === "INPUT" || active.tagName === "TEXTAREA")) return;

    if (e.key.toLowerCase() === "x") {
      const temp = fgColor();
      setFgColor(bgColor());
      setBgColor(temp);
      scheduler.requestRender();
    } else if (e.key.toLowerCase() === "d") {
      handleResetColors();
    }
  };

  onMount(() => {
    window.addEventListener("keydown", handleKeyDown);
    window.addEventListener("keydown", handlePopoverKeyDown);
    onCleanup(() => {
      window.removeEventListener("keydown", handleKeyDown);
      window.removeEventListener("keydown", handlePopoverKeyDown);
    });
  });

  return (
    <aside class={clsx(
      "flex w-[52px] shrink-0 flex-col items-center gap-0.5 bg-editor-toolbar py-2",
      props.disabled && "opacity-50 pointer-events-none grayscale"
    )}>
      <For each={TOOL_ITEMS}>
        {(tool) => {
          // Resolve the active variant icon/label so the rail button reflects
          // the last-selected sub-tool (e.g. ellipse → circle icon).
          const activeVariant = createMemo(() => {
            if (!tool.variants) return null;
            return tool.variants.find((v) => v.id === selectionShape()) ?? tool.variants[0];
          });
          const currentIcon = () => activeVariant()?.icon ?? tool.icon;
          const currentLabel = () => activeVariant()?.label ?? tool.label;
          const tooltipContent = () => currentLabel() + (tool.variants ? " (Right-click for options)" : "");

          return (
            <div class="relative">
              <Tooltip content={tooltipContent()} shortcut={TOOL_SHORTCUTS[tool.id]} placement="right" disabled={popoverTool() === tool.id}>
                <button
                  onClick={() => handleToolChange(tool.id)}
                  onContextMenu={(e) => {
                    if (tool.variants) {
                      e.preventDefault();
                      setPopoverTool(tool.id);
                      window.addEventListener("click", closePopover, { once: true });
                    }
                  }}
                  class={clsx(
                    "flex size-9 shrink-0 items-center justify-center rounded-[5px] transition-all duration-100 relative",
                    activeTool() === tool.id
                      ? "bg-white/5 text-editor-text"
                      : "text-editor-icon hover:bg-white/5 hover:text-editor-text"
                  )}
                  aria-label={currentLabel()}
                  data-has-variants={tool.variants ? "true" : undefined}
                >
                  <Icon name={currentIcon()} class="size-[18px]" strokeWidth={1.6} />

                  {/* Micro-indicator (bottom-right 3px triangle) for tools with sub-variants */}
                  <Show when={tool.variants}>
                    <svg
                      class="absolute bottom-1 right-1 size-[5px] text-editor-text-dim/80 pointer-events-none"
                      viewBox="0 0 6 6"
                      fill="currentColor"
                      aria-hidden="true"
                    >
                      <polygon points="6,6 0,6 6,0" />
                    </svg>
                  </Show>
                </button>
              </Tooltip>

              {/* Variant fly-out popover */}
              <Show when={popoverTool() === tool.id && tool.variants}>
                <div
                  ref={popoverRef!}
                  class="absolute left-full top-0 z-50 ml-1.5 min-w-[140px] overflow-hidden rounded-md border border-editor-field-border bg-editor-panel py-1 shadow-lg"
                >
                  <For each={tool.variants}>
                    {(variant) => (
                      <button
                        onClick={() => {
                          if (variant.id === "ellipse" || variant.id === "rect") {
                            setSelectionShape(variant.id);
                          }
                          setActiveTool(tool.id);
                          setPopoverTool(null);
                          scheduler.requestRender();
                        }}
                        class={clsx(
                          "flex w-full items-center gap-2 px-3 py-1.5 text-[11px] whitespace-nowrap",
                          activeTool() === tool.id && selectionShape() === variant.id
                            ? "bg-white/10 text-editor-text"
                            : "text-editor-text-dim hover:bg-white/5 hover:text-editor-text"
                        )}
                      >
                        <Icon name={variant.icon} class="size-4" strokeWidth={1.6} />
                        {variant.label}
                      </button>
                    )}
                  </For>
                </div>
              </Show>
            </div>
          );
        }}
      </For>

      <div class="mb-1 mt-auto h-px w-6 shrink-0 bg-editor-divider" />
      
      {/* Overlapping Color Swatches Container */}
      <div class="relative size-[38px] shrink-0 group my-2 select-none">
        {/* Background Swatch */}
        <Tooltip content="Background Color" placement="right">
          <div 
            onClick={() => handleOpenColorPicker("background")}
            class="absolute bottom-0 right-0 size-[28px] rounded-full border border-white/20 shadow-md cursor-pointer transition-transform duration-100 hover:scale-105"
            style={{ "background-color": bgColor() }}
          />
        </Tooltip>

        {/* Foreground Swatch */}
        <Tooltip content="Foreground Color" placement="right">
          <div 
            onClick={() => handleOpenColorPicker("foreground")}
            class="absolute top-0 left-0 size-[28px] rounded-full border border-white/30 outline outline-1 outline-black/40 shadow-md cursor-pointer z-10 transition-transform duration-100 hover:scale-105"
            style={{ "background-color": fgColor() }}
          />
        </Tooltip>

        {/* Diagonal Swap Micro-Arrow Trigger */}
        <Tooltip content="Swap Colors" shortcut="X" placement="right">
          <button
            onClick={handleSwapColors}
            class="absolute -top-1.5 -right-1.5 z-20 size-4 bg-editor-toolbar border border-editor-divider rounded-full flex items-center justify-center text-editor-icon hover:text-editor-text scale-0 group-hover:scale-100 transition-transform duration-150 shadow cursor-pointer"
            aria-label="Swap Colors"
          >
            <Icon name="rotate" class="size-2.5" />
          </button>
        </Tooltip>
      </div>
    </aside>
  );
}
