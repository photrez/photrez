import { createContext, useContext, createSignal, onMount, onCleanup, ParentProps, Show } from "solid-js";
import type { LayerDragPayload, DropTarget } from "./dragTypes";
import { useEditor } from "./shell/EditorContext";

const HOVER_TAB_DURATION_MS = 500;

// Maps a layer-drag payload + whether the current drop target is a *different*
// document to the HTML5 drag cursor. Same-document drags are reorders (move);
// cross-document drags copy by default and move (cut) when Alt is held. Used to
// drive `dataTransfer.dropEffect` so the OS cursor itself shows copy/move —
// no separate floating badge that could lag the pointer.
export function dragDropEffect(payload: LayerDragPayload | null, isCrossDoc: boolean): "copy" | "move" {
  if (!payload) return "move";
  if (isCrossDoc && !payload.isAltPressed) return "copy";
  return "move";
}

/** Convert a drag effect to a CSS cursor value.
 *  CSS `"copy"` shows the browser-synthesized copy cursor; the Rust
 *  `setDragNativeCursor` command overrides it with the OS native cursor
 *  via Win32 SetCursor. */
export function dragEffectToCssCursor(effect: "copy" | "move" | "default"): string {
  return effect;
}

export interface DragState {
  dragKind: "layer" | "file" | null;
  payload: LayerDragPayload | null;
  filePaths: string[] | null;
  dragStartPosition: { x: number; y: number } | null;
  dropTarget: DropTarget;
  hoverTabId: string | null;
  cascadeIndex: number;
}

export interface DragController {
  state: () => DragState;
  beginLayerDrag(payload: LayerDragPayload, ghostEl: HTMLElement | null): void;
  beginFileDrag(paths: string[], position: { x: number; y: number }): void;
  endDrag(): void;
  setDropTarget(target: DropTarget): void;
  startTabHover(tabId: string): void;
  cancelTabHover(): void;
  isTabHovering(tabId: string): boolean;
}

export interface WorkspaceLike {
  switchDocument(id: string): void;
}

const DragControllerContext = createContext<DragController>();

export function DragControllerProvider(props: ParentProps<{ workspaceOverride?: WorkspaceLike }>) {
  const editor = props.workspaceOverride ? null : useEditor();
  const workspace = (): WorkspaceLike => {
    if (props.workspaceOverride) return props.workspaceOverride;
    return editor!.workspace as WorkspaceLike;
  };

  const [state, setState] = createSignal<DragState>({
    dragKind: null,
    payload: null,
    filePaths: null,
    dragStartPosition: null,
    dropTarget: null,
    hoverTabId: null,
    cascadeIndex: 0,
  });

  let hoverTabTimerId: ReturnType<typeof setTimeout> | null = null;
  let cascadeCounter = 0;

  function cancelTabHoverInternal() {
    if (hoverTabTimerId !== null) {
      clearTimeout(hoverTabTimerId);
      hoverTabTimerId = null;
    }
    setState((s) => ({ ...s, hoverTabId: null }));
  }

  const api: DragController = {
    state,
    beginLayerDrag(payload, _ghostEl) {
      setState((s) => ({ ...s, dragKind: "layer", payload, dropTarget: null }));
    },
    beginFileDrag(paths, position) {
      cascadeCounter = 0;
      setState((s) => ({
        ...s,
        dragKind: "file",
        filePaths: paths,
        dragStartPosition: position,
        dropTarget: null,
        cascadeIndex: 0,
      }));
    },
    endDrag() {
      cancelTabHoverInternal();
      setState((s) => ({
        ...s,
        dragKind: null,
        payload: null,
        filePaths: null,
        dropTarget: null,
      }));
    },
    setDropTarget(target) {
      setState((s) => ({ ...s, dropTarget: target }));
    },
    startTabHover(tabId) {
      // Don't reset the timer if it's the same tab — the user is already
      // mid-hover and the countdown should keep ticking. Without this,
      // every pointermove (every pixel) cancels + restarts the 500ms
      // timer, so the user can never hold still long enough for it to
      // fire (and the visual countdown progress bar keeps resetting).
      if (state().hoverTabId === tabId && hoverTabTimerId !== null) {
        return;
      }
      cancelTabHoverInternal();
      setState((s) => ({ ...s, hoverTabId: tabId }));
      hoverTabTimerId = setTimeout(() => {
        workspace().switchDocument(tabId);
        cancelTabHoverInternal();
      }, HOVER_TAB_DURATION_MS);
    },
    cancelTabHover: cancelTabHoverInternal,
    isTabHovering(tabId) {
      return state().hoverTabId === tabId;
    },
  };

  return (
    <DragControllerContext.Provider value={api}>
      {props.children}
    </DragControllerContext.Provider>
  );
}

export function useDragController(): DragController {
  const ctx = useContext(DragControllerContext);
  if (!ctx) throw new Error("useDragController must be used within DragControllerProvider");
  return ctx;
}

// Mounts a document-level dragover handler that always calls
// preventDefault while a drag is in flight (dragKind !== null).
// Without this the browser shows the "forbidden" cursor over
// any non-drop zone (canvas, topbar, empty space) because it can't
// confirm a drop target accepts the drag. Per-zone handlers still
// call preventDefault themselves to opt into specific drop targets.
export function DragGlobalGuard() {
  const dragController = useDragController();
  onMount(() => {
    const onDragOver = (e: DragEvent) => {
      const state = dragController.state();
      if (state.dragKind !== null) {
        e.preventDefault();
      } else if (e.dataTransfer?.types?.includes("Files")) {
        // OS external file drag detected — set dragKind so the browser
        // allows the drop and shows the "copy" cursor.  File paths are
        // unknown at dragover time; the drop handler reads them from
        // e.dataTransfer.files instead of state.filePaths.
        dragController.beginFileDrag([], { x: e.clientX, y: e.clientY });
        e.preventDefault();
      }
    };

    // Clean up stuck drag state when OS file drag is aborted outside the
    // window (dragleave fires when cursor leaves the document, drop fires
    // on successful drop as safety net). Without these listeners, dragKind
    // stays "file" forever, blocking all subsequent drag operations.
    const onDragEnd = (e: DragEvent) => {
      if (dragController.state().dragKind === "file") {
        // MDN file drag-and-drop rule: the drop event must be cancelled at
        // window/document level, otherwise the browser performs its default
        // action (navigating to / opening the dropped file) when the drop
        // lands OUTSIDE any drop zone (topbar, tool rail, panel gaps).
        // Zone handlers already preventDefault themselves — this only
        // covers the no-zone case.
        e.preventDefault();
        dragController.endDrag();
      }
    };

    const onDragEnter = (e: DragEvent) => {
      // Like dragover: while a layer/file drag is in flight, every element the
      // cursor enters must be marked a valid drop target. Otherwise Chromium
      // decides the target is invalid at *dragenter* time and shows the no-drop
      // cursor, ignoring the `dropEffect` (copy/move) we set during dragover.
      if (dragController.state().dragKind !== null) {
        e.preventDefault();
      }
    };

    document.addEventListener("dragover", onDragOver);
    document.addEventListener("dragenter", onDragEnter);
    document.addEventListener("dragleave", onDragEnd);
    document.addEventListener("drop", onDragEnd);
    onCleanup(() => {
      document.removeEventListener("dragover", onDragOver);
      document.removeEventListener("dragenter", onDragEnter);
      document.removeEventListener("dragleave", onDragEnd);
      document.removeEventListener("drop", onDragEnd);
    });
  });
  return null;
}
