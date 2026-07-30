import { useEditor } from "../shell/EditorContext";
import { flattenAllLayers, mergeActiveLayerDown, stampVisibleLayers } from "./layerOperations";
import { cancelLayerTransformSession } from "../transformSession";
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
  } = useEditor();

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
      if (flattenAllLayers(engine, history, renderer)) {
        scheduler.requestRender();
      } else {
        showToast("Could not flatten layers", "warn");
      }
    }
  };

  const handleApplyAdjustment = () => {
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
      const result = engine.commitBasicAdjustment(activeId, renderer);
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
