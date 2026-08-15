import { Show } from "solid-js";
import { clsx } from "clsx";
import { Icon } from "../icons";
import { Tooltip } from "../Tooltip";
import { showToast } from "../Toast";
import { LayerNode, DocumentModel } from "@/engine/types";
import { LayerThumb } from "./LayerThumb";
import { LAYER_DRAG_MIME, LayerDragPayload } from "../dragTypes";
import { useDragController } from "../DragController";

// actually touches. Avoids the production `any` while staying decoupled
// from the full WorkspaceManager/Scheduler types →LayerItem only needs
// the read-and-request paths, not the document lifecycle.
interface LayerItemWorkspaceFacade {
  getActiveEngine: () => {
    snapshot: () => DocumentModel;
    setLayerName: (id: string, name: string) => void;
  } | null;
  getActiveHistory: () => {
    commit: (snapshot: DocumentModel, label?: string) => void;
  } | null;
}

interface LayerItemSchedulerFacade {
  requestRender: () => void;
}

interface LayerItemProps {
  layer: LayerNode;
  idx: number;
  isActive: boolean;
  isSelected?: boolean;
  isEditing: boolean;
  editName: string;
  setEditingLayerId: (id: string | null) => void;
  setEditName: (name: string) => void;
  onSelect: (id: string, e: MouseEvent) => void;
  /** Text layers: double-click opens the text edit session (plan §7.3). */
  onEditText?: (layerId: string) => void;
  onContextMenu?: (event: MouseEvent, layer: LayerNode, idx: number) => void;
  onToggleVisibility: (e: MouseEvent, id: string) => void;
  onToggleLock: (e: MouseEvent, id: string) => void;
  onMoveUp: (e: MouseEvent, idx: number) => void;
  onMoveDown: (e: MouseEvent, idx: number) => void;
  onPointerDragMove?: (e: PointerEvent, layer: LayerNode, idx: number) => void;
  onPointerDragEnd?: (e: PointerEvent, layer: LayerNode, idx: number) => void;
  canMoveUp?: boolean;
  canMoveDown?: boolean;
  layersLength: number;
  workspace: LayerItemWorkspaceFacade;
  scheduler: LayerItemSchedulerFacade;
  activeDocumentId: string;
}

export function LayerItem(props: LayerItemProps) {
  const dragController = useDragController();

  // A layer can move up unless it's the top row or the (locked) Background;
  // it can move down unless the immediate layer below is the Background.
  const canMoveUp = props.canMoveUp ?? (props.idx > 0 && !props.layer.isBackground);
  const canMoveDown = props.canMoveDown ?? props.idx < props.layersLength - 1;

  const commitRename = () => {
    const nextName = props.editName.trim();
    if (!nextName || nextName === props.layer.name) {
      props.setEditingLayerId(null);
      return;
    }

    const engine = props.workspace.getActiveEngine();
    if (engine) {
      const history = props.workspace.getActiveHistory();
      history?.commit(engine.snapshot(), "Rename Layer");
      engine.setLayerName(props.layer.id, nextName);
      props.scheduler.requestRender();
    }
    props.setEditingLayerId(null);
  };

  const isThisLayerBeingDragged = () => {
    const state = dragController.state();
    if (state.dragKind !== "layer") return false;
    const payload = state.payload;
    return payload !== null && payload.layerId === props.layer.id;
  };

  const dropPositionForThisRow = () => {
    const state = dragController.state();
    if (state.dragKind !== "layer") return null;
    const target = state.dropTarget;
    if (!target || target.type !== "layers-panel") return null;
    if (target.insertAt !== props.idx) return null;
    return target.insertPosition ?? "above";
  };

  const handlePointerDown = (e: PointerEvent) => {
    if (e.button !== 0) return;
    if (props.layer.locked && !props.layer.isBackground) return;
    // Don't start drag on interactive buttons / input fields
    const target = e.target as HTMLElement;
    if (target.closest("button, input")) return;

    const startX = e.clientX;
    const startY = e.clientY;
    let hasDragged = false;
    const targetEl = e.currentTarget as HTMLElement;
    targetEl.setPointerCapture?.(e.pointerId);

    const onPointerMove = (moveEvent: PointerEvent) => {
      const dx = moveEvent.clientX - startX;
      const dy = moveEvent.clientY - startY;
      if (!hasDragged) {
        if (Math.hypot(dx, dy) < 4) return;
        hasDragged = true;
        const payload: LayerDragPayload = {
          version: 1,
          sourceDocId: props.activeDocumentId,
          layerId: props.layer.id,
          sourceName: props.layer.name,
          isAltPressed: moveEvent.altKey,
        };
        dragController.beginLayerDrag(payload, null);
      }

      if (hasDragged) {
        props.onPointerDragMove?.(moveEvent, props.layer, props.idx);
      }
    };

    const onPointerUp = (upEvent: PointerEvent) => {
      try {
        targetEl.releasePointerCapture?.(upEvent.pointerId);
      } catch {
        // Safe fallback
      }
      targetEl.removeEventListener("pointermove", onPointerMove);
      targetEl.removeEventListener("pointerup", onPointerUp);
      targetEl.removeEventListener("pointercancel", onPointerUp);

      if (hasDragged) {
        props.onPointerDragEnd?.(upEvent, props.layer, props.idx);
      } else {
        props.onSelect(props.layer.id, upEvent);
      }
    };

    targetEl.addEventListener("pointermove", onPointerMove);
    targetEl.addEventListener("pointerup", onPointerUp);
    targetEl.addEventListener("pointercancel", onPointerUp);
  };

  return (
    <div
      data-layer-idx={props.idx}
      onPointerDown={handlePointerDown}
      onClick={(e) => props.onSelect(props.layer.id, e)}
      onContextMenu={(event) => props.onContextMenu?.(event, props.layer, props.idx)}
      class={clsx(
        "flex h-[50px] items-center gap-2.5 px-3.5 cursor-grab select-none group border-b border-editor-divider/10 relative transition-all duration-75 touch-auto active:cursor-grabbing",
        props.isActive
          ? "bg-editor-row-active ring-1 ring-inset ring-editor-accent/40"
          : props.isSelected
            ? "bg-editor-row-active/70 ring-1 ring-inset ring-editor-accent/20"
            : "hover:bg-white/[0.03]",
        // Source layer being dragged: dimmed + amber ring + subtle scale
        isThisLayerBeingDragged() && "opacity-35 ring-1 ring-editor-accent/60 ring-inset scale-[0.98] border-dashed border-editor-accent/50 bg-editor-divider/20",
        // Drop insertion bar above this row (2px solid photon amber)
        dropPositionForThisRow() === "above" && "before:absolute before:top-[-1.5px] before:left-1 before:right-1 before:h-[2.5px] before:bg-editor-accent before:z-30 before:rounded-full",
        // Drop insertion bar below this row (2px solid photon amber)
        dropPositionForThisRow() === "below" && "after:absolute after:bottom-[-1.5px] after:left-1 after:right-1 after:h-[2.5px] after:bg-editor-accent after:z-30 after:rounded-full"
      )}
    >
      {/* Eye toggle button */}
      <Tooltip content={props.layer.visible ? "Hide Layer" : "Show Layer"}>
        <button
          data-layer-visibility
          onClick={(e) => props.onToggleVisibility(e, props.layer.id)}
          class="text-editor-icon hover:text-editor-text size-6 flex items-center justify-center z-10"
          aria-label={props.layer.visible ? "Hide Layer" : "Show Layer"}
        >
          <Icon
            name="eye"
            class={clsx("size-4 shrink-0", !props.layer.visible && "opacity-30")}
            strokeWidth={1.75}
          />
        </button>
      </Tooltip>

      {/* Layer Thumbnail */}
      <Show
        when={props.layer.type === "adjustment"}
        fallback={
          <div
            class="relative shrink-0"
            onDblClick={(e: MouseEvent) => {
              // Text layers: double-click the thumbnail opens the edit session.
              if (props.layer.type !== "text") return;
              if (props.layer.locked && !props.layer.isBackground) return;
              e.stopPropagation();
              props.onEditText?.(props.layer.id);
            }}
          >
            <LayerThumb layer={props.layer} isActive={props.isActive} />
            {/* Text layer glyph: "T" badge (plan §7.3) */}
            <Show when={props.layer.type === "text"}>
              <span
                data-text-layer-glyph
                aria-label="Text layer — double-click to edit"
                class="absolute -bottom-[3px] -right-[3px] size-[14px] rounded-[3px] bg-editor-accent text-white text-[9px] font-bold leading-none flex items-center justify-center border border-black/40 shadow-sm pointer-events-none select-none"
              >
                T
              </span>
            </Show>
          </div>
        }
      >
        {/* Adjustment Layer: Standard Black-and-White circular icon */}
        <div class="size-[34px] shrink-0 rounded-[3px] border border-black/40 bg-black flex items-center justify-center">
          <div
            class="size-[20px] rounded-full border border-white/20"
            style={{
              background: "conic-gradient(#fff 180deg, #222 180deg)",
              transform: "rotate(-45deg)"
            }}
          />
        </div>
      </Show>

      {/* Optional Layer Mask Thumbnail for Mountain layer (Matching high-fidelity mockup) */}
      <Show when={props.layer.name === "Mountain"}>
        <div class="size-[34px] shrink-0 rounded-[3px] border border-black/40 bg-black flex items-center justify-center relative overflow-hidden">
          <div class="absolute inset-[6px] bg-white rounded-full blur-[1px]" />
        </div>
      </Show>

      <Show
        when={props.isEditing}
        fallback={
          <span
            onDblClick={(e: MouseEvent) => {
              if (props.layer.locked && !props.layer.isBackground) return;
              e.stopPropagation();
              // Text layers: double-click opens the edit session instead of
              // rename. When no edit handler is wired (decoupled direct
              // renders), fall through to rename so the affordance survives.
              if (props.layer.type === "text" && props.onEditText) {
                props.onEditText(props.layer.id);
                return;
              }
              props.setEditingLayerId(props.layer.id);
              props.setEditName(props.layer.name);
            }}
            class="flex-1 text-[12.5px] text-editor-text truncate select-none"
          >
            {props.layer.name}
          </span>
        }
      >
        <input
          type="text"
          value={props.editName}
          onInput={(e) => props.setEditName(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              commitRename();
            } else if (e.key === "Escape") {
              props.setEditingLayerId(null);
            }
          }}
          onClick={(e) => e.stopPropagation()}
          onBlur={commitRename}
          class="flex-1 text-[12.5px] text-editor-text bg-editor-field border border-editor-field-border rounded px-1.5 focus:outline-none focus-visible:border-editor-accent h-[22px] min-w-0"
          ref={(el) => setTimeout(() => el?.focus(), 10)}
        />
      </Show>

      {/* Up and Down Chevrons for Reordering */}
      <div class="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity duration-100 pr-1">
        <Tooltip content="Move Layer Up">
          <button
            disabled={!canMoveUp}
            onClick={(e) => props.onMoveUp(e, props.idx)}
            class="size-[22px] flex items-center justify-center hover:bg-white/10 rounded disabled:opacity-20 disabled:hover:bg-transparent"
          >
            <Icon name="chevron-up" class="size-3.5" />
          </button>
        </Tooltip>
        <Tooltip content="Move Layer Down">
          <button
            disabled={!canMoveDown}
            onClick={(e) => props.onMoveDown(e, props.idx)}
            class="size-[22px] flex items-center justify-center hover:bg-white/10 rounded disabled:opacity-20 disabled:hover:bg-transparent"
          >
            <Icon name="chevron-down" class="size-3.5" />
          </button>
        </Tooltip>
      </div>

      {/* Adjustments Indicator */}
      <Show when={props.layer.hasAdjustments}>
        <Tooltip content="Layer has basic adjustments (brightness/contrast/saturation) applied">
          <div
            class="text-editor-accent size-6 flex items-center justify-center mr-1"
          >
            <Icon name="sliders" class="size-3.5 shrink-0" strokeWidth={1.75} />
          </div>
        </Tooltip>
      </Show>

      {/* Lock Indicator */}
      <button
        onClick={(e) => {
          if (props.layer.isBackground) {
            e.stopPropagation();
            showToast("Rename the layer to unlock", "warn");
            return;
          }
          props.onToggleLock(e, props.layer.id);
        }}
        class="text-editor-icon hover:text-editor-text size-6 flex items-center justify-center"
        aria-label={
          props.layer.isBackground
            ? "Background layer (rename to unlock)"
            : props.layer.locked
              ? "Unlock Layer"
              : "Lock Layer"
        }
      >
        <Icon
          name={props.layer.locked || props.layer.isBackground ? "lock" : "unlock"}
          class="size-3.5 shrink-0"
          strokeWidth={1.75}
        />
      </button>
    </div>
  );
}