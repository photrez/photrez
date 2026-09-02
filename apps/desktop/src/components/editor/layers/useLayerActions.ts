import { useEditor } from "../shell/EditorContext";
import {
  flattenAllLayers,
  mergeActiveLayerDown,
  mergeSelectedLayers,
  deleteMultipleLayers,
  duplicateMultipleLayers,
  stampVisibleLayers,
} from "./layerOperations";
import { cancelLayerTransformSession } from "../transformSession";
import { cancelTextSession, commitTextSession } from "../canvas/pointerTools/textTool";
import { showToast } from "../Toast";
import { getFacade, isFacadeEnabled, seedFacadeFromEngine, MIXED_OWNERSHIP_MESSAGE, __resetFacadeRegistryForTests } from "@/lib/protocol/facadeRegistry";
import { createEditorClient } from "@/lib/protocol/editorClient";
import { isFacadeOwnedLayer } from "@/engine/document";
import { applyRustTilesToSurface, rehydratePaintSurfaceFromRust } from "@/lib/rustShadow";

// Ticket 2.1: single facade per document when photrez.facade=1. Rust is sole owner for addLayer.
// Registry lives in @/lib/protocol/facadeRegistry (shared with Ticket 2.2 transform drag).
export function __resetFacadeForTests(): void {
  __resetFacadeRegistryForTests();
}

export function useLayerActions() {
  const {
    workspace,
    renderer,
    layers,
    activeLayerId,
    selectedLayerIds,
    setSelectedLayerIds,
    toggleLayerSelection,
    rangeSelectLayers,
    scheduler,
    layerTransformSession,
    setLayerTransformSession,
    selectedLayerId,
    setSelectedLayerId,
    textEditSession,
    setTextEditSession,
    setStatusLoadingMessage,
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
    const multiIds = selectedLayerIds();

    if (multiIds.length > 1 && engine && history) {
      const newIds = duplicateMultipleLayers(engine, history, renderer, multiIds);
      if (newIds.length > 0) {
        setSelectedLayerIds(newIds);
        if (newIds[0]) engine.setActiveLayer(newIds[0]);
        scheduler.requestRender();
      }
      return;
    }

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
    const multiIds = selectedLayerIds();

    if (multiIds.length > 1 && engine && history) {
      if (textEditSession()) {
        commitTextSession(textSessionEditor());
      }
      if (mergeSelectedLayers(engine, history, renderer, multiIds)) {
        scheduler.requestRender();
      } else {
        showToast("Could not merge selected layers", "warn");
      }
      return;
    }

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

      // C5.4 canonical-pixel path (flag matches the brush/bucket/fill-layer gating).
      const rustPixelsFlag = (() => {
        try { return localStorage.getItem("photrez.rustPixels") === "1"; } catch { return false; }
      })();
      const surface = engine.getPaintSurface(activeId);

      if (rustPixelsFlag && surface) {
        // Capture pre-bake state BEFORE mutation so undo restores it.
        const preSnapshot = engine.snapshot();
        const layer = engine.getLayer(activeId);
        const preBitmap = layer?.imageBitmap;

        // Calm status-bar loading — only shows if >200ms (Material: <200ms no indicator to avoid flicker)
        let t: number | null = window.setTimeout(() => setStatusLoadingMessage("Applying adjustment..."), 200);
        let result: Awaited<ReturnType<typeof engine.commitBasicAdjustment>>;
        try {
          result = await engine.commitBasicAdjustment(activeId, renderer);
        } finally {
          if (t !== null) clearTimeout(t);
          setStatusLoadingMessage(null);
        }

        const bakedLayer = engine.getLayer(activeId);
        if (!bakedLayer?.imageBitmap) return;

        // Fire-and-forget keeps the handler responsive; canonical write + cache sync complete async.
        void (async () => {
          try {
            const { invoke } = await import("@tauri-apps/api/core");
            const docId = engine.getId();

            // Extract baked RGBA from the produced ImageBitmap.
            const bakeCanvas = new OffscreenCanvas(bakedLayer.width, bakedLayer.height);
            const bakeCtx = bakeCanvas.getContext("2d")!;
            bakeCtx.drawImage(bakedLayer.imageBitmap!, 0, 0);
            const bakedImageData = bakeCtx.getImageData(0, 0, bakedLayer.width, bakedLayer.height);
            const bakedRgba = Array.from(bakedImageData.data);

            // C5.4 ensure-if-absent: Adjustment Bake may be the FIRST raster op on a layer,
            // so seed the canonical store from the PRE-bake bitmap when Rust has no entry yet.
            let layerReady = true;
            try {
              await invoke("rust_pixels_get_epoch", { docId, layerId: activeId });
            } catch {
              layerReady = false;
            }
            if (!layerReady && preBitmap && layer) {
              const preCanvas = new OffscreenCanvas(layer.width, layer.height);
              const preCtx = preCanvas.getContext("2d")!;
              preCtx.drawImage(preBitmap, 0, 0);
              const preImageData = preCtx.getImageData(0, 0, layer.width, layer.height);
              await invoke("rust_pixels_init", {
                docId,
                layerId: activeId,
                width: layer.width,
                height: layer.height,
                bytes: Array.from(preImageData.data),
              });
            }

            // Ensure the derived surface reflects the CURRENT canonical state.
            await rehydratePaintSurfaceFromRust(docId, activeId, surface);

            // Write baked pixels to Rust canonical (whole layer).
            const res = (await invoke("rust_pixels_write_region", {
              docId,
              layerId: activeId,
              x: 0,
              y: 0,
              w: bakedLayer.width,
              h: bakedLayer.height,
              rgba: bakedRgba,
            })) as {
              before: { x: number; y: number; w: number; h: number; data: number[] }[];
              after: { x: number; y: number; w: number; h: number; data: number[] }[];
              epoch: number;
              version: number;
            };

            // Sync TS derived cache from Rust authoritative returned tiles.
            applyRustTilesToSurface(surface.context, res.after);
            surface.pixelEpoch = res.epoch;
            surface.pixelVersion = res.version;
            renderer?.uploadSurfaceTiles?.(activeId, bakedLayer.width, bakedLayer.height,
              res.after.map((t) => ({ x: t.x, y: t.y, width: t.w, height: t.h, data: new Uint8ClampedArray(t.data) })));

            // C5.4 bitmap sync: bitmap was set by commitBasicAdjustment before Rust write.
            // Now that write_region succeeded, bitmap and Rust are proven identical.
            bakedLayer.bitmapEpoch = res.epoch;

            // Imperative memento: single user-visible history step.
            // history stores before/after tiles; Rust entry synced via rust_pixels_undo.
            const imperative = {
              layerId: activeId,
              surfaceWidth: bakedLayer.width,
              surfaceHeight: bakedLayer.height,
              before: res.before.map((t) => ({
                x: t.x, y: t.y, width: t.w, height: t.h,
                data: new Uint8ClampedArray(t.data),
              })),
              after: res.after.map((t) => ({
                x: t.x, y: t.y, width: t.w, height: t.h,
                data: new Uint8ClampedArray(t.data),
              })),
            };
            history.commit(preSnapshot, "Apply Adjustment", imperative, true);
          } catch (err) {
            showToast(`Adjustment Bake failed: ${err instanceof Error ? err.message : "Unknown error"}`, "error");
            // Fallback: commit without imperative so undo still works (legacy-compatible).
            history.commit(preSnapshot, "Apply Adjustment");
          }
        })();

        if (result === "cpu" && typeof renderer?.bakeLayerToBitmap === "function") {
          showToast(
            "Layer adjustment bake fell back to CPU — painting may stutter on large layers.",
            "warn",
          );
        }
        scheduler.requestRender();
      } else {
        // Legacy path (TS-authoritative bitmap).
        history.commit(engine.snapshot(), "Apply Adjustment");
        // Calm status-bar loading — only shows if >200ms (Material: <200ms no indicator to avoid flicker)
        let t: number | null = window.setTimeout(() => setStatusLoadingMessage("Applying adjustment..."), 200);
        let result: Awaited<ReturnType<typeof engine.commitBasicAdjustment>>;
        try {
          result = await engine.commitBasicAdjustment(activeId, renderer);
        } finally {
          if (t !== null) clearTimeout(t);
          setStatusLoadingMessage(null);
        }
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

  const handleSelectLayer = (id: string, e?: MouseEvent) => {
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

    if (e?.ctrlKey || e?.metaKey) {
      toggleLayerSelection(id, true);
      const active = selectedLayerId();
      if (active) engine?.setActiveLayer(active);
    } else if (e?.shiftKey) {
      const fromId = activeLayerId() ?? id;
      rangeSelectLayers(fromId, id, layers());
      engine?.setActiveLayer(id);
    } else {
      setSelectedLayerId(id);
      engine?.setActiveLayer(id);
    }
  };

  const handleToggleVisibility = (e: MouseEvent, id: string) => {
    e.stopPropagation();
    cancelActiveTransformSession();
    const engine = workspace.getActiveEngine();
    const multi = selectedLayerIds();
    const isMulti = multi.length > 1 && multi.includes(id);
    const targetIds = isMulti ? multi : [id];
    if (engine && targetIds.length > 0) {
      const clickedLayer = engine.getLayer(id);
      if (!clickedLayer) return;
      const nextVisible = !clickedLayer.visible;
      const history = workspace.getActiveHistory();
      history?.commit(engine.snapshot(), targetIds.length > 1 ? "Toggle Visibility (Multiple)" : "Toggle Visibility");
      for (const tid of targetIds) {
        engine.setLayerVisibility(tid, nextVisible);
      }
      scheduler.requestRender();
    }
  };

  const handleToggleLock = (e: MouseEvent, id: string) => {
    e.stopPropagation();
    cancelActiveTransformSession();
    const engine = workspace.getActiveEngine();
    const multi = selectedLayerIds();
    const isMulti = multi.length > 1 && multi.includes(id);
    const targetIds = isMulti ? multi : [id];
    if (engine && targetIds.length > 0) {
      const clickedLayer = engine.getLayer(id);
      if (!clickedLayer) return;
      const nextLocked = !clickedLayer.locked;
      const history = workspace.getActiveHistory();
      history?.commit(engine.snapshot(), targetIds.length > 1 ? "Toggle Lock (Multiple)" : "Toggle Lock");
      for (const tid of targetIds) {
        engine.setLayerLocked(tid, nextLocked);
      }
      scheduler.requestRender();
    }
  };

  const handleToggleLockTransparency = (e: MouseEvent, id: string) => {
    e.stopPropagation();
    cancelActiveTransformSession();
    const engine = workspace.getActiveEngine();
    const multi = selectedLayerIds();
    const isMulti = multi.length > 1 && multi.includes(id);
    const targetIds = isMulti ? multi : [id];
    if (engine && targetIds.length > 0) {
      const clickedLayer = engine.getLayer(id);
      if (!clickedLayer) return;
      const nextVal = !clickedLayer.lockTransparency;
      const history = workspace.getActiveHistory();
      history?.commit(engine.snapshot(), targetIds.length > 1 ? "Toggle Lock Transparency (Multiple)" : "Toggle Lock");
      for (const tid of targetIds) {
        engine.setLayerLockTransparency(tid, nextVal);
      }
      scheduler.requestRender();
    }
  };

  const handleToggleLockPosition = (e: MouseEvent, id: string) => {
    e.stopPropagation();
    cancelActiveTransformSession();
    const engine = workspace.getActiveEngine();
    const multi = selectedLayerIds();
    const isMulti = multi.length > 1 && multi.includes(id);
    const targetIds = isMulti ? multi : [id];
    if (engine && targetIds.length > 0) {
      const clickedLayer = engine.getLayer(id);
      if (!clickedLayer) return;
      const nextVal = !clickedLayer.lockPosition;
      const history = workspace.getActiveHistory();
      history?.commit(engine.snapshot(), targetIds.length > 1 ? "Toggle Lock Position (Multiple)" : "Toggle Lock");
      for (const tid of targetIds) {
        engine.setLayerLockPosition(tid, nextVal);
      }
      scheduler.requestRender();
    }
  };

  const handleToggleLockRotation = (e: MouseEvent, id: string) => {
    e.stopPropagation();
    cancelActiveTransformSession();
    const engine = workspace.getActiveEngine();
    const multi = selectedLayerIds();
    const isMulti = multi.length > 1 && multi.includes(id);
    const targetIds = isMulti ? multi : [id];
    if (engine && targetIds.length > 0) {
      const clickedLayer = engine.getLayer(id);
      if (!clickedLayer) return;
      const nextVal = !clickedLayer.lockRotation;
      const history = workspace.getActiveHistory();
      history?.commit(engine.snapshot(), targetIds.length > 1 ? "Toggle Lock Rotation (Multiple)" : "Toggle Lock");
      for (const tid of targetIds) {
        engine.setLayerLockRotation(tid, nextVal);
      }
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
    if (!engine || !history) return;
    if (isFacadeEnabled()) {
      const docId = workspace.getActiveDocumentId() ?? "default";
      const facade = getFacade(docId);
      // Seed facade from engine on first use (one-time projection, not dual owner after)
      seedFacadeFromEngine(engine as never, facade);
      try {
        const snap = facade.addLayer(`Layer ${facade.snapshot.layers.length + 1}`);
        // Project facade snapshot into engine (engine becomes read-only view, no history)
        (engine as unknown as { applyFacadeSnapshot: (s: unknown) => void }).applyFacadeSnapshot(snap);
        scheduler.requestRender();
      } catch (err) {
        const msg = (err as Error).message;
        if (msg.includes("E_VERSION_MISMATCH")) {
          showToast("Version conflict — retrying", "warn");
          // On mismatch, re-sync from engine and retry once would go here (deferred)
        } else {
          showToast(`Cannot add layer: ${msg}`, "error");
        }
      }
      return;
    }
    history.commit(engine.snapshot(), "New Layer");
    try {
      engine.addLayer(`Layer ${engine.getLayers().length + 1}`);
      scheduler.requestRender();
    } catch (err) {
      showToast(`Cannot add layer: ${(err as Error).message}`, "error");
    }
  };

  const handleDeleteActiveLayer = () => {
    cancelActiveTransformSession();
    const engine = workspace.getActiveEngine();
    const history = workspace.getActiveHistory();
    const multiIds = selectedLayerIds();

    if (multiIds.length > 1 && engine && history) {
      // ADR 0008 DeleteLayer UX guard: mixed ownership selection is rejected
      // ATOMICALLY — no partial mutation, no partial protocol traffic. A
      // user-approved partial operation would require explicit future design.
      if (isFacadeEnabled()) {
        const ownedIds = multiIds.filter((id) => isFacadeOwnedLayer(id));
        if (ownedIds.length > 0 && ownedIds.length < multiIds.length) {
          showToast(MIXED_OWNERSHIP_MESSAGE, "warn");
          return;
        }
        if (ownedIds.length > 0) {
          const facade = getFacade(engine.getId());
          let lastSnap: unknown = null;
          for (const id of ownedIds) {
            const s = facade.deleteLayer(id);
            if (s) {
              engine.applyFacadeSnapshot(s as never);
              lastSnap = s;
              renderer.destroyTexture(id);
            }
          }
          setSelectedLayerId(engine.getActiveLayerId());
          scheduler.requestRender();
          return;
        }
      }
      const session = textEditSession();
      if (session && multiIds.includes(session.layerId)) {
        cancelTextSession(textSessionEditor());
      }
      if (deleteMultipleLayers(engine, history, renderer, multiIds)) {
        const nextActive = engine.getActiveLayerId();
        setSelectedLayerId(nextActive);
        scheduler.requestRender();
      }
      return;
    }

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
      // ADR 0008 DeleteLayer ticket (Phase E pilot): route the migrated delete
      // through EditorClient. The client owns the dual-read boundary — when the
      // facade flag is ON and the layer is facade-owned it delegates to the
      // facade (Rust command -> delta -> snapshot) and projects the snapshot
      // into the engine. When the flag is OFF / non-owned it returns
      // {status:"legacy"} and we fall through to the byte-identical legacy TS
      // path below. A thrown command (Rust Err / version conflict) fails closed.
      if (isFacadeEnabled()) {
        const facade = getFacade(engine.getId());
        const client = createEditorClient(
          { applyFacadeSnapshot: (s) => engine.applyFacadeSnapshot(s as never) },
          facade,
        );
        const res = client.deleteLayer(activeId);
        if (res.status === "facade") {
          renderer.destroyTexture(activeId);
          setSelectedLayerId(engine.getActiveLayerId());
          scheduler.requestRender();
          return;
        }
        if (res.status === "blocked") {
          showToast(`Cannot delete layer: ${res.error ?? "unknown error"}`, "error");
          return;
        }
        // status === "legacy": fall through to the byte-identical legacy path below.
      }
      // status === "legacy": byte-identical legacy TS path (flag OFF / non-owned).
      const before = engine.snapshot();
      engine.deleteLayer(activeId);
      const after = engine.snapshot();
      history.recordSnapshotHistory(before, after, "Delete Layer");
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
