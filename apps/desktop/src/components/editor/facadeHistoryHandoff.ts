// Facade (Rust-owned) history handoff for a single undo/redo step.
//
// Extracted from useEditorCommands so the barrier-clear path is unit-testable
// without mounting the whole hook. Returns true when this branch fully handled
// the step (caller must return); false when the caller should fall through to
// the legacy TS history store.
//
// See ADR 0008 H0. When photrez.facade is OFF this branch is never entered
// (hasFacadeOwnedLayers() is false), so production byte-identical behavior is
// preserved. The history_cursor_commit predicate it drives validates the
// walker-recorded barrier (seq, direction) only - never index arithmetic -
// which is what makes it correct on redo-truncated (non-dense) streams where
// entries[i].seq != i+1.

import { getFacade, confirmExternalCursor, syncFacadeVersionFromPixel } from "@/lib/protocol/facadeRegistry";
import { applyRustTilesToSurface } from "@/lib/rustShadow";
import type { EditorContextValue } from "./shell/EditorContext";

/**
 * Project the tiles Rust returned for a pixel undo/redo step, and drain the TS
 * twin of that step.
 *
 * Rust is the executor: `Command::Undo`/`Command::Redo` already moved the
 * cursor and wrote the canonical buffer, so the host only mirrors the bytes
 * into its derived caches (paint surface + GPU textures). Returns true when
 * the step is fully handled - the caller must NOT fall through to the TS
 * history store, which would pop a second entry for a step already taken.
 *
 * THIS IS THE SINGLE SITE a `rustOwned` pixel step is consumed through when
 * the facade executor runs, for undo and for redo alike: it is the only
 * caller of the pixel-patches branch, and `direction` selects which stack the
 * twin moves off. The tile path in useEditorCommands reaches the same
 * conclusion by popping through `undo()`/`redo()` itself.
 */
async function projectRustPixelHandoff(
  editor: EditorContextValue,
  engine: NonNullable<ReturnType<EditorContextValue["workspace"]["getActiveEngine"]>>,
  handoff: NonNullable<ReturnType<typeof getFacade>["lastPixelPatches"]>,
  direction: "undo" | "redo",
): Promise<boolean> {
  const toSurface = handoff.tiles.map((t) => ({
    x: t.x, y: t.y, w: t.w, h: t.h, data: new Uint8ClampedArray(t.data),
  }));
  const surf = engine.getPaintSurface(handoff.layerId) as
    | {
        context: { putImageData(img: { width: number; height: number; data: Uint8ClampedArray }, x: number, y: number): void };
        pixelEpoch: number;
        pixelVersion?: number;
      }
    | null
    | undefined;
  if (surf) {
    applyRustTilesToSurface(surf.context, toSurface);
    // Stamp the exact canonical state these tiles reflect so a later derived
    // cache read knows it is current instead of re-fetching.
    surf.pixelEpoch = handoff.epoch;
    surf.pixelVersion = handoff.version;
    syncFacadeVersionFromPixel(engine.getId() ?? "default", handoff.version);
  }
  if (handoff.tiles.length > 0) {
    // Same call shape the Rust paint/fill commits use: the layer's own dims
    // are the paint-surface dims, and the GPU upload shape is width/height.
    const layer = engine.getLayer(handoff.layerId);
    editor.renderer.uploadSurfaceTiles?.(
      handoff.layerId,
      layer?.width ?? 0,
      layer?.height ?? 0,
      toSurface.map((t) => ({ x: t.x, y: t.y, width: t.w, height: t.h, data: t.data })),
    );
    // The layer's model bitmap still holds the pre-step pixels (engine.restore
    // is intentionally skipped for a pixel step), so ask the engine to re-read
    // the canonical buffer. Same repair the flag-ON rust_pixels path uses.
    await engine.ensureBitmapCurrent(editor.workspace.getActiveDocumentId() ?? "", handoff.layerId);
  }
  // Drain the TS twin now that the canonical pixels are projected. Without it
  // the TS depth freezes while the Rust cursor advances, so `canUndo()` keeps
  // reporting work that no longer exists and a press past the end is dispatched
  // (70ea270, real-app artifact case 2). Refused for a step Rust took that has
  // no twin here: that is a Pixel entry from a producer that pushes none, and
  // draining some other entry would silently drop real TS history.
  const history = editor.workspace.getActiveHistory?.();
  const drained = history?.discardRustOwnedPixelStep?.(direction) ?? false;
  if (!drained) {
    console.warn(
      "[facade-history] Rust took a pixel step with no TS twin to drain",
      direction,
      handoff.layerId,
    );
  }
  editor.scheduler.requestRender();
  editor.workspace.notifyVisualChange();
  return true;
}

export async function runFacadeExternalHandoff(
  editor: EditorContextValue,
  direction: "undo" | "redo",
): Promise<boolean> {
  const engine = editor.workspace.getActiveEngine();
  if (!engine) return false;
  // Mirror useEditorCommands restore sweep: after projecting the facade snapshot
  // onto the engine, re-upload any layer that now carries a retained bitmap so
  // the renderer's texture cache matches the engine's (dropped-node reuse keeps
  // pixels alive across routed delete -> undo). Without this the GPU-side handle
  // stays stale and the restored layer renders blank.
  const captureBitmaps = (): Map<string, ImageBitmap | null> => {
    // Engine is host-provided; tolerate a partial engine (e.g. test doubles that
    // omit getLayers). Production DocumentEngine always exposes it.
    const layers = typeof engine.getLayers === "function" ? engine.getLayers() : [];
    return new Map(layers.map((l) => [l.id, l.imageBitmap ?? null]));
  };
  const reuploadAttachedImages = (before: Map<string, ImageBitmap | null>): void => {
    const layers = typeof engine.getLayers === "function" ? engine.getLayers() : [];
    for (const layer of layers) {
      if (!layer.imageBitmap) continue;
      // Upload only layers the projection added or swapped. Untouched layers
      // keep the same bitmap object (the projection never swaps pixels), so
      // their texture already matches and a full re-upload only burns time.
      if (before.get(layer.id) !== layer.imageBitmap) editor.renderer.uploadImage(layer.id, layer.imageBitmap);
    }
  };
  // The Rust command is the only fall-through trigger. If IT fails, the cursor
  // did not move and the legacy TS store still owns the step. Once it resolves,
  // Rust HAS moved for this step: every branch below must report handled, so a
  // projection failure can never send the caller on to pop a TS entry AND fire
  // rust_pixels_undo - two extra undo steps for one press.
  let facade: ReturnType<typeof getFacade>;
  let snap: unknown;
  try {
    facade = getFacade(engine.getId());
    snap = await (direction === "undo" ? facade.undo() : facade.redo());
  } catch {
    // Rust command rejected - fall through to legacy TS history.
    return false;
  }
  try {
    // Rust pixel handoff: the walker stepped a Rust PIXEL entry, so the cursor
    // has ALREADY moved and the tiles came back with the result. This branch
    // runs before the empty-delta check on purpose: a pixel step produces an
    // empty RenderDelta (it changes no layer metadata), so an empty delta is no
    // longer evidence that "Rust had nothing". Returning true here is what
    // keeps the step single - falling through would pop a second entry.
    if (facade.lastPixelPatches) {
      return await projectRustPixelHandoff(editor, engine, facade.lastPixelPatches, direction);
    }
    // External history handoff: the walker landed on a legacy (external) entry
    // and set the engine's pending-external barrier (the wedge). The ONLY
    // guaranteed effect here is clearing that barrier; we must not claim the
    // undo restored the model. When facade-owned layers exist, the legacy TS
    // fall-through pops the TS entry and engine.restore() throws E_FACADE_OWNED
    // (mixed-history constraint) - a known pre-existing limitation tracked
    // separately.
    if (facade.lastExternalHandoff) {
      // Heal source: this branch does NOT re-push. The native-authority heal
      // re-push runs AFTER the legacy TS restore in useEditorCommands
      // (restoreHistorySnapshot, handoff-fallthrough branch) so it carries the
      // post-restore engine state. Here we only project the facade-restored
      // snapshot onto the engine so it is not left stale before the barrier is
      // cleared (confirmExternalCursor no longer takes an engine).
      const before = captureBitmaps();
      engine.applyFacadeSnapshot(snap as never, {
        dimsAuthoritative: facade.lastProjectionDimsAuthoritative,
      });
      reuploadAttachedImages(before);
      const committed = await confirmExternalCursor(
        engine.getId(),
        facade.lastExternalHandoff.seq,
        facade.lastExternalHandoff.direction,
      );
      if (!committed.ok) {
        // confirmExternalCursor set historyDegraded (fail-fast). There is no UI
        // consumer of historyDegraded in this change (surfacing it is a separate
        // follow-up), so the user gets no visible signal yet - we stop here to
        // avoid a possible engine-cursor divergence, but we do NOT claim
        // non-silent behavior.
        return true;
      }
    }
    if (!facade.lastHistoryDeltaWasEmpty) {
      const before = captureBitmaps();
      engine.applyFacadeSnapshot(snap as never, {
        dimsAuthoritative: facade.lastProjectionDimsAuthoritative,
      });
      reuploadAttachedImages(before);
      editor.scheduler.requestRender();
      editor.workspace.notifyVisualChange();
      return true;
    }
    return false; // fall through to legacy TS history
  } catch (err) {
    // Rust already moved its cursor for this step, so "handled" is the only
    // safe answer: returning false here would make the caller pop a TS entry
    // and fire rust_pixels_undo for a step already taken.
    console.warn("[facade-history] projection failed after the Rust cursor moved:", err);
    return true;
  }
}
