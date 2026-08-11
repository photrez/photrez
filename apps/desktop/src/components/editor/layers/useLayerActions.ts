import { useEditor } from "../shell/EditorContext";
import { flattenAllLayers, mergeActiveLayerDown, stampVisibleLayers } from "./layerOperations";
import { cancelLayerTransformSession } from "../transformSession";
import { cancelTextSession, commitTextSession } from "../canvas/pointerTools/textTool";
import { showToast } from "../Toast";

export function useLayerActions() {
  const {
    workspace,
    renderer,
    layers,
    activeLayerId,
    scheduler,
    layerTransformSession,
    setLayerTransformSession,
    setSelectedLayerId,
    textEditSession,
    setTextEditSession,
  } = useEditor();

  const textSessionEditor = () => ({ workspace, textEditSession, setTextEditSession, scheduler });

  const cancelActiveTransformSession = () => {
    const engine = workspace.getActiveEngine();
    if (cancelLayerTransformSession(layerTransformSession(), engine)) {
      setLayerTransformSession(null);
      scheduler.requestRender();
    }
  };

  const handleDuplicateActiveLayer = () => {
    cancelActiveTransformSession();
    const engine = workspace.getActiveEngine();
    const history = workspace.getActiveHistory();
    const activeId = activeLayerId();
    if (!activeId) {
      showToast("No layer selected", "warn");
      return;
    }
    if (engine && history && activeId) {
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
  };

  const handleMergeActiveLayerDown = () => {
    cancelActiveTransformSession();
    const engine = workspace.getActiveEngine();
    const history = workspace.getActiveHistory();
    const activeId = activeLayerId();
    if (!activeId) {
      showToast("No layer selected", "warn");
      return;
    }
    if (engine && history && activeId) {
      // B6-adjacent: merging consumes the ACTIVE layer AND the one directly
      // below it (mergeDown composites both into a NEW id and removes both).
      // Commit any open text session on EITHER first so the typed text
      // survives in the merged result (unlike delete, the content lives on)
      // and no session dangles over a removed layer (its Escape/cancel would
      // resurrect it via restore).
      const session = textEditSession();
      const activeIndex = engine.getLayers().findIndex((l) => l.id === activeId);
      const mergeTargetId = activeIndex >= 0 ? engine.getLayers()[activeIndex + 1]?.id : undefined;
      if (session && (session.layerId === activeId || session.layerId === mergeTargetId)) {
        commitTextSession(textSessionEditor());
        // An empty TEMP layer is removed by commit's empty-commit cleanup —
        // if it was the ACTIVE layer the merge intent is moot; bail instead of
        // merging the layer that moved up into the active slot. (An empty temp
        // as the merge TARGET just drops out; the merge still proceeds.)
        if (!engine.getLayer(activeId)) {
          scheduler.requestRender();
          return;
        }
      }
      if (mergeActiveLayerDown(engine, history, renderer, activeId)) {
        scheduler.requestRender();
      } else {
        showToast("Could not merge layers", "warn");
      }
    }
  };

  const handleFlattenAllLayers = () => {
    cancelActiveTransformSession();
    const engine = workspace.getActiveEngine();
    const history = workspace.getActiveHistory();
    if (engine && history) {
      // B6-adjacent: flatten removes EVERY layer — commit any open text
      // session first so the typed text survives in the flattened result and
      // no session dangles over layers that no longer exist.
      if (textEditSession()) {
        commitTextSession(textSessionEditor());
      }
      if (flattenAllLayers(engine, history, renderer)) {
        scheduler.requestRender();
      } else {
        showToast("Could not flatten layers", "warn");
      }
    }
  };

  const handleApplyAdjustment = async () => {
    cancelActiveTransformSession();
    const engine = workspace.getActiveEngine();
    const history = workspace.getActiveHistory();
    const activeId = activeLayerId();
    if (!activeId) {
      showToast("No layer selected", "warn");
      return;
    }
    if (engine && history && activeId) {
      // Nothing to bake if the layer has no live adjustment.
      if (!engine.getLayer(activeId)?.basicAdjustment) return;
      history.commit(engine.snapshot(), "Apply Adjustment");
      // GPU-preferred bake (falls back to CPU inside the engine); the result is
      // re-uploaded so the composited layer reflects the now-baked pixels.
      const result = await engine.commitBasicAdjustment(activeId, renderer);
      const bakedLayer = engine.getLayer(activeId);
      if (bakedLayer?.imageBitmap) renderer.uploadImage(activeId, bakedLayer.imageBitmap);
      if (result === "cpu" && typeof renderer?.bakeLayerToBitmap === "function") {
        showToast(
          "Layer adjustment bake fell back to CPU — painting may stutter on large layers.",
          "warn",
        );
      }
      scheduler.requestRender();
    }
  };

  const handleStampVisible = () => {
    cancelActiveTransformSession();
    const engine = workspace.getActiveEngine();
    const history = workspace.getActiveHistory();
    if (engine && history) {
      if (stampVisibleLayers(engine, history, renderer)) {
        scheduler.requestRender();
      } else {
        showToast("Nothing to stamp", "warn");
      }
    }
  };

  const handleSelectLayer = (id: string) => {
    // B9: clicking a DIFFERENT layer while a text session is open commits the
    // session first (click-away pattern). Otherwise the overlay keeps editing
    // the session layer while the panel highlights another, and the option bar
    // binds to the wrong layer (@bug 2026-08-09 B9). Selecting the session's
    // own layer (re-edit flow) is a no-op — the session stays open.
    const session = textEditSession();
    if (session && session.layerId !== id) {
      commitTextSession(textSessionEditor());
    }
    const engine = workspace.getActiveEngine();
    engine?.setActiveLayer(id);
    setSelectedLayerId(id);
  };

  const handleToggleVisibility = (e: MouseEvent, id: string) => {
    e.stopPropagation();
    cancelActiveTransformSession();
    const engine = workspace.getActiveEngine();
    const layer = engine?.getLayer(id);
    if (engine && layer) {
      const history = workspace.getActiveHistory();
      history?.commit(engine.snapshot(), "Toggle Visibility");
      engine.setLayerVisibility(id, !layer.visible);
      scheduler.requestRender();
    }
  };

  const handleToggleLock = (e: MouseEvent, id: string) => {
    e.stopPropagation();
    cancelActiveTransformSession();
    const engine = workspace.getActiveEngine();
    const layer = engine?.getLayer(id);
    if (engine && layer) {
      const history = workspace.getActiveHistory();
      history?.commit(engine.snapshot(), "Toggle Lock");
      engine.setLayerLocked(id, !layer.locked);
      scheduler.requestRender();
    }
  };

  const handleToggleLockTransparency = (e: MouseEvent, id: string) => {
    e.stopPropagation();
    cancelActiveTransformSession();
    const engine = workspace.getActiveEngine();
    const layer = engine?.getLayer(id);
    if (engine && layer) {
      const history = workspace.getActiveHistory();
      history?.commit(engine.snapshot(), "Toggle Lock");
      engine.setLayerLockTransparency(id, !layer.lockTransparency);
      scheduler.requestRender();
    }
  };

  const handleToggleLockPosition = (e: MouseEvent, id: string) => {
    e.stopPropagation();
    cancelActiveTransformSession();
    const engine = workspace.getActiveEngine();
    const layer = engine?.getLayer(id);
    if (engine && layer) {
      const history = workspace.getActiveHistory();
      history?.commit(engine.snapshot(), "Toggle Lock");
      engine.setLayerLockPosition(id, !layer.lockPosition);
      scheduler.requestRender();
    }
  };

  const handleToggleLockRotation = (e: MouseEvent, id: string) => {
    e.stopPropagation();
    cancelActiveTransformSession();
    const engine = workspace.getActiveEngine();
    const layer = engine?.getLayer(id);
    if (engine && layer) {
      const history = workspace.getActiveHistory();
      history?.commit(engine.snapshot(), "Toggle Lock");
      engine.setLayerLockRotation(id, !layer.lockRotation);
      scheduler.requestRender();
    }
  };

  const handleMoveUp = (e: MouseEvent, index: number) => {
    e.stopPropagation();
    cancelActiveTransformSession();
    if (index > 0) {
      const engine = workspace.getActiveEngine();
      const history = workspace.getActiveHistory();
      if (engine && history) {
        history.commit(engine.snapshot(), "Reorder Layer");
        engine.reorderLayer(index, index - 1);
        scheduler.requestRender();
      }
    }
  };

  const handleMoveDown = (e: MouseEvent, index: number) => {
    e.stopPropagation();
    cancelActiveTransformSession();
    if (index < layers().length - 1) {
      const engine = workspace.getActiveEngine();
      const history = workspace.getActiveHistory();
      if (engine && history) {
        history.commit(engine.snapshot(), "Reorder Layer");
        engine.reorderLayer(index, index + 1);
        scheduler.requestRender();
      }
    }
  };

  const handleAddLayer = () => {
    cancelActiveTransformSession();
    const engine = workspace.getActiveEngine();
    const history = workspace.getActiveHistory();
    if (engine && history) {
      history.commit(engine.snapshot(), "New Layer");
      try {
        engine.addLayer(`Layer ${engine.getLayers().length + 1}`);
        scheduler.requestRender();
      } catch (err) {
        showToast(`Cannot add layer: ${(err as Error).message}`, "error");
      }
    }
  };

  const handleDeleteActiveLayer = () => {
    cancelActiveTransformSession();
    const engine = workspace.getActiveEngine();
    const history = workspace.getActiveHistory();
    const activeId = activeLayerId();
    if (!activeId) {
      showToast("No layer selected", "warn");
      return;
    }
    if (engine && history && activeId) {
      // B6: close an open text session on the layer being deleted FIRST. The
      // session's layerId would otherwise dangle (overlay keeps editing a
      // deleted layer), Escape afterwards would resurrect the layer via
      // restore(), and a later tool-switch commit would push a ghost "Edit
      // Text" step whose snapshot still contains the deleted layer. A TEMP
      // (new) text layer is removed by the cancel with no history entry — the
      // delete intent is already fulfilled, so bail out.
      const session = textEditSession();
      if (session && session.layerId === activeId) {
        cancelTextSession(textSessionEditor());
        if (!engine.getLayer(activeId)) {
          scheduler.requestRender();
          return;
        }
      }
      const layer = engine.getLayer(activeId);
      if (layer?.isBackground) {
        showToast("Cannot delete the Background layer", "warn");
        return;
      }
      if (engine.getLayers().length <= 1) return;
      history.commit(engine.snapshot(), "Delete Layer");
      engine.deleteLayer(activeId);
      renderer.destroyTexture(activeId);
      scheduler.requestRender();
    }
  };

  return {
    handleDuplicateActiveLayer,
    handleMergeActiveLayerDown,
    handleFlattenAllLayers,
    handleApplyAdjustment,
    handleStampVisible,
    handleSelectLayer,
    handleToggleVisibility,
    handleToggleLock,
    handleToggleLockTransparency,
    handleToggleLockPosition,
    handleToggleLockRotation,
    handleMoveUp,
    handleMoveDown,
    handleAddLayer,
    handleDeleteActiveLayer,
  };
}
