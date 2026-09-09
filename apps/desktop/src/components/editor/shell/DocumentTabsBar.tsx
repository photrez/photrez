import { For, Show, createSignal } from "solid-js";
import { clsx } from "clsx";
import { Icon } from "../icons";
import { Tooltip } from "../Tooltip";
import { useEditor } from "./EditorContext";
import { WorkspaceManager } from "@/engine/workspace";
import { MAX_OPEN_DOCUMENTS } from "@/engine/types";
import { cancelLayerTransformSession } from "../transformSession";
import { useDragController, dragDropEffect } from "../DragController";
import { addFilesAsLayers, addLayerFromCrossDoc, createNewDocsFromFiles, type WorkspaceFacade } from "../crossDocLayerOps";
import { showToast } from "../Toast";
import { useDialog } from "../dialogs/DialogProvider";
import { ContextMenu, type ContextMenuEntry } from "../ContextMenu";
import { exportActiveDocument } from "../exportDocument";
import { useI18n } from "@/i18n/I18nProvider";

function ExportButton() {
  const { t } = useI18n();
  const { setShowExportDialog, workspace, activeDocumentId, documents } = useEditor();
  const [menuOpen, setMenuOpen] = createSignal(false);
  const [menuPos, setMenuPos] = createSignal({ x: 0, y: 0 });

  const getDocInfo = () => {
    const id = activeDocumentId();
    if (!id) return null;
    const engine = workspace.getEngine(id);
    if (!engine) return null;
    const doc = documents().find((d) => d.id === id);
    return { engine, name: doc?.displayName || "Untitled" };
  };

  const handleQuickExport = async (format: "png" | "jpeg" | "webp", quality: number) => {
    const info = getDocInfo();
    if (!info) {
      showToast("No active document", "error");
      return;
    }
    try {
      const path = await exportActiveDocument(info.engine, info.name, format, quality);
      if (path) {
        showToast(`Saved to ${path.split(/[/\\]/).pop()}`, "info");
      }
    } catch (e) {
      showToast(`Export failed: ${e instanceof Error ? e.message : String(e)}`, "error");
    }
  };

  const menuItems: ContextMenuEntry[] = [
    {
      kind: "item",
      label: t("menus.items.quickExportPng", "Quick Export as PNG"),
      onSelect: () => handleQuickExport("png", 100),
    },
    {
      kind: "item",
      label: t("menus.items.quickExportJpg", "Quick Export as JPG"),
      onSelect: () => handleQuickExport("jpeg", 90),
    },
    {
      kind: "item",
      label: t("menus.items.quickExportWebp", "Quick Export as WebP"),
      onSelect: () => handleQuickExport("webp", 90),
    },
    { kind: "separator" },
    {
      kind: "item",
      label: t("menus.items.exportAs", "Export As..."),
      onSelect: () => setShowExportDialog(true),
    },
  ];

  return (
    <>
      <button
        onClick={(e) => {
          const rect = e.currentTarget.getBoundingClientRect();
          setMenuPos({ x: rect.right - 210, y: rect.bottom + 4 });
          setMenuOpen(true);
        }}
        class="flex h-[28px] shrink-0 items-center gap-2 rounded-[4px] border border-editor-field-border px-3 text-[12.5px] text-editor-text transition-colors hover:bg-white/[0.045] hover:text-editor-text"
      >
        {t("menus.items.export", "Export")}
        <Icon
          name="chevron-down"
          class="size-3.5 text-editor-text-dim"
          strokeWidth={1.75}
        />
      </button>
      <ContextMenu
        open={menuOpen()}
        x={menuPos().x}
        y={menuPos().y}
        ariaLabel={t("menus.items.export", "Export menu")}
        items={menuItems}
        onClose={() => setMenuOpen(false)}
      />
    </>
  );
}

function LayoutToggleButton() {
  const { t } = useI18n();
  const { rightDockLayout, setRightDockLayout } = useEditor();
  return (
    <Tooltip content={rightDockLayout() === "side-by-side" ? t("menus.items.useStackedSideDock", "Switch to Stacked Dock") : t("menus.items.useSideBySideSideDock", "Switch to Side-by-Side Dock")}>
      <button
        onClick={() => setRightDockLayout(rightDockLayout() === "side-by-side" ? "stacked" : "side-by-side")}
        class={clsx(
          "flex size-7 items-center justify-center rounded-[4px] text-editor-icon hover:bg-white/[0.045] hover:text-editor-text",
          rightDockLayout() === "stacked" && "text-editor-accent"
        )}
      >
        <Icon
          name="columns"
          class="size-4"
          strokeWidth={1.75}
        />
      </button>
    </Tooltip>
  );
}

function CloseDockButton() {
  const { t } = useI18n();
  const { setRightDockOpen } = useEditor();
  return (
    <button
      class="flex size-7 items-center justify-center rounded-[4px] text-editor-icon hover:bg-white/[0.045] hover:text-editor-text lg:hidden"
      aria-label={t("menus.items.closeSidePanels", "Close side panels")}
      onClick={() => setRightDockOpen(false)}
    >
      <Icon name="x" class="size-4" strokeWidth={1.75} />
    </button>
  );
}

export function DocumentTabsBar() {
  const { t } = useI18n();
  const { workspace, documents, activeDocumentId, renderer, scheduler, layerTransformSession, setLayerTransformSession } = useEditor();
  const drag = useDragController();
  const dialog = useDialog();
  const [isHovered, setIsHovered] = createSignal(false);
  const [reorderingTab, setReorderingTab] = createSignal<{
    id: string;
    sourceIndex: number;
    targetIndex: number;
    position: "left" | "right";
  } | null>(null);

  const cancelActiveTransformSession = () => {
    const engine = workspace.getActiveEngine();
    if (cancelLayerTransformSession(layerTransformSession(), engine)) {
      setLayerTransformSession(null);
      scheduler.requestRender();
    }
  };

  const handleSwitchTab = (id: string) => {
    if (layerTransformSession()) {
      cancelActiveTransformSession();
    }
    workspace.switchDocument(id);
    scheduler.requestRender();
  };

  const handleTabPointerDown = (e: PointerEvent, tabId: string, index: number) => {
    if (e.button !== 0) return;
    if ((e.target as HTMLElement).closest("button")) return;
    if (drag.state().dragKind !== null) return;

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
      }

      const tabListEl = targetEl.parentElement;
      if (!tabListEl) return;
      const tabElements = Array.from(tabListEl.querySelectorAll<HTMLElement>("[data-document-tab]"));
      let targetIndex = index;
      let position: "left" | "right" = "left";

      for (let i = 0; i < tabElements.length; i++) {
        const rect = tabElements[i].getBoundingClientRect();
        if (moveEvent.clientX >= rect.left && moveEvent.clientX <= rect.right) {
          targetIndex = i;
          position = moveEvent.clientX < rect.left + rect.width / 2 ? "left" : "right";
          break;
        } else if (i === 0 && moveEvent.clientX < rect.left) {
          targetIndex = 0;
          position = "left";
          break;
        } else if (i === tabElements.length - 1 && moveEvent.clientX > rect.right) {
          targetIndex = tabElements.length - 1;
          position = "right";
          break;
        }
      }

      setReorderingTab({
        id: tabId,
        sourceIndex: index,
        targetIndex,
        position,
      });
    };

    const onPointerUp = (upEvent: PointerEvent) => {
      try {
        targetEl.releasePointerCapture?.(upEvent.pointerId);
      } catch {
        // Safe fallback if already lost
      }
      targetEl.removeEventListener("pointermove", onPointerMove);
      targetEl.removeEventListener("pointerup", onPointerUp);
      targetEl.removeEventListener("pointercancel", onPointerUp);

      const reorderState = reorderingTab();
      setReorderingTab(null);

      if (hasDragged && reorderState) {
        let finalIndex = reorderState.targetIndex;
        if (reorderState.sourceIndex < reorderState.targetIndex) {
          finalIndex = reorderState.position === "left" ? reorderState.targetIndex - 1 : reorderState.targetIndex;
        } else if (reorderState.sourceIndex > reorderState.targetIndex) {
          finalIndex = reorderState.position === "right" ? reorderState.targetIndex + 1 : reorderState.targetIndex;
        }
        finalIndex = Math.max(0, Math.min(documents().length - 1, finalIndex));
        if (finalIndex !== index) {
          workspace.reorderDocument(index, finalIndex);
          scheduler.requestRender();
        }
      } else if (!hasDragged) {
        handleSwitchTab(tabId);
      }
    };

    targetEl.addEventListener("pointermove", onPointerMove);
    targetEl.addEventListener("pointerup", onPointerUp);
    targetEl.addEventListener("pointercancel", onPointerUp);
  };

  const handleCloseTab = async (e: MouseEvent, id: string) => {
    e.stopPropagation();
    if (layerTransformSession()) {
      cancelActiveTransformSession();
    }

    const session = workspace.getSession(id);
    if (session?.dirty) {
      const confirmed = await dialog.confirm({
        title: t("dialogs.unsavedChanges.title", "Unsaved Changes"),
        message: t("dialogs.unsavedChanges.message", { name: session.displayName }),
        confirmLabel: t("common.discard", "Discard"),
        cancelLabel: t("common.cancel", "Cancel"),
        tone: "danger",
      });
      if (!confirmed) return;
    }

    // Clean up WebGL textures for all layers in this document before removing
    if (session) {
      for (const layer of session.engine.getLayers()) {
        renderer.destroyTexture(layer.id);
      }
    }

    workspace.removeDocument(id);
    scheduler.requestRender();
  };

  const handleNewTab = async () => {
    if (layerTransformSession()) {
      cancelActiveTransformSession();
    }
    if (workspace.isFull()) {
      showToast(`Workspace full: close a document first (max ${MAX_OPEN_DOCUMENTS})`, "error");
      return;
    }
    const result = await dialog.newDocument();
    if (result) {
      const id = `doc-${crypto.randomUUID()}`;
      const session = WorkspaceManager.createBlankDocument(
        id,
        result.name,
        result.width,
        result.height,
        { backgroundColor: result.backgroundColor }
      );
      workspace.addDocument(session);
      scheduler.requestRender();
    }
  };

  // Pointer-driven hover detection (separate from HTML5 drag handlers
  // below). The canvas drag uses pointer events, not HTML5 drag events,
  // so the tab's onDragOver/onDragLeave don't fire for canvas drags.
  // We need pointerenter/pointerleave on the tab itself to start and
  // cancel the 500ms hover-to-switch timer.
  const handleTabPointerEnter = (e: PointerEvent, tabId: string) => {
    if (drag.state().dragKind === null) return;
    // Override the tab's CSS cursor:pointer with the drag cursor so
    // the copy/move cursor follows the pointer over tabs too.
    // We compute the drag cursor from the payload directly because
    // layerDragCursor lives in useCanvasDerivedState and is out of
    // scope here.
    const state = drag.state();
    if (state.payload) {
      const isCrossDoc = state.payload.sourceDocId !== tabId;
      const effect = dragDropEffect(state.payload, isCrossDoc);
      (e.currentTarget as HTMLElement).style.cursor = effect;
    }
    // Track the drop target so the canvas drag cursor (layerDragCursor in
    // useCanvasDerivedState) can detect cross-doc vs same-doc and show the
    // correct "copy" or "move" cursor — matching the HTML5 drag path which
    // sets dropTarget in handleTabDragOver.
    drag.setDropTarget({ type: "tab", docId: tabId });
    if (activeDocumentId() === tabId) {
      drag.cancelTabHover();
      return;
    }
    drag.startTabHover(tabId);
  };

  const handleTabPointerLeave = (e: PointerEvent, tabId: string) => {
    // Restore the tab's CSS cursor (remove inline override so cursor:pointer
    // from the CSS class takes back over).
    (e.currentTarget as HTMLElement | null)?.style.removeProperty("cursor");
    // Clear drop target if this tab was the target — mirrors the cleanup
    // in handleTabDragLeave for the HTML5 drag path.
    const current = drag.state().dropTarget;
    if (current && current.type === "tab" && current.docId === tabId) {
      drag.setDropTarget(null);
    }
    // Only cancel if the timer was for THIS tab. Otherwise we cancel
    // a timer started by another tab.
    if (drag.state().hoverTabId === tabId) {
      drag.cancelTabHover();
    }
  };

  const handleTabDragOver = (e: DragEvent, tabId: string) => {
    e.preventDefault();
    const state = drag.state();
    if (state.payload && e.dataTransfer) {
      e.dataTransfer.dropEffect = dragDropEffect(state.payload, state.payload.sourceDocId !== tabId);
    }
    drag.setDropTarget({ type: "tab", docId: tabId });
    if (activeDocumentId() !== tabId) {
      drag.startTabHover(tabId);
    } else {
      drag.cancelTabHover();
    }
  };

  const handleTabDragLeave = (e: DragEvent, tabId: string) => {
    const target = e.currentTarget;
    if (target && target instanceof Element && target.contains(e.relatedTarget as Node)) return;
    drag.cancelTabHover();
    const current = drag.state().dropTarget;
    if (current && current.type === "tab" && current.docId === tabId) {
      drag.setDropTarget(null);
    }
  };

  const handleTabDrop = async (e: DragEvent, tabId: string) => {
    e.preventDefault();
    drag.cancelTabHover();
    const state = drag.state();
    if (state.dragKind === "layer" && state.payload) {
      const engine = workspace.getEngine(tabId);
      if (engine) {
        const { newLayerId } = addLayerFromCrossDoc(
          state.payload,
          { type: "tab", docId: tabId },
          { x: engine.getWidth() / 2, y: engine.getHeight() / 2 },
          workspace
        );
        // Same-doc → reorder (same layer id, bitmap already uploaded).
        if (newLayerId && newLayerId !== state.payload.layerId) {
          const newLayer = engine.getLayer(newLayerId);
          if (newLayer?.imageBitmap) renderer.uploadImage(newLayerId, newLayer.imageBitmap);
        }
        scheduler.requestRender();
      }
    } else if (state.dragKind === "file" && state.filePaths) {
      const engine = workspace.getEngine(tabId);
      if (engine) {
        const created = await addFilesAsLayers(
          state.filePaths,
          { type: "tab", docId: tabId },
          { x: engine.getWidth() / 2, y: engine.getHeight() / 2 },
          workspace
        );
        for (const { layerId, bitmap } of created) {
          renderer.uploadImage(layerId, bitmap);
        }
        if (created.length) scheduler.requestRender();
      }
    }
    drag.endDrag();
  };

  const handleTabBarDragOver = (e: DragEvent) => {
    const target = e.target as HTMLElement | null;
    if (target && target.closest("[data-document-tab]")) return;
    e.preventDefault();
    drag.setDropTarget({ type: "tab-empty" });
    drag.cancelTabHover();
  };

  const handleTabBarDrop = async (e: DragEvent) => {
    e.preventDefault();
    const state = drag.state();
    if (state.dragKind === "file" && state.filePaths) {
      // a single downcast replaces the previous `as unknown as ...[1]`
      // double-assert noise.
      const facade: WorkspaceFacade = workspace;
      const created = await createNewDocsFromFiles(state.filePaths, facade);
      for (const { backgroundLayerId, bitmap } of created) {
        renderer.uploadImage(backgroundLayerId, bitmap);
      }
      if (created.length) scheduler.requestRender();
    }
    drag.endDrag();
  };

  const handleWheel = (e: WheelEvent) => {
    if (e.deltaY !== 0) {
      e.preventDefault();
      const container = e.currentTarget as HTMLElement | null;
      if (container) {
        container.scrollLeft += e.deltaY;
      }
    }
  };

  const handleTabListKeyDown = (e: KeyboardEvent) => {
    const docs = documents();
    if (docs.length < 2) return;
    const currentIdx = docs.findIndex((d) => d.id === activeDocumentId());
    if (currentIdx === -1) return;
    if (e.key === "ArrowRight" || e.key === "ArrowLeft") {
      e.preventDefault();
      const nextIdx = e.key === "ArrowRight"
        ? (currentIdx + 1) % docs.length
        : (currentIdx - 1 + docs.length) % docs.length;
      handleSwitchTab(docs[nextIdx].id);
    }
  };

  return (
    <div
      class="flex h-[38px] shrink-0 items-center border-b border-editor-divider bg-editor-topbar justify-between select-none"
      data-tab-bar-empty
      onDragOver={handleTabBarDragOver}
      onDrop={handleTabBarDrop}
    >
      <div 
        role="tablist"
        class={clsx(
          "flex items-stretch overflow-x-auto tab-scrollbar flex-1 h-full min-w-0",
          isHovered() && "is-hovered"
        )}
        onWheel={handleWheel}
        onKeyDown={handleTabListKeyDown}
        onMouseEnter={() => setIsHovered(true)}
        onMouseLeave={() => setIsHovered(false)}
      >
        <For each={documents()}>
          {(tab, idx) => {
            const isDragOver = () => {
              const dt = drag.state().dropTarget;
              return dt !== null && dt.type === "tab" && dt.docId === tab.id;
            };
            const isHovering = () => drag.state().hoverTabId === tab.id;
            // During a cross-doc layer drag, focus the user on the destination:
            // dim every tab that is neither the active doc nor the hovered target.
            const isDimmed = () => {
              const st = drag.state();
              if (st.dragKind !== "layer") return false;
              if (activeDocumentId() === tab.id) return false;
              if (isDragOver()) return false;
              return true;
            };

            const isBeingReordered = () => reorderingTab()?.id === tab.id;
            const isDropIndicatorLeft = () => {
              const r = reorderingTab();
              return r !== null && r.targetIndex === idx() && r.position === "left" && r.sourceIndex !== idx();
            };
            const isDropIndicatorRight = () => {
              const r = reorderingTab();
              return r !== null && r.targetIndex === idx() && r.position === "right" && r.sourceIndex !== idx();
            };

            return (
              <div
                role="tab"
                tabindex={activeDocumentId() === tab.id ? 0 : -1}
                aria-selected={activeDocumentId() === tab.id}
                data-document-tab={tab.id}
                data-drag-over={isDragOver() ? "tab" : null}
                data-hover-tab-progress={isHovering() ? "1" : null}
                onClick={() => handleSwitchTab(tab.id)}
                onPointerDown={(e) => handleTabPointerDown(e, tab.id, idx())}
                onPointerEnter={(e) => handleTabPointerEnter(e, tab.id)}
                onPointerLeave={(e) => handleTabPointerLeave(e, tab.id)}
                onDragOver={(e) => handleTabDragOver(e, tab.id)}
                onDragLeave={(e) => handleTabDragLeave(e, tab.id)}
                onDrop={(e) => handleTabDrop(e, tab.id)}
                class={clsx(
                  "group relative flex shrink-0 items-center gap-2 border-r border-editor-divider pl-3 pr-2.5 cursor-pointer select-none transition-all duration-75",
                  activeDocumentId() === tab.id ? "bg-editor-bg" : "bg-editor-topbar hover:bg-editor-topbar-hover",
                  isDragOver() && "outline outline-2 outline-editor-accent",
                  isDimmed() && "opacity-30",
                  isBeingReordered() && "opacity-60 scale-[0.98] ring-1 ring-editor-accent/60 bg-editor-divider/30",
                  isDropIndicatorLeft() && "before:absolute before:left-[-1px] before:top-0 before:bottom-0 before:w-[2px] before:bg-editor-accent before:z-30 before:rounded-full",
                  isDropIndicatorRight() && "after:absolute after:right-[-1px] after:top-0 after:bottom-0 after:w-[2px] after:bg-editor-accent after:z-30 after:rounded-full",
                )}
              >
                <span
                  class={clsx(
                    "whitespace-nowrap text-[11.5px]",
                    tab.isDirty
                      ? "text-editor-accent font-medium"
                      : activeDocumentId() === tab.id
                        ? "text-editor-text font-medium"
                        : "text-editor-text-dim",
                  )}
                >
                  {tab.displayName}
                  {tab.isDirty && <span class="ml-1 text-editor-accent">•</span>}
                </span>
                <button
                  onClick={(e) => handleCloseTab(e, tab.id)}
                  class="flex size-4 items-center justify-center rounded text-editor-text-dim hover:text-editor-text hover:bg-editor-app-hover"
                  aria-label={t("common.closeTab", "Close tab")}
                >
                  <Icon name="x" class="size-3.5" strokeWidth={1.75} />
                </button>
                <Show when={activeDocumentId() === tab.id}>
                  <span class="absolute inset-x-0 bottom-0 h-[2px] bg-editor-accent" />
                </Show>
              </div>
            );
          }}
        </For>
      </div>

      <button
        onClick={handleNewTab}
        class="flex w-11 shrink-0 items-center justify-center text-editor-icon hover:text-editor-text border-l border-editor-divider h-full"
        aria-label={t("menus.items.newDocument", "New document")}
      >
        <Icon name="plus" class="size-[18px]" strokeWidth={1.75} />
      </button>

      <div class="flex items-center gap-2 pr-4 pl-2 shrink-0 border-l border-editor-divider h-full">
        <LayoutToggleButton />
        <ExportButton />
        <CloseDockButton />
      </div>
    </div>
  );
}
