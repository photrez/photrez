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

import { getFacade, confirmExternalCursor, syncFacadeVersionFromPixel, getExternalRecordSnapshot, parkExternalReplaySnapshot } from "@/lib/protocol/facadeRegistry";
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

/**
 * Is the Rust stream's next step one the user actually performed?
 *
 * The stream does not start at zero. Every document opens at cursor 1, because
 * the factory commits its own SetBackgroundFlag entry
 * (`createBlankDocument` -> `commitFacadeBackgroundFlag`) before the user
 * touches anything. That entry is the FLOOR of history, not a user edit, and
 * Rust reports a real handled step for undoing it - so a press that has run out
 * of user work will happily step it, walk the cursor in front of history, and
 * bank a redo slot the user can never spend. Measured at 9403949: 5 strokes,
 * 5 undos (all six assertions correct), then a 6th press moved the cursor
 * 1 -> 0 and left the 5-redo round trip one step short of the 5-stroke state.
 *
 * So the question is only ever "does the stream hold anything ABOVE that
 * baseline", and the baseline is the one entry the host itself created at open
 * time. Anything the user did - a pixel stroke, a routed canvas op, a
 * transform, an External step - adds an entry above it.
 *
 * This is deliberately NOT a TS-gate query. Routed ops write no TS history
 * entry by design (paramsRouting, canvasRouting, structuralRouting all say so),
 * so `canUndo()` is false on a document whose only work was a routed canvas
 * resize; treating that as "nothing to undo" would swallow a legitimate undo
 * (see `routedCanvasUndo.wiring.test.ts`, which drives exactly that with an
 * empty TS stack). The TS store and the Rust stream count different things, so
 * the TS gate cannot answer for the stream. The stream can, and it does.
 */
// Exported so the refusal can be tested against the shipped predicate rather than
// a reimplementation of it. A test-local copy of `cursor > floor` would keep
// passing if this function changed - which is exactly the regression it guards:
// a frozen cursor cannot surface as a dead undo button (the button is enabled
// unconditionally under facade ownership), it silently undoes the wrong entry.
export async function rustStreamHoldsUserWork(
  editor: EditorContextValue,
  direction: "undo" | "redo",
): Promise<boolean> {
  // Every failure mode here resolves to "assume there is work", never to
  // "refuse": a read that cannot be completed is not evidence of an empty
  // stream, and blocking an undo on missing evidence would lose the user's
  // work. That includes a partial editor double without the accessor, and an
  // empty/unknown doc id.
  let docId = "";
  try {
    docId = editor.workspace.getActiveDocumentId?.() ?? "";
  } catch {
    return true;
  }
  if (!docId) return true;
  let query: { cursor: number; entries: unknown[] };
  try {
    // getHistoryQuery is authority-aware: it answers for whichever store backs
    // the facade cursor (native ProtocolEngine or wasm). The pixel-store depth
    // probe would be the WRONG source - that is a different history under wasm
    // authority, and gating on it refuses legitimate facade-only undos there.
    const { getHistoryQuery } = await import("@/lib/protocol/bridge_emu");
    query = await getHistoryQuery(docId);
  } catch {
    return true;
  }
  const entries = Array.isArray(query?.entries) ? query.entries : null;
  const cursor = query?.cursor;
  if (!entries || typeof cursor !== "number") return true;
  if (direction === "redo") {
    // Entries AHEAD of the cursor. The baseline is always behind it, so any
    // entry at all is user work.
    return entries.length > cursor;
  }
  // Entries BELOW the cursor. How many of those are the document-open baseline?
  // Exactly one, and only if this document actually got one - which the host
  // knows because the factory's commit recorded it. Assuming "always 1" would
  // refuse a legitimate undo on any document that never had a baseline (a
  // document opened without the route armed, or with authority off).
  let floor = 0;
  try {
    const { hasOpenBaselineEntry } = await import("@/lib/protocol/backgroundFlagRouting");
    if (hasOpenBaselineEntry(docId)) floor = 1;
  } catch {
    floor = 0; // cannot prove a baseline exists -> assume none -> never refuse wrongly
  }
  return cursor > floor;
}

export async function runFacadeExternalHandoff(
  editor: EditorContextValue,
  direction: "undo" | "redo",
): Promise<boolean> {
  const engine = editor.workspace.getActiveEngine();
  if (!engine) return false;
  // Refuse BEFORE driving the cursor. Once facade.undo() runs, Rust has moved
  // and there is no undoing that from the host side without a second step.
  if (!(await rustStreamHoldsUserWork(editor, direction))) {
    console.info(
      "[facade-history] nothing above the document-open baseline; not driving the cursor",
      direction,
    );
    return false;
  }
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
  // The Rust command is the only fall-through trigger, and ONLY when the cursor
  // genuinely did not move - i.e. Rust had no entry for this step and the legacy TS
  // store really does own it. Once it resolves, Rust HAS moved for this step: every
  // branch below must report handled, so a projection failure can never send the
  // caller on to pop a TS entry AND fire rust_pixels_undo - two extra undo steps for
  // one press.
  let facade: ReturnType<typeof getFacade>;
  let snap: unknown;
  try {
    facade = getFacade(engine.getId());
    snap = await (direction === "undo" ? facade.undo() : facade.redo());
  } catch (err) {
    // E_EXTERNAL_PENDING is NOT "Rust had nothing". It means a PREVIOUS external
    // step already moved the cursor and its `confirmExternalCursor` failed, which is
    // fail-fast and does NOT clear the barrier. The legacy store does not own this
    // step: it holds entries recorded before the layers became facade-owned. Falling
    // through pops one of those and `engine.restore` throws E_FACADE_OWNED - the
    // shipped "Undo failed: E_FACADE_OWNED: legacy restore blocked while facade owns
    // layers" message, arriving one press after the silent cursor-commit failure that
    // caused it. Refuse this press instead: the step belongs to the facade and is only
    // BLOCKED, so "handled" is the truthful answer, no unrelated legacy entry is spent,
    // and the user keeps every step they still own.
    if (/E_EXTERNAL_PENDING/.test(String((err as Error)?.message ?? err))) {
      return true;
    }
    // Any other rejection: the cursor did not move and the legacy TS store still owns
    // the step. Fall through to legacy TS history.
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
      // Raster half of a host-crop step, BOTH directions. The projection above
      // restored the document size (via the External size pair) but deliberately
      // never wrote a raster or a layer's pixel dims - those are model-owned and
      // no facade command can write them - so the layers are still carrying
      // post-crop pixels at post-crop dims. Rust hands back the entry's token on
      // both arms; the host resolves it to the rasters for THIS direction.
      //
      // An undo resolves the pre-op half and, before overwriting it, parks the
      // live model as the entry's post-op half - the same reason the record could
      // not park it (the record runs before the mutation). A redo resolves that
      // parked half. Replaying the pre-op half instead writes pre-crop rasters
      // into a cropped document: a size that reads correct over layers and a
      // pixel store that are all wrong.
      //
      // Runs AFTER applyFacadeSnapshot so the layer set is final: it only writes
      // pixels onto layers that already exist and can never add one. Runs BEFORE
      // reuploadAttachedImages so the re-upload sees the restored bitmaps -
      // comparing against `before` is what makes it upload them at all.
      const raster = getExternalRecordSnapshot(facade.lastExternalToken, direction);
      if (direction === "undo" && raster) {
        parkExternalReplaySnapshot(facade.lastExternalToken, engine.snapshot());
      }
      if (raster) {
        engine.applyExternalRasterRestore(raster);
      }
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
