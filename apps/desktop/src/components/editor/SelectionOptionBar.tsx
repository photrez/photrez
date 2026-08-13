import { Show, createSignal } from "solid-js";
import { useEditor } from "./shell/EditorContext";
import { EditableNumField } from "./primitives";
import { ToolPill, MoreDropdown, Divider, ToggleBtn, SelectDropdown } from "./shell/OptionBarShared";
import { Tooltip } from "./Tooltip";
import { Icon } from "./icons";
import { SelectionOperations } from "@/features/selection/SelectionOperations";
import { clsx } from "clsx";

export function SelectionOptionBar() {
  const {
    workspace,
    renderer,
    scheduler,
    selection: selectionSignal,
    activeTool,
    selectionEditMode,
    setSelectionEditMode,
    selectionConstraintMode,
    setSelectionConstraintMode,
    selectionRatioW,
    setSelectionRatioW,
    selectionRatioH,
    setSelectionRatioH,
    selectionSizeW,
    setSelectionSizeW,
    selectionSizeH,
    setSelectionSizeH,
    selectionShape,
    setSelectionShape,
  } = useEditor();

  const engine = () => workspace.getActiveEngine();
  const historyGetter = () => workspace.getActiveHistory();
  const selection = () => selectionSignal() ?? engine()?.getSelection() ?? null;
  const hasSelection = () => selection() !== null;

  const uploadActiveLayerBitmap = () => {
    const e = engine();
    if (!e) return;
    const activeId = e.getActiveLayerId();
    if (!activeId) return;
    const layer = e.getLayer(activeId);
    if (layer?.imageBitmap) {
      renderer.uploadImage(layer.id, layer.imageBitmap);
    }
  };

  const submitW = (n: number) => {
    const s = selection();
    if (s && !isNaN(n) && n > 0) {
      engine()?.createSelection(s.x, s.y, n, s.height, s.angle, selectionShape());
      scheduler.requestRender();
    }
  };

  const submitH = (n: number) => {
    const s = selection();
    if (s && !isNaN(n) && n > 0) {
      engine()?.createSelection(s.x, s.y, s.width, n, s.angle, selectionShape());
      scheduler.requestRender();
    }
  };

  const submitX = (n: number) => {
    const s = selection();
    if (s && !isNaN(n)) {
      engine()?.createSelection(n, s.y, s.width, s.height, s.angle, selectionShape());
      scheduler.requestRender();
    }
  };

  const submitY = (n: number) => {
    const s = selection();
    if (s && !isNaN(n)) {
      engine()?.createSelection(s.x, n, s.width, s.height, s.angle, selectionShape());
      scheduler.requestRender();
    }
  };

  const submitAngle = (n: number) => {
    const s = selection();
    if (s && !isNaN(n)) {
      engine()?.createSelection(s.x, s.y, s.width, s.height, n, selectionShape());
      scheduler.requestRender();
    }
  };

  const handleInvert = () => {
    engine()?.invertSelection();
    setSelectionEditMode(false);
    scheduler.requestRender();
  };

  const handleDeselect = () => {
    engine()?.clearSelection();
    setSelectionEditMode(false);
    scheduler.requestRender();
  };

  const handleCut = () => {
    const e = engine();
    const h = historyGetter();
    if (e?.getSelection() && h) {
      // Commit pre-action snapshot so the cut is undoable AND redoable.
      h.commit(e.snapshot(), "Cut");
      SelectionOperations.cutSelection(e);
      uploadActiveLayerBitmap();
      scheduler.requestRender();
    }
  };

  const handleCopy = () => {
    const e = engine();
    if (e?.getSelection()) {
      SelectionOperations.copySelection(e);
    }
  };

  const handlePaste = () => {
    const e = engine();
    const h = historyGetter();
    if (e && h) {
      // Commit pre-action snapshot so the new layer is undoable/redoable.
      h.commit(e.snapshot(), "Paste");
      SelectionOperations.pasteSelection(e);
      uploadActiveLayerBitmap();
      scheduler.requestRender();
    }
  };

  const handleDelete = () => {
    const e = engine();
    const h = historyGetter();
    if (e?.getSelection() && h) {
      // Commit pre-action snapshot so the deletion is undoable/redoable.
      h.commit(e.snapshot(), "Delete Pixels");
      SelectionOperations.deleteSelection(e);
      uploadActiveLayerBitmap();
      scheduler.requestRender();
    }
  };

  return (
    <>
      <ToolPill icon={selectionShape() === "ellipse" ? "circle-dashed" : "square-dashed"} label="Selection" />

      <Divider />

      {/* Marquee shape dropdown selector */}
      <SelectDropdown
        value={selectionShape()}
        options={[
          { value: "rect", label: "Rectangular", icon: "square-dashed" },
          { value: "ellipse", label: "Elliptical", icon: "circle-dashed" },
        ]}
        onChange={(v) => setSelectionShape(v as "rect" | "ellipse")}
      />

      <Divider />

      {/* Style/Constraint Selector */}
      <SelectDropdown
        labelPrefix="Style"
        value={selectionConstraintMode()}
        options={[
          { value: "normal", label: "Normal" },
          { value: "ratio", label: "Fixed Ratio" },
          { value: "size", label: "Fixed Size" },
        ]}
        onChange={(v) => setSelectionConstraintMode(v as "normal" | "ratio" | "size")}
      />

      {/* Show constraint W/H inputs when style is Fixed Ratio or Fixed Size */}
      <Show when={selectionConstraintMode() !== "normal"}>
        <div class="flex shrink-0 items-center gap-1">
          <EditableNumField
            label="W"
            suffix={selectionConstraintMode() === "size" ? "px" : undefined}
            value={selectionConstraintMode() === "ratio" ? selectionRatioW() : selectionSizeW()}
            onSubmit={(v) => {
              if (v > 0) {
                if (selectionConstraintMode() === "ratio") {
                  setSelectionRatioW(v);
                } else {
                  setSelectionSizeW(v);
                }
              }
            }}
            class="w-[62px]"
          />
          <EditableNumField
            label="H"
            suffix={selectionConstraintMode() === "size" ? "px" : undefined}
            value={selectionConstraintMode() === "ratio" ? selectionRatioH() : selectionSizeH()}
            onSubmit={(v) => {
              if (v > 0) {
                if (selectionConstraintMode() === "ratio") {
                  setSelectionRatioH(v);
                } else {
                  setSelectionSizeH(v);
                }
              }
            }}
            class="w-[62px]"
          />
        </div>
      </Show>

      <Divider />

      <span class="hidden @min-[960px]:inline-block text-[10px] font-bold uppercase tracking-wider text-editor-text-dim shrink-0">Position</span>

      <div class="flex shrink-0 items-center gap-1">
        <EditableNumField
          label="X"
          labelClass="@max-[900px]:hidden"
          suffix="px"
          value={selection()?.x ?? 0}
          onSubmit={submitX}
          disabled={!hasSelection()}
          class="w-[62px]"
        />
        <EditableNumField
          label="Y"
          labelClass="@max-[900px]:hidden"
          suffix="px"
          value={selection()?.y ?? 0}
          onSubmit={submitY}
          disabled={!hasSelection()}
          class="w-[62px]"
        />
      </div>

      <Divider />

      <span class="hidden @min-[960px]:inline-block text-[10px] font-bold uppercase tracking-wider text-editor-text-dim shrink-0">Size</span>

      <div class="flex shrink-0 items-center gap-1">
        <EditableNumField
          label="W"
          labelClass="@max-[900px]:hidden"
          suffix="px"
          value={selection()?.width ?? 0}
          onSubmit={submitW}
          disabled={!hasSelection()}
          class="w-[62px]"
        />
        <EditableNumField
          label="H"
          labelClass="@max-[900px]:hidden"
          suffix="px"
          value={selection()?.height ?? 0}
          onSubmit={submitH}
          disabled={!hasSelection()}
          class="w-[62px]"
        />
      </div>

      <Divider />

      <span class="hidden @min-[960px]:inline-block text-[10px] font-bold uppercase tracking-wider text-editor-text-dim shrink-0">Rotation</span>

      <EditableNumField
        label="R"
        labelClass="@max-[900px]:hidden"
        value={selection()?.angle ?? 0}
        suffix="°"
        onSubmit={submitAngle}
        disabled={!hasSelection()}
        class="w-[58px]"
      />

      {/* Main Bar Controls */}
      <div class="hidden @min-[880px]:flex items-center gap-1.5 shrink-0">
        <Divider />
        <Tooltip content="Show resize/rotate handles" shortcut="Ctrl+T">
          <ToggleBtn
            active={selectionEditMode() && hasSelection()}
            onChange={(val) => {
              if (hasSelection()) setSelectionEditMode(val);
            }}
            icon="maximize"
            label="Transform"
            labelClass="@max-[900px]:hidden"
            class={clsx(!hasSelection() && "opacity-30 pointer-events-none")}
          />
        </Tooltip>

        <Divider />

        <Tooltip content="Cut Selection" shortcut="Ctrl+X">
          <button
            onClick={handleCut}
            disabled={!hasSelection()}
            class={clsx(
              "flex h-[24px] shrink-0 items-center gap-1 rounded-[3px] border border-transparent px-2 text-[11px]",
              hasSelection()
                ? "text-editor-text-dim hover:border-editor-field-border hover:text-editor-text cursor-pointer"
                : "text-editor-text-dim opacity-30 pointer-events-none"
            )}
          >
            <Icon name="slice" class="size-3" strokeWidth={1.5} />
            Cut
          </button>
        </Tooltip>

        <Tooltip content="Copy Selection" shortcut="Ctrl+C">
          <button
            onClick={handleCopy}
            disabled={!hasSelection()}
            class={clsx(
              "flex h-[24px] shrink-0 items-center gap-1 rounded-[3px] border border-transparent px-2 text-[11px]",
              hasSelection()
                ? "text-editor-text-dim hover:border-editor-field-border hover:text-editor-text cursor-pointer"
                : "text-editor-text-dim opacity-30 pointer-events-none"
            )}
          >
            <Icon name="copy" class="size-3" strokeWidth={1.5} />
            Copy
          </button>
        </Tooltip>

        <Tooltip content="Paste" shortcut="Ctrl+V">
          <button
            onClick={handlePaste}
            disabled={!hasSelection()}
            class={clsx(
              "flex h-[24px] shrink-0 items-center gap-1 rounded-[3px] border border-transparent px-2 text-[11px]",
              hasSelection()
                ? "text-editor-text-dim hover:border-editor-field-border hover:text-editor-text cursor-pointer"
                : "text-editor-text-dim opacity-30 pointer-events-none"
            )}
          >
            <Icon name="square-dashed" class="size-3" strokeWidth={1.5} />
            Paste
          </button>
        </Tooltip>

        <Divider />

        <Tooltip content="Invert Selection" shortcut="Ctrl+I">
          <button
            onClick={handleInvert}
            disabled={!hasSelection()}
            class={clsx(
              "flex h-[24px] shrink-0 items-center gap-1 rounded-[3px] border border-transparent px-2 text-[11px]",
              hasSelection()
                ? "text-editor-text-dim hover:border-editor-field-border hover:text-editor-text cursor-pointer"
                : "text-editor-text-dim opacity-30 pointer-events-none"
            )}
          >
            <Icon name="flip-h" class="size-3" strokeWidth={1.5} />
            Invert
          </button>
        </Tooltip>

        <Tooltip content="Delete Selection Pixels" shortcut="Del">
          <button
            onClick={handleDelete}
            disabled={!hasSelection()}
            class={clsx(
              "flex h-[24px] shrink-0 items-center gap-1 rounded-[3px] border border-transparent px-2 text-[11px]",
              hasSelection()
                ? "text-editor-text-dim hover:border-editor-field-border hover:text-editor-text cursor-pointer"
                : "text-editor-text-dim opacity-30 pointer-events-none"
            )}
          >
            <Icon name="trash" class="size-3" strokeWidth={1.5} />
            Delete
          </button>
        </Tooltip>

        <Tooltip content="Deselect" shortcut="Esc">
          <button
            onClick={handleDeselect}
            disabled={!hasSelection()}
            class={clsx(
              "flex h-[24px] shrink-0 items-center gap-1 rounded-[3px] border border-transparent px-2 text-[11px]",
              hasSelection()
                ? "text-editor-text-dim hover:border-editor-field-border hover:text-editor-text cursor-pointer"
                : "text-editor-text-dim opacity-30 pointer-events-none"
            )}
          >
            <Icon name="x" class="size-3" strokeWidth={1.5} />
            Deselect
          </button>
        </Tooltip>
      </div>

      {/* Overflow dropdown for narrow container */}
      <MoreDropdown>
        <div class="flex flex-col gap-1.5">
          <span class="text-[10px] font-bold text-editor-text-dim uppercase tracking-wider">Position</span>
          <div class="grid grid-cols-2 gap-1.5">
            <EditableNumField label="X" value={selection()?.x ?? 0} onSubmit={submitX} disabled={!hasSelection()} suffix="px" class="w-full" />
            <EditableNumField label="Y" value={selection()?.y ?? 0} onSubmit={submitY} disabled={!hasSelection()} suffix="px" class="w-full" />
          </div>
        </div>

        <div class="flex flex-col gap-1.5">
          <span class="text-[10px] font-bold text-editor-text-dim uppercase tracking-wider">Size</span>
          <div class="grid grid-cols-2 gap-1.5">
            <EditableNumField label="W" value={selection()?.width ?? 0} onSubmit={submitW} disabled={!hasSelection()} suffix="px" class="w-full" />
            <EditableNumField label="H" value={selection()?.height ?? 0} onSubmit={submitH} disabled={!hasSelection()} suffix="px" class="w-full" />
          </div>
        </div>

        <div class="flex flex-col gap-1.5">
          <span class="text-[10px] font-bold text-editor-text-dim uppercase tracking-wider">Rotation</span>
          <EditableNumField
            label="R"
            value={selection()?.angle ?? 0}
            suffix="°"
            onSubmit={submitAngle}
            disabled={!hasSelection()}
            class="w-full"
          />
        </div>

        <div class="h-px bg-editor-divider my-1" />

        <div class="grid grid-cols-2 gap-1.5">
          <Tooltip content="Cut Selection" shortcut="Ctrl+X">
            <button
              onClick={handleCut}
              disabled={!hasSelection()}
              class={clsx(
                "flex h-[24px] items-center justify-center gap-1 rounded-[3px] border px-2 text-[11px] transition-colors",
                hasSelection()
                  ? "border-editor-field-border bg-editor-field text-editor-text hover:bg-editor-field/85 cursor-pointer"
                  : "border-transparent text-editor-text-dim opacity-30 pointer-events-none"
              )}
            >
              <Icon name="slice" class="size-3" strokeWidth={1.5} />
              Cut
            </button>
          </Tooltip>
          <Tooltip content="Copy Selection" shortcut="Ctrl+C">
            <button
              onClick={handleCopy}
              disabled={!hasSelection()}
              class={clsx(
                "flex h-[24px] items-center justify-center gap-1 rounded-[3px] border px-2 text-[11px] transition-colors",
                hasSelection()
                  ? "border-editor-field-border bg-editor-field text-editor-text hover:bg-editor-field/85 cursor-pointer"
                  : "border-transparent text-editor-text-dim opacity-30 pointer-events-none"
              )}
            >
              <Icon name="copy" class="size-3" strokeWidth={1.5} />
              Copy
            </button>
          </Tooltip>
          <Tooltip content="Paste" shortcut="Ctrl+V">
            <button
              onClick={handlePaste}
              disabled={!hasSelection()}
              class={clsx(
                "flex h-[24px] items-center justify-center gap-1 rounded-[3px] border px-2 text-[11px] transition-colors",
                hasSelection()
                  ? "border-editor-field-border bg-editor-field text-editor-text hover:bg-editor-field/85 cursor-pointer"
                  : "border-transparent text-editor-text-dim opacity-30 pointer-events-none"
              )}
            >
              <Icon name="square-dashed" class="size-3" strokeWidth={1.5} />
              Paste
            </button>
          </Tooltip>
          <Tooltip content="Invert Selection" shortcut="Ctrl+I">
            <button
              onClick={handleInvert}
              disabled={!hasSelection()}
              class={clsx(
                "flex h-[24px] items-center justify-center gap-1 rounded-[3px] border px-2 text-[11px] transition-colors",
                hasSelection()
                  ? "border-editor-field-border bg-editor-field text-editor-text hover:bg-editor-field/85 cursor-pointer"
                  : "border-transparent text-editor-text-dim opacity-30 pointer-events-none"
              )}
            >
              <Icon name="flip-h" class="size-3" strokeWidth={1.5} />
              Invert
            </button>
          </Tooltip>
          <Tooltip content="Delete Selection Pixels" shortcut="Del">
            <button
              onClick={handleDelete}
              disabled={!hasSelection()}
              class={clsx(
                "flex h-[24px] items-center justify-center gap-1 rounded-[3px] border px-2 text-[11px] transition-colors",
                hasSelection()
                  ? "border-editor-field-border bg-editor-field text-editor-text hover:bg-editor-field/85 cursor-pointer"
                  : "border-transparent text-editor-text-dim opacity-30 pointer-events-none"
              )}
            >
              <Icon name="trash" class="size-3" strokeWidth={1.5} />
              Delete
            </button>
          </Tooltip>
          <Tooltip content="Deselect" shortcut="Esc">
            <button
              onClick={handleDeselect}
              disabled={!hasSelection()}
              class={clsx(
                "flex h-[24px] items-center justify-center gap-1 rounded-[3px] border px-2 text-[11px] transition-colors",
                hasSelection()
                  ? "border-editor-field-border bg-editor-field text-editor-text hover:bg-editor-field/85 cursor-pointer"
                  : "border-transparent text-editor-text-dim opacity-30 pointer-events-none"
              )}
            >
              <Icon name="x" class="size-3" strokeWidth={1.5} />
              Deselect
            </button>
          </Tooltip>
        </div>
      </MoreDropdown>
    </>
  );
}
