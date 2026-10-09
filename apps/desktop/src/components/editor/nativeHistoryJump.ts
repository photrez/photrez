// Native history jump (History panel): N native cursor steps, each step's
// {delta, pixelPatches?, externalToken?} projected onto the engine, then the
// host stack re-synced from protocol_history_query_native. Replaces the old
// model-restore loop, which moved the TS cursor/pixels but left the native
// cursor on the entry the user had clicked away from.
//
// Each step goes through runFacadeExternalHandoff - the same CommandResult
// consumer the single-press undo/redo path uses - so pixel patches, the
// external-token replay for pre-cutover entries, empty-delta metadata steps,
// and the E_EXTERNAL_PENDING barrier behave identically for a jump as for a
// single press.
import { getHistoryQuery } from "@/lib/protocol/bridge_emu";
import { deriveOpenBaselineFloor } from "@/engine/nativeHistoryView";
import { runFacadeExternalHandoff, handoffMovedCursor } from "./facadeHistoryHandoff";
import type { EditorContextValue } from "./shell/EditorContext";

/** The slice of the editor context the jump needs (kept narrow so the panel
 *  provider can call it before the full composite exists). */
export interface NativeJumpHost {
  workspace: EditorContextValue["workspace"];
  renderer: EditorContextValue["renderer"];
  scheduler: EditorContextValue["scheduler"];
}

/** Outcome of a multi-step jump, so a blocked or truncated jump is never
 *  silent: the caller and the console both see where it stopped. */
export interface NativeJumpResult {
  completedSteps: number;
  blocked: boolean;
  reason?: string;
}

export async function navigateHistoryNative(
  host: NativeJumpHost,
  index: number,
): Promise<NativeJumpResult> {
  const notRun: NativeJumpResult = { completedSteps: 0, blocked: false };
  const engine = host.workspace.getActiveEngine();
  const history = host.workspace.getActiveHistory();
  if (!engine || !history) return notRun;
  let docId = "";
  try {
    docId = host.workspace.getActiveDocumentId?.() ?? "";
  } catch {
    return notRun;
  }
  if (!docId) return notRun;
  let query: { cursor: number; entries: unknown[] };
  try {
    query = await getHistoryQuery(docId);
  } catch {
    return notRun;
  }
  const entries = Array.isArray(query?.entries) ? query.entries : null;
  const cursor = query?.cursor;
  if (!entries || typeof cursor !== "number") return notRun;
  // The panel indexes history from 0=Open. The document-open baseline entry is
  // stream position 0, not a panel step, so panel index N maps to cursor
  // N + floor. The floor comes from the stream content itself; a host-side
  // sticky flag would survive eviction and under-select the oldest user entry.
  const floor = deriveOpenBaselineFloor(
    entries as import("@/lib/protocol/types").HistoryEntryView[],
  );
  if (!Number.isInteger(index) || index < 0 || index > entries.length - floor) return notRun;
  const target = Math.max(floor, Math.min(entries.length, index + floor));
  const diff = target - cursor;
  if (diff === 0) return notRun;

  const steps = Math.abs(diff);
  const direction = diff < 0 ? "undo" : "redo";
  let completedSteps = 0;
  for (let step = 0; step < steps; step++) {
    const handled = await runFacadeExternalHandoff(
      host as EditorContextValue,
      direction,
    );
    // Read the moved flag on EVERY iteration: the boolean return alone cannot
    // distinguish "the walker stepped, the delta was empty" (true after a
    // moved cursor) from "the walker never moved" (false). Without reading it
    // per iteration, a false-after-move iteration lands the jump short and
    // leaks the moved flag into the next press.
    const moved = handoffMovedCursor();
    if (handled) {
      if (moved) {
        completedSteps++;
        continue;
      }
      // Handled but the cursor never moved: the external-entry barrier is
      // wedged (E_EXTERNAL_PENDING). Stop the jump and say so; counting it as
      // progress would report successes while nothing moves.
      const reason = "external entry pending confirmation blocked the step";
      console.warn(`[native-history-jump] blocked at step ${step}: ${reason}`);
      await history.resyncFromNativeQuery();
      host.scheduler.requestRender();
      host.workspace.notifyVisualChange();
      return { completedSteps, blocked: true, reason };
    }
    if (moved) {
      // The walker moved the cursor but reported unhandled (a legacy External
      // entry with an empty delta). Drain the TS twin of that step so the
      // derived stack does not leak it, then carry on.
      history.discardRustOwnedPixelStep?.(direction);
      completedSteps++;
      continue;
    }
    // Neither handled nor moved: the step belongs to the legacy TS store.
    // Pop it through the same model restore the single-press path uses.
    const snapshot =
      direction === "undo"
        ? history.undo(engine.snapshot())
        : history.redo(engine.snapshot());
    if (snapshot) {
      engine.restore(snapshot);
      completedSteps++;
      continue;
    }
    const reason = `no native entry and no TS twin for step ${step}`;
    console.warn(`[native-history-jump] stopped: ${reason}`);
    await history.resyncFromNativeQuery();
    host.scheduler.requestRender();
    host.workspace.notifyVisualChange();
    return { completedSteps, blocked: true, reason };
  }

  await history.resyncFromNativeQuery();
  host.scheduler.requestRender();
  host.workspace.notifyVisualChange();
  return { completedSteps, blocked: false };
}

