// Facade (Rust-owned) history handoff for a single undo/redo step.
//
// Extracted from useEditorCommands so the barrier-clear path is unit-testable
// without mounting the whole hook. Every path reports the step handled
// (`true`): the native walker is the sole executor for a facade-owned
// document, and the legacy TS pop it used to fall through to is retired.
//
// "handled" DOES NOT MEAN THE CURSOR MOVED. The external-handoff arm
// commits the cursor through `confirmExternalCursor` and can then report an
// empty delta - a legacy `External` entry with no captured after-state
// restores no layer changes. A caller-side cursor step in that case becomes
// a SECOND undo for one press, so the caller must ask `handoffMovedCursor()`
// and tell the history. Every path here has already consumed the press and
// never pops, so it needs no note.
//
// See ADR 0008 H0. When the active document owns no facade layers this branch
// is never entered - the dispatch gate is per-document - so production
// byte-identical behavior is preserved. The history_cursor_commit predicate it
// drives validates the walker-recorded barrier (seq, direction) only - never
// index arithmetic - which is what makes it correct on redo-truncated
// (non-dense) streams where entries[i].seq != i+1.

/**
 * Did the most recent `runFacadeExternalHandoff` move the Rust cursor?
 * One-shot per call: the native jump loop reads it immediately after the call
 * to decide whether the step made progress.
 *
 * Module state rather than a return value because the boolean return is
 * constant (every press is handled) and cannot carry the moved/not-moved fact.
 *
 * NOT reading it is safe, and that is why this works: `runFacadeExternalHandoff`
 * clears the flag as its first statement, so a flag left set by a handled
 * press is erased before the next press can reach the walker. Only a press
 * whose own walker ran can read true.
 */
let cursorMovedByHandoff = false;

/** Read and reset the one-shot flag. See the module header. */
export function handoffMovedCursor(): boolean {
  const moved = cursorMovedByHandoff;
  cursorMovedByHandoff = false;
  return moved;
}

import { getFacade, confirmExternalCursor, syncFacadeVersionFromPixel, getExternalRecordSnapshot, parkExternalReplaySnapshot } from "@/lib/protocol/facadeRegistry";
import { deriveOpenBaselineFloor } from "@/engine/nativeHistoryView";
import { applyRustTilesToSurface } from "@/lib/rustShadow";
import type { EditorContextValue } from "./shell/EditorContext";

/**
 * Project the tiles Rust returned for a pixel undo/redo step onto
 * the host caches.
 *
 * Rust is the executor: `Command::Undo`/`Command::Redo` already
 * moved the cursor and wrote the canonical buffer, so the host only
 * mirrors the bytes into its derived caches (paint surface + GPU
 * textures). Returns true when the step is fully handled - the
 * caller must NOT pop the TS history store, which would spend a
 * second entry for a step already taken.
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
    // the facade cursor (native DocumentCore or wasm). The pixel-store depth
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
  // Read it from the stream itself: the host-recorded flag is sticky and would
  // over-count after eviction removed the entry (or survive a baseline-less
  // stream). A baseline that is not present reads as 0 and never refuses a
  // legitimate undo.
  const floor = deriveOpenBaselineFloor(entries as import("@/lib/protocol/types").HistoryEntryView[]);
  return cursor > floor;
}

export async function runFacadeExternalHandoff(
  editor: EditorContextValue,
  direction: "undo" | "redo",
): Promise<boolean> {
  // THE INVARIANT: cleared here, at ENTRY, therefore only this press's walker can
  // leave it true.
  //
  // `cursorMovedByHandoff` has exactly one writer, and it runs in the middle of this
  // function. A reader can therefore only be misled if it runs BEFORE this clear. The
  // reader is `handoffMovedCursor()`, called by `useEditorCommands` AFTER this function
  // returns - but only on the fall-through path: a handled press returns early to the
  // dispatcher, which returns without reading. So a handled press leaves the flag set,
  // and the next press's read would take it as its own. Clearing at entry removes the
  // possibility by construction: whatever the previous press did, this press starts at
  // false, and only this press's own walker can raise it.
  //
  // DO NOT MOVE THIS DOWN, past either early return. Both the `!engine` return and the
  // `rustStreamHoldsUserWork` refusal exit without setting the flag, and on a press
  // following a handled one they would hand the stale `true` to the dispatcher, which
  // would then suppress that press's cursor step - one host entry permanently unstepped.
  cursorMovedByHandoff = false;
  const engine = editor.workspace.getActiveEngine();
  if (!engine) {
    // No engine: no executor exists for this press. The legacy TS pop this
    // branch used to fall through to is retired, so the truthful answer is
    // a handled no-op with a refreshed view.
    await editor.workspace.getActiveHistory?.()?.resyncFromNativeQuery?.();
    return true;
  }
  // Refuse BEFORE driving the cursor. Once facade.undo() runs, Rust has moved
  // and there is no undoing that from the host side without a second step.
  //
  // A refusal is HANDLED, not a fall-through. `cursor > floor` (the
  // document-open baseline floor) is the only host-side statement of "this
  // document holds no user work", so its answer must never send the press to an
  // executor that owns nothing: the host stack cannot restore a snapshot whose
  // layers are facade-owned (E_FACADE_OWNED), so falling through spent a legacy
  // entry and then raised "Undo failed: E_FACADE_OWNED" on the DEFAULT shape of
  // every opened project - facade-owned, native stream at 0/0, because seeding
  // records no history entry. The native executor answers a cursor-0 press the
  // same way (Command::Undo returns an empty delta and a version bump, Ok), so
  // refusing is the truthful answer, not a lost press. The view is refreshed
  // here so the refusal is self-sufficient for every caller; the single-press
  // dispatcher resyncs again after any handled press.
  if (!(await rustStreamHoldsUserWork(editor, direction))) {
    console.info(
      "[facade-history] nothing above the document-open baseline; not driving the cursor",
      direction,
    );
    await editor.workspace.getActiveHistory?.()?.resyncFromNativeQuery?.();
    return true;
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
  // The Rust command is the sole executor, and ONLY a rejection means the
  // cursor genuinely did not move. Once it resolves, Rust HAS moved for this
  // step: every branch below must report handled, so a projection failure can
  // never send the caller on to pop a TS entry AND fire rust_pixels_undo - two
  // extra undo steps for one press.
  let facade: ReturnType<typeof getFacade>;
  let snap: unknown;
  try {
    facade = getFacade(engine.getId());
    snap = await (direction === "undo" ? facade.undo() : facade.redo());
    // `facade.undo()` is the native walker. It moved the cursor for a Pixel or
    // External tip, and NOT for a Snapshot tip or at a boundary. Recorded either
    // way - a false positive is harmless, because every path that returns true here
    // never pops, and only the false-returning paths consult the flag.
    cursorMovedByHandoff = true;
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
    // Rejection: the cursor did not move. A legacy `External` entry can only
    // surface through the facade walker when its cursor step belongs to the
    // facade; the legacy TS store the press would fall through to is a dead
    // executor here, so report handled and refresh the view instead.
    await editor.workspace.getActiveHistory?.()?.resyncFromNativeQuery?.();
    return true;
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
    // Empty delta: the walker moved the cursor and a legacy `External` entry
    // restored no layer changes. The host pop the legacy path would have
    // issued is retired, so the truthful answer is a handled no-op that leaves
    // the cursor where the walker moved it.
    await editor.workspace.getActiveHistory?.()?.resyncFromNativeQuery?.();
    return true;
  } catch (err) {
    // Rust already moved its cursor for this step, so "handled" is the only
    // safe answer: returning false here would make the caller pop a TS entry
    // and fire rust_pixels_undo for a step already taken.
    console.warn("[facade-history] projection failed after the Rust cursor moved:", err);
    return true;
  }
}
