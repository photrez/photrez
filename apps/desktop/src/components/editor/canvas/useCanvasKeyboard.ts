import { onMount, onCleanup } from "solid-js";
import { constrainCropRectToDocument } from "@/viewport/cropGeometry";
import { docFrameToScreenFrame, getModernCropApplyRotation, modernFrameToCropRect } from "@/viewport/modernCropGeometry";
import type { ToolType } from "@/viewport/input-handler";
import { useEditor } from "../shell/EditorContext";
import { flattenAllLayers, mergeActiveLayerDown, stampVisibleLayers, fillActiveLayerWithColor } from "../layers/layerOperations";
import { cancelLayerTransformSession, commitLayerTransformSession } from "../transformSession";
import { discardCropSession, applyCropPreview } from "../cropToolActions";
import { PAINT_SIZE_STEP_HARDNESS, paintSizeStep, adjustPaintSize, adjustPaintHardness } from "../brushToolState";
import { SelectionOperations } from "@/features/selection/SelectionOperations";
import { showToast } from "../Toast";
import { registerShortcut } from "../keyboardRegistry";

interface CanvasKeyboardOptions {
  isSpacePressed: () => boolean;
  setIsSpacePressed: (pressed: boolean) => void;
  isAltPressed: () => boolean;
  setIsAltPressed: (pressed: boolean) => void;
  isPanning: () => boolean;
  setIsPanning: (panning: boolean) => void;
  stopMomentum: () => void;
  onSelectionChange?: () => void;
  fitToScreenAndRender: (animated?: boolean) => void;
  syncViewport: () => void;
  getCanvasContainerRef: () => HTMLDivElement | undefined;
}

export function useCanvasKeyboard(options: CanvasKeyboardOptions) {
  const {
    workspace,
    renderer,
    scheduler,
    activeTool,
    setActiveTool,
    selectionShape,
    setSelectionShape,
    zoom,
    docWidth,
    docHeight,
    fgColor,
    bgColor,
    activeLayerId,
    cropRect,
    setCropRect,
    cropMode,
    cropSizeTarget,
    cropRotation,
    setCropRotation,
    hiddenCropPreview,
    setHiddenCropPreview,
    cropInteractionMode,
    undoLastCrop,
    redoCrop,
    canCropUndo,
    canCropRedo,
    undoModernCrop,
    redoModernCrop,
    commitModernCropState,
    commitCropState,
    undoTransformWithCurrent,
    redoTransformWithCurrent,
    clearTransformStacks,
    cropDeletePixels,
    cropFillEnabled,
    cropFillSource,
    cropFillCustomColor,
    modernCropFrame,
    modernCropImageTransform,
    setModernCropImageTransform,
    resetModernCrop,
    viewportWidth,
    viewportHeight,
    pan,
    setViewportState,
    selectedLayerId,
    setSelectedLayerId,
    layerTransformSession,
    setLayerTransformSession,
    selectionEditMode,
    setSelectionEditMode,
    brushSize,
    setBrushSize,
    eraserSize,
    setEraserSize,
    brushHardness,
    setBrushHardness,
    eraserHardness,
    setEraserHardness,
    brushOpacity,
    brushFlow,
    brushSmoothing,
    eraserOpacity,
    eraserFlow,
    eraserSmoothing,
    camera,
    syncFromCamera,
    setRenamingLayerId,
    setRenameLayerName,
    chromeVisible,
    setChromeVisible,
  } = useEditor();

  const resolvedCropFillColor = () => (
    cropFillSource() === "background"
      ? (typeof bgColor === "function" ? bgColor() : "#ffffff")
      : cropFillCustomColor()
  );
  const isCropFillEnabled = () => (
    typeof cropFillEnabled === "function" ? cropFillEnabled() : false
  );

  onMount(() => {
    // ── Register keyboard shortcuts (conflict detection) ──
    // Chain-of-responsibility with useEditorCommands: canvas captures keydown
    // FIRST (capture phase) so it can intercept Ctrl+Z/Y for transform mini-
    // undo/redo. When the mini stack is empty, it falls through by NOT calling
    // stopPropagation → useEditorCommands receives the event in bubble phase.
    // This ordering prevents useEditorCommands from eating the event before
    // canvas can decide (which happened when both used bubble-only).
    registerShortcut("Ctrl+Z", "useCanvasKeyboard", { intentional: true });     // transform undo → fallthrough
    registerShortcut("Ctrl+Y", "useCanvasKeyboard", { intentional: true });     // transform redo → fallthrough
    registerShortcut("Ctrl+Shift+Z", "useCanvasKeyboard", { intentional: true });
    // Canvas-only shortcuts:
    registerShortcut("Ctrl+Shift+T", "useCanvasKeyboard");
    registerShortcut("Ctrl+Shift+Alt+E", "useCanvasKeyboard");
    registerShortcut("Ctrl+E", "useCanvasKeyboard");
    registerShortcut("Ctrl+J", "useCanvasKeyboard");
    registerShortcut("Ctrl+Shift+N", "useCanvasKeyboard");
    registerShortcut("Ctrl+]", "useCanvasKeyboard");
    registerShortcut("Ctrl+[", "useCanvasKeyboard");
    registerShortcut("Ctrl+Shift+]", "useCanvasKeyboard");
    registerShortcut("Ctrl+Shift+[", "useCanvasKeyboard");
    registerShortcut("Ctrl+G", "useCanvasKeyboard");
    registerShortcut("Ctrl+Shift+G", "useCanvasKeyboard");
    registerShortcut("Ctrl+0", "useCanvasKeyboard");
    registerShortcut("B", "useCanvasKeyboard");
    registerShortcut("E", "useCanvasKeyboard");
    registerShortcut("V", "useCanvasKeyboard");
    registerShortcut("M", "useCanvasKeyboard");
    registerShortcut("C", "useCanvasKeyboard");
    registerShortcut("I", "useCanvasKeyboard");
    registerShortcut("Space", "useCanvasKeyboard");
    registerShortcut("Alt", "useCanvasKeyboard");
    registerShortcut("[", "useCanvasKeyboard");
    registerShortcut("]", "useCanvasKeyboard");
    registerShortcut("Shift+[", "useCanvasKeyboard");
    registerShortcut("Shift+]", "useCanvasKeyboard");
    registerShortcut("0-9", "useCanvasKeyboard");
    registerShortcut("Delete", "useCanvasKeyboard");
    registerShortcut("Backspace", "useCanvasKeyboard");
    registerShortcut("Alt+Delete", "useCanvasKeyboard");
    registerShortcut("Ctrl+Delete", "useCanvasKeyboard");
    registerShortcut("Alt+Backspace", "useCanvasKeyboard");
    registerShortcut("Ctrl+Backspace", "useCanvasKeyboard");
    registerShortcut("F2", "useCanvasKeyboard");
    registerShortcut("Escape", "useCanvasKeyboard");
    registerShortcut("Enter", "useCanvasKeyboard");
    registerShortcut("Arrow keys", "useCanvasKeyboard");

    const handleKeyDown = async (e: KeyboardEvent) => {
      const active = document.activeElement;
      if (
        active &&
        (active.tagName === "INPUT" ||
          active.tagName === "TEXTAREA" ||
          (active as HTMLElement).isContentEditable)
      ) {
        return;
      }

      if (document.querySelector('[aria-modal="true"]')) return;

      options.stopMomentum();
      if (e.defaultPrevented) return;

      // Ctrl+Shift+T: Toggle UI chrome (panels and toolbars)
      if (e.key === "T" && e.ctrlKey && e.shiftKey && !e.altKey && !e.metaKey) {
        e.preventDefault();
        e.stopPropagation();
        setChromeVisible((v) => !v);
        return;
      }

      // Dev-only: F5 reloads the webview when HMR gets stuck
      if (import.meta.env.DEV && e.key === "F5") {
        e.preventDefault();
        window.location.reload();
        return;
      }

      const engine = workspace.getActiveEngine();
      if (!engine) return;

      const history = workspace.getActiveHistory();
      if (!history) return;

      // Alt+Delete / Alt+Backspace → fill active layer with foreground color.
      // Ctrl+Delete / Ctrl+Backspace → fill active layer with background color.
      // (Solid fill of the whole layer.) Plain Delete/Backspace
      // has no modifier here, so it falls through to the existing
      // delete-layer / delete-selection behavior further below.
      if (e.key === "Delete" || e.key === "Backspace") {
        // Block during active transform session: fill/delete commit to global
        // history, but Ctrl+Z during a transform session only reaches the
        // session's local undo stack, making the action un-undoable.
        if (layerTransformSession()) {
          e.preventDefault();
          e.stopPropagation();
          return;
        }
        // Same rationale for the crop tool: the crop mini-undo stack owns
        // Ctrl+Z while a crop rect is live, so a destructive fill/delete here
        // would not be reachable from the crop undo path.
        if (activeTool() === "crop") {
          e.preventDefault();
          e.stopPropagation();
          return;
        }
        const resolveColor = (c: string | (() => string)) =>
          typeof c === "function" ? c() : c;
        if (e.altKey) {
          e.preventDefault();
          e.stopPropagation();
          if (!engine.getActiveLayerId()) {
            showToast("No editable layer selected", "warn");
          } else if (fillActiveLayerWithColor(engine, history, renderer, resolveColor(fgColor))) {
            scheduler.requestRender();
          } else {
            showToast("Could not fill layer", "warn");
          }
          return;
        }
        if (e.ctrlKey || e.metaKey) {
          e.preventDefault();
          e.stopPropagation();
          if (!engine.getActiveLayerId()) {
            showToast("No editable layer selected", "warn");
          } else if (fillActiveLayerWithColor(engine, history, renderer, resolveColor(bgColor))) {
            scheduler.requestRender();
          } else {
            showToast("Could not fill layer", "warn");
          }
          return;
        }
      }

      // F2: Rename active layer
      if (e.key === "F2") {
        e.preventDefault();
        e.stopPropagation();
        const activeId = engine.getActiveLayerId();
        if (activeId) {
          const layer = engine.getLayer(activeId);
          setRenamingLayerId(activeId);
          setRenameLayerName(layer?.name ?? "");
        }
        return;
      }

      // Layer transform session keyboard shortcuts (takes precedence over crop/tool shortcuts)
      if (layerTransformSession()) {
        const ctrl = e.ctrlKey || e.metaKey;
        const key = e.key.toLowerCase();

        // Ctrl+Z / Ctrl+Y: transform mini undo/redo within the active session.
        // Each pointerDown for a resize/rotate gesture saves a snapshot to the
        // mini undo stack, so these keys revert/restore individual gestures.
        if (ctrl && key === "z" && !e.shiftKey) {
          const session = layerTransformSession();
          const layer = engine.getLayer(session!.layerId);
          if (layer) {
            const entry = undoTransformWithCurrent(layer.transform);
            if (entry) {
              e.preventDefault();
              e.stopPropagation();
              engine.transformLayer(layer.id, entry.transform);
              scheduler.requestRender();
              return;
            }
            // Mini undo stack empty — fall through so useEditorCommands.ts
            // can cancel the session (existing Ctrl+Z behavior).
          }
        }

        if ((ctrl && key === "y") || (ctrl && e.shiftKey && key === "z")) {
          const session = layerTransformSession();
          const layer = engine.getLayer(session!.layerId);
          if (layer) {
            const entry = redoTransformWithCurrent(layer.transform);
            if (entry) {
              e.preventDefault();
              e.stopPropagation();
              engine.transformLayer(layer.id, entry.transform);
              scheduler.requestRender();
              return;
            }
            // Mini redo stack empty — fall through to general redo handler.
          }
        }

        if (e.key === "Enter") {
          e.preventDefault();
          e.stopPropagation();
          if (commitLayerTransformSession(layerTransformSession(), engine, history)) {
            clearTransformStacks();
            setLayerTransformSession(null);
            scheduler.requestRender();
          }
          return;
        }

        if (e.key === "Escape") {
          e.preventDefault();
          e.stopPropagation();
          if (cancelLayerTransformSession(layerTransformSession(), engine)) {
            clearTransformStacks();
            setLayerTransformSession(null);
            scheduler.requestRender();
          }
          return;
        }
      }

      // Crop tool keyboard shortcuts
      if (activeTool() === "crop") {
        const ctrl = e.ctrlKey || e.metaKey;
      const key = e.key.toLowerCase();
        if (ctrl && e.shiftKey && key === "z") {
          e.preventDefault();
          e.stopPropagation();
          if (cropInteractionMode() === "modern") {
            redoModernCrop();
          } else if (canCropRedo()) {
            const entry = redoCrop();
            if (entry) {
              setCropRect(entry.rect);
              setCropRotation(entry.rotation);
            }
          }
          return;
        }
        if (ctrl && key === "z") {
          e.preventDefault();
          e.stopPropagation();
          if (cropInteractionMode() === "modern") {
            undoModernCrop();
          } else if (canCropUndo()) {
            const entry = undoLastCrop();
            if (entry) {
              setCropRect(entry.rect);
              setCropRotation(entry.rotation);
            }
          }
          return;
        }
        if (ctrl && key === "y") {
          e.preventDefault();
          e.stopPropagation();
          if (cropInteractionMode() === "modern") {
            redoModernCrop();
          } else if (canCropRedo()) {
            const entry = redoCrop();
            if (entry) {
              setCropRect(entry.rect);
              setCropRotation(entry.rotation);
            }
          }
          return;
        }
          if (e.key === "Enter") {
            e.preventDefault();
            if (cropInteractionMode() === "modern" && modernCropFrame()) {
              // Convert doc-coord frame to screen space for modernFrameToCropRect
              const screenFrame = docFrameToScreenFrame(modernCropFrame(), zoom(), pan());
              const rect = modernFrameToCropRect({
                frame: screenFrame!,
                viewport: {
                  width: viewportWidth(),
                  height: viewportHeight(),
                  panX: pan().x,
                  panY: pan().y,
                  zoom: zoom(),
                },
                transform: modernCropImageTransform(),
              });
              applyCropPreview({
                workspace, renderer,
                viewport: { width: viewportWidth(), height: viewportHeight() },
                cropRect: rect,
              cropMode: cropMode(),
              cropSizeTarget: cropSizeTarget(),
              cropDeletePixels: cropDeletePixels(),
              cropFillColor: isCropFillEnabled() ? resolvedCropFillColor() : null,
              cropRotation: getModernCropApplyRotation(modernCropImageTransform().rotation),
              scheduler,
              setCropRect, setCropRotation, setHiddenCropPreview, setActiveTool,
              setSelectedLayerId,
              recenterViewport: options.fitToScreenAndRender,
            });
            resetModernCrop();
          } else {
            applyCropPreview({
              workspace,
              renderer,
              viewport: { width: viewportWidth(), height: viewportHeight() },
              cropRect: cropRect(),
              cropMode: cropMode(),
              cropSizeTarget: cropSizeTarget(),
              cropDeletePixels: cropDeletePixels(),
              cropFillColor: isCropFillEnabled() ? resolvedCropFillColor() : null,
              cropRotation: cropRotation(),
              scheduler,
              setCropRect,
              setCropRotation,
              setHiddenCropPreview,
              setActiveTool,
              setSelectedLayerId,
              recenterViewport: options.fitToScreenAndRender,
            });
          }
          return;
        }
        if (e.key === "Escape") {
          e.preventDefault();
          setCropRect(null);
          setCropRotation(0);
          setHiddenCropPreview(null);
          if (cropInteractionMode() === "modern") {
            resetModernCrop();
          }
          scheduler.requestRender();
          return;
        }
        if (e.key.startsWith("Arrow") && (cropRect() || modernCropFrame())) {
          e.preventDefault();
          const step = e.shiftKey ? 10 : 1;
          let dx = 0;
          let dy = 0;
          if (e.key === "ArrowUp") dy = -step;
          else if (e.key === "ArrowDown") dy = step;
          else if (e.key === "ArrowLeft") dx = -step;
          else if (e.key === "ArrowRight") dx = step;

          if (cropInteractionMode() === "modern" && modernCropFrame()) {
            if (!e.repeat) commitModernCropState();
            const t = modernCropImageTransform();
            setModernCropImageTransform({
              ...t,
              offsetX: t.offsetX + dx,
              offsetY: t.offsetY + dy,
            });
          } else {
            const rect = cropRect()!;
            if (!e.repeat) commitCropState(rect, cropRotation());
            const newRect = constrainCropRectToDocument(
              { ...rect, x: rect.x + dx, y: rect.y + dy },
              docWidth(),
              docHeight()
            );

            const actualDx = newRect.x - rect.x;
            const actualDy = newRect.y - rect.y;

            setCropRect(newRect);

            const currentPan = pan();
            setViewportState({
              x: currentPan.x - actualDx * zoom(),
              y: currentPan.y - actualDy * zoom(),
              zoom: zoom(),
            });
          }
          scheduler.requestRender();
          return;
        }
      }

      // Selection tool keyboard shortcuts
      if (activeTool() === "selection") {
        // Ctrl+D: Deselect
        if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "d") {
          e.preventDefault();
          e.stopPropagation();
          engine.clearSelection();
          setSelectionEditMode(false);
          options.onSelectionChange?.();
          scheduler.requestRender();
          return;
        }

        // Ctrl+I: Invert selection
        if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "i") {
          e.preventDefault();
          e.stopPropagation();
          engine.invertSelection();
          setSelectionEditMode(false);
          options.onSelectionChange?.();
          scheduler.requestRender();
          return;
        }

        // Ctrl+T: Toggle transform/edit mode (show resize/rotate handles)
        if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "t") {
          e.preventDefault();
          e.stopPropagation();
          if (engine.getSelection()) {
            setSelectionEditMode(!selectionEditMode());
            scheduler.requestRender();
          }
          return;
        }

        // Escape: Cancel drawing / deselect
        if (e.key === "Escape") {
          e.preventDefault();
          engine.clearSelection();
          setSelectionEditMode(false);
          options.onSelectionChange?.();
          scheduler.requestRender();
          return;
        }

        // Ctrl+X: Cut selection
        if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "x") {
          e.preventDefault();
          e.stopPropagation();
          if (engine.getSelection()) {
            // Commit pre-action snapshot so the cut is undoable AND redoable.
            // Without this, the post-cut state was never pushed to the undo
            // stack and redo had no entry to replay.
            history.commit(engine.snapshot(), "Cut");
            SelectionOperations.cutSelection(engine);
            // Re-upload the modified layer's bitmap to the renderer so the
            // canvas reflects the cut immediately (otherwise the GPU texture
            // still holds the pre-cut pixels until the next texture refresh).
            const activeId = engine.getActiveLayerId();
            if (activeId) {
              const layer = engine.getLayer(activeId);
              if (layer?.imageBitmap) {
                renderer.uploadImage(layer.id, layer.imageBitmap);
              }
            }
            options.onSelectionChange?.();
            scheduler.requestRender();
          }
          return;
        }

        // Ctrl+C: Copy selection
        if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "c") {
          e.preventDefault();
          e.stopPropagation();
          if (engine.getSelection()) {
            SelectionOperations.copySelection(engine);
            scheduler.requestRender();
          }
          return;
        }

        // Ctrl+V: Paste selection
        if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "v") {
          e.preventDefault();
          e.stopPropagation();
          // Commit pre-action snapshot so the new layer is undoable/redoable.
          history.commit(engine.snapshot(), "Paste");
          SelectionOperations.pasteSelection(engine);
          // Re-upload the newly-pasted layer's bitmap to the renderer. After
          // pasteSelection, engine.getActiveLayerId() points at the new
          // "Pasted Layer" (addLayer sets activeLayerId to the new layer).
          const pastedId = engine.getActiveLayerId();
          if (pastedId) {
            const pastedLayer = engine.getLayer(pastedId);
            if (pastedLayer?.imageBitmap) {
              renderer.uploadImage(pastedLayer.id, pastedLayer.imageBitmap);
            }
          }
          options.onSelectionChange?.();
          scheduler.requestRender();
          return;
        }

        // Delete / Backspace: Delete selection pixels
        if (e.key === "Delete" || e.key === "Backspace") {
          e.preventDefault();
          e.stopPropagation();
          const sel = engine.getSelection();
          if (sel) {
            // Commit pre-action snapshot so the deletion is undoable/redoable.
            history.commit(engine.snapshot(), "Delete Pixels");
            SelectionOperations.deleteSelection(engine);
            // Re-upload the modified layer's bitmap to the renderer so the
            // canvas reflects the deletion immediately.
            const activeId = engine.getActiveLayerId();
            if (activeId) {
              const layer = engine.getLayer(activeId);
              if (layer?.imageBitmap) {
                renderer.uploadImage(layer.id, layer.imageBitmap);
              }
            }
            options.onSelectionChange?.();
            scheduler.requestRender();
          }
          return;
        }
      }

      const key = e.key.toLowerCase();

      const ctrl = e.ctrlKey || e.metaKey;

      // Block destructive layer operations during an active transform session:
      // they would commit to global history, but Ctrl+Z during a transform only
      // reaches the session's local undo stack, making them un-undoable.
      // (Flip / Ctrl+G is intentionally allowed — it mutates the transform and
      // is captured by the session's mini undo stack.)
      if (layerTransformSession()) {
        if (
          (ctrl && e.shiftKey && e.altKey && key === "e") ||  // Stamp Visible
          (ctrl && key === "e") ||                            // Merge / Flatten
          (ctrl && key === "j") ||                            // Duplicate layer
          (ctrl && e.shiftKey && key === "n") ||              // New layer
          (ctrl && (e.key === "]" || e.key === "["))          // Reorder (incl. shift)
        ) {
          e.preventDefault();
          e.stopPropagation();
          return;
        }
      }

      // Stamp Visible: Ctrl+Shift+Alt+E — composite all visible layers into a new top layer
      if (ctrl && e.shiftKey && e.altKey && key === "e") {
        e.preventDefault();
        e.stopPropagation();
        if (!engine.getActiveLayerId()) {
          showToast("No layer selected", "warn");
        } else if (stampVisibleLayers(engine, history, renderer)) {
          scheduler.requestRender();
        } else {
          showToast("Nothing to stamp", "warn");
        }
        return;
      }

      if (ctrl && key === "e") {
        e.preventDefault();
        e.stopPropagation();

        const activeId = engine.getActiveLayerId();
        if (e.shiftKey) {
          if (flattenAllLayers(engine, history, renderer)) {
            scheduler.requestRender();
          } else {
            showToast("Could not flatten layers", "warn");
          }
        } else if (activeId) {
          if (mergeActiveLayerDown(engine, history, renderer, activeId)) {
            scheduler.requestRender();
          } else {
            showToast("Could not merge layers", "warn");
          }
        } else {
          showToast("No layer selected", "warn");
        }

        return;
      }

      if (ctrl && key === "j") {
        e.preventDefault();
        e.stopPropagation();
        const activeId = engine.getActiveLayerId();
        if (activeId) {
          history.commit(engine.snapshot(), "Duplicate Layer");
          try {
            const dup = engine.duplicateLayer(activeId);
            if (dup.imageBitmap) {
              renderer.uploadImage(dup.id, dup.imageBitmap);
            }
            scheduler.requestRender();
          } catch (err) {
            showToast(`Cannot duplicate layer: ${(err as Error).message}`, "error");
          }
        }
        return;
      }

      // Layer: Ctrl+Shift+N - Add new layer
      if (ctrl && e.shiftKey && key === "n") {
        e.preventDefault();
        e.stopPropagation();
        history.commit(engine.snapshot(), "New Layer");
        try {
          engine.addLayer(`Layer ${engine.getLayers().length + 1}`);
          scheduler.requestRender();
        } catch (err) {
          showToast(`Cannot add layer: ${(err as Error).message}`, "error");
        }
        return;
      }

      // Layer: Ctrl+] - Move active layer up in stack (towards top, index 0)
      if (ctrl && !e.shiftKey && e.key === "]") {
        e.preventDefault();
        e.stopPropagation();
        const activeId = engine.getActiveLayerId();
        if (activeId) {
          const idx = engine.getLayers().findIndex((l) => l.id === activeId);
          if (idx > 0) {
            history.commit(engine.snapshot(), "Reorder Layer");
            engine.reorderLayer(idx, idx - 1);
            scheduler.requestRender();
          }
        }
        return;
      }

      // Layer: Ctrl+[ - Move active layer down in stack (towards bottom)
      if (ctrl && !e.shiftKey && e.key === "[") {
        e.preventDefault();
        e.stopPropagation();
        const activeId = engine.getActiveLayerId();
        if (activeId) {
          const stack = engine.getLayers();
          const idx = stack.findIndex((l) => l.id === activeId);
          if (idx >= 0 && idx < stack.length - 1) {
            history.commit(engine.snapshot(), "Reorder Layer");
            engine.reorderLayer(idx, idx + 1);
            scheduler.requestRender();
          }
        }
        return;
      }

      // Layer: Ctrl+Shift+] - Move active layer to top of stack
      if (ctrl && e.shiftKey && e.key === "]") {
        e.preventDefault();
        e.stopPropagation();
        const activeId = engine.getActiveLayerId();
        if (activeId) {
          const idx = engine.getLayers().findIndex((l) => l.id === activeId);
          if (idx > 0) {
            history.commit(engine.snapshot(), "Reorder Layer");
            engine.reorderLayer(idx, 0);
            scheduler.requestRender();
          }
        }
        return;
      }

      // Layer: Ctrl+Shift+[ - Move active layer to bottom of stack
      if (ctrl && e.shiftKey && e.key === "[") {
        e.preventDefault();
        e.stopPropagation();
        const activeId = engine.getActiveLayerId();
        if (activeId) {
          const stack = engine.getLayers();
          const idx = stack.findIndex((l) => l.id === activeId);
          if (idx >= 0 && idx < stack.length - 1) {
            history.commit(engine.snapshot(), "Reorder Layer");
            engine.reorderLayer(idx, stack.length - 1);
            scheduler.requestRender();
          }
        }
        return;
      }

      // Layer: Ctrl+G - Flip horizontal, Ctrl+Shift+G - Flip vertical
      if (ctrl && key === "g") {
        e.preventDefault();
        e.stopPropagation();
        const activeId = engine.getActiveLayerId();
        if (!activeId) {
          showToast("No layer selected", "warn");
          return;
        }
        if (activeId) {
          const layer = engine.getLayer(activeId);
          if (layer && !layer.locked) {
            history.commit(engine.snapshot(), "Flip Layer");
            engine.flipLayer(activeId, e.shiftKey ? "v" : "h");
            scheduler.requestRender();
          }
        }
        return;
      }

      // Layer: Delete / Backspace - Delete active layer
      // (Selection tool handles this earlier when in selection mode.)
      if (e.key === "Delete" || e.key === "Backspace") {
        e.preventDefault();
        e.stopPropagation();
        const activeId = engine.getActiveLayerId();
        if (!activeId) {
          showToast("No layer selected", "warn");
          return;
        }
        if (activeId && engine.getLayers().length > 1) {
          history.commit(engine.snapshot(), "Delete Layer");
          engine.deleteLayer(activeId);
          scheduler.requestRender();
        }
        return;
      }

      // Layer: 0-9 (no modifier) - Set active layer opacity
      // 0 = 100%, 1 = 10%, 2 = 20%, ..., 9 = 90%
      if (!ctrl && !e.shiftKey && !e.altKey && e.key.length === 1 && e.key >= "0" && e.key <= "9") {
        const activeId = engine.getActiveLayerId();
        if (activeId) {
          const layer = engine.getLayer(activeId);
          if (layer && !layer.locked) {
            e.preventDefault();
            e.stopPropagation();
            const digit = e.key.charCodeAt(0) - 48;
            const opacity = digit === 0 ? 1.0 : digit / 10;
            if (layer.opacity === opacity) return;
            history.commit(engine.snapshot(), "Layer Opacity");
            engine.setLayerOpacity(activeId, opacity);
            scheduler.requestRender();
            return;
          }
        }
      }

      // Paint tool shortcuts
      if (!ctrl && key === "b") {
        e.preventDefault();
        setActiveTool("brush");
        scheduler.requestRender();
        return;
      }

      if (!ctrl && key === "e") {
        e.preventDefault();
        setActiveTool("eraser");
        scheduler.requestRender();
        return;
      }

      // Tool selection shortcuts
      if (!ctrl && key === "v") {
        e.preventDefault();
        setActiveTool("move");
        scheduler.requestRender();
        return;
      }

      if (!ctrl && key === "m") {
        e.preventDefault();
        if (e.shiftKey) {
          // Shift+M toggles the marquee shape between rect and ellipse.
          setSelectionShape(selectionShape() === "ellipse" ? "rect" : "ellipse");
        } else {
          setSelectionShape("rect");
          setActiveTool("selection");
        }
        scheduler.requestRender();
        return;
      }

      if (!ctrl && key === "c") {
        e.preventDefault();
        setActiveTool("crop");
        scheduler.requestRender();
        return;
      }

      if (!ctrl && key === "g") {
        e.preventDefault();
        if (e.shiftKey) {
          setActiveTool("gradient");
        } else {
          setActiveTool("paintBucket");
        }
        scheduler.requestRender();
        return;
      }

      if (!ctrl && key === "i") {
        e.preventDefault();
        setActiveTool("eyedropper");
        scheduler.requestRender();
        return;
      }

      if (!ctrl && (e.key === "[" || e.key === "]") && (activeTool() === "brush" || activeTool() === "eraser")) {
        e.preventDefault();
        if (e.shiftKey) {
          const delta = e.key === "[" ? -PAINT_SIZE_STEP_HARDNESS : PAINT_SIZE_STEP_HARDNESS;
          const next = adjustPaintHardness(activeTool(), {
            brushSize: brushSize(),
            brushHardness: brushHardness(),
            brushOpacity: brushOpacity(),
            brushFlow: brushFlow(),
            brushSmoothing: brushSmoothing(),
            eraserSize: eraserSize(),
            eraserHardness: eraserHardness(),
            eraserOpacity: eraserOpacity(),
            eraserFlow: eraserFlow(),
            eraserSmoothing: eraserSmoothing(),
          }, delta);
          setBrushHardness(next.brushHardness);
          setEraserHardness(next.eraserHardness);
        } else {
          const currentSize = activeTool() === "eraser" ? eraserSize() : brushSize();
          const step = paintSizeStep(currentSize);
          const delta = e.key === "[" ? -step : step;
          const next = adjustPaintSize(activeTool(), {
            brushSize: brushSize(),
            brushHardness: brushHardness(),
            brushOpacity: brushOpacity(),
            brushFlow: brushFlow(),
            brushSmoothing: brushSmoothing(),
            eraserSize: eraserSize(),
            eraserHardness: eraserHardness(),
            eraserOpacity: eraserOpacity(),
            eraserFlow: eraserFlow(),
            eraserSmoothing: eraserSmoothing(),
          }, delta);
          setBrushSize(next.brushSize);
          setEraserSize(next.eraserSize);
        }
        scheduler.requestRender();
        return;
      }

      // Alt key tracking for eyedropper shortcut
      if (e.key === "Alt") {
        e.preventDefault();
        options.setIsAltPressed(true);
      }

      // Spacebar panning toggle
      if (e.code === "Space") {
        e.preventDefault();
        // Blur any focused element (e.g. OptionCheckbox native input) so Space
        // doesn't toggle the control while we're trying to pan. The native checkbox
        // default behavior fires before the window keydown handler even with
        // preventDefault, so we proactively remove focus from the active element.
        (document.activeElement as HTMLElement)?.blur();
        options.stopMomentum();
        if (!options.isSpacePressed()) {
          options.setIsSpacePressed(true);
        }
        return;
      }

      // Escape deselects layer in Move tool
      if (activeTool() === "move" && e.key === "Escape" && selectedLayerId()) {
        e.preventDefault();
        engine.setActiveLayer(null);
        setSelectedLayerId(null);
        scheduler.requestRender();
        return;
      }

      // Keyboard nudge for Move Tool: Arrow = 1px, Shift+Arrow = 10px.
      // Works whether or not the layer transform overlay (handles/rotate ring)
      // is active, matching standard raster editors — arrow nudges the selected
      // layer 1px (10px with Shift) even while the transform session is live.
      if (activeTool() === "move" && e.key.startsWith("Arrow")) {
        const activeId = engine.getActiveLayerId();
        if (!activeId) return;
        const layer = engine.getLayer(activeId);
        if (!layer || layer.locked) return;

        e.preventDefault();
        const step = e.shiftKey ? 10 : 1;
        let dx = 0, dy = 0;
        if (e.key === "ArrowUp") dy = -step;
        else if (e.key === "ArrowDown") dy = step;
        else if (e.key === "ArrowLeft") dx = -step;
        else if (e.key === "ArrowRight") dx = step;

        if (!e.repeat) {
          history.commit(engine.snapshot(), "Move Layer");
        }
        engine.moveLayer(activeId, layer.transform.x + dx, layer.transform.y + dy);
        scheduler.requestRender();
        return;
      }

      // Zoom Shortcuts: Ctrl + Plus / Equal
      // NOTE: Handled by useEditorCommands.ts with animation
      /*
      if (
        ctrl &&
        (key === "=" ||
          key === "+" ||
          e.code === "Equal" ||
          e.code === "NumpadAdd")
      ) {
        e.preventDefault();
        e.stopPropagation();
        options.stopMomentum();

        const canvasContainerRef = options.getCanvasContainerRef();
        if (canvasContainerRef) {
          const rect = canvasContainerRef.getBoundingClientRect();
          const centerX = rect.width / 2;
          const centerY = rect.height / 2;
          camera.zoomToPoint(1.25, centerX, centerY);
          syncFromCamera();
          scheduler.requestRender();
        }
        return;
      }
      */

      // Zoom Shortcuts: Ctrl + Minus
      // NOTE: Handled by useEditorCommands.ts with animation
      /*
      if (
        ctrl &&
        (key === "-" || e.code === "Minus" || e.code === "NumpadSubtract")
      ) {
        e.preventDefault();
        e.stopPropagation();
        options.stopMomentum();

        const canvasContainerRef = options.getCanvasContainerRef();
        if (canvasContainerRef) {
          const rect = canvasContainerRef.getBoundingClientRect();
          const centerX = rect.width / 2;
          const centerY = rect.height / 2;
          camera.zoomToPoint(0.8, centerX, centerY);
          syncFromCamera();
          scheduler.requestRender();
        }
        return;
      }
      */

      // Fit Screen Shortcuts: Ctrl + 0
      if (
        ctrl &&
        (key === "0" || e.code === "Digit0" || e.code === "Numpad0")
      ) {
        e.preventDefault();
        e.stopPropagation();
        options.stopMomentum();
        options.fitToScreenAndRender(false);
        return;
      }
    };

    const handleKeyUp = (e: KeyboardEvent) => {
      if (e.code === "Space") {
        options.setIsSpacePressed(false);
      }
      if (e.key === "Alt") {
        options.setIsAltPressed(false);
      }
    };

    const handleWindowBlur = () => {
      options.setIsSpacePressed(false);
      options.setIsPanning(false);
      options.setIsAltPressed(false);
    };

    // Capture phase: fires BEFORE bubble-phase handlers (useEditorCommands,
    // which also listens on window). This ensures canvas gets first dibs on
    // Ctrl+Z/Y for transform mini-undo. When canvas falls through (no mini
    // undo), it doesn't stopPropagation → editor commands handles it in bubble.
    window.addEventListener("keydown", handleKeyDown, { capture: true });
    window.addEventListener("keyup", handleKeyUp);
    window.addEventListener("blur", handleWindowBlur);

    onCleanup(() => {
      window.removeEventListener("keydown", handleKeyDown, { capture: true });
      window.removeEventListener("keyup", handleKeyUp);
      window.removeEventListener("blur", handleWindowBlur);
    });
  });
}
