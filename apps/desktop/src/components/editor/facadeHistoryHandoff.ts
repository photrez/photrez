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

import { getFacade, confirmExternalCursor } from "@/lib/protocol/facadeRegistry";
import type { EditorContextValue } from "./shell/EditorContext";

export function runFacadeExternalHandoff(
  editor: EditorContextValue,
  direction: "undo" | "redo",
): boolean {
  const engine = editor.workspace.getActiveEngine();
  if (!engine) return false;
  try {
    const facade = getFacade(engine.getId());
    const snap = direction === "undo" ? facade.undo() : facade.redo();
    // External history handoff: the walker landed on a legacy (external) entry
    // and set the engine's pending-external barrier (the wedge). The ONLY
    // guaranteed effect here is clearing that barrier; we must not claim the
    // undo restored the model. When facade-owned layers exist, the legacy TS
    // fall-through pops the TS entry and engine.restore() throws E_FACADE_OWNED
    // (mixed-history constraint) - a known pre-existing limitation tracked
    // separately.
    if (facade.lastExternalHandoff) {
      const committed = confirmExternalCursor(
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
      engine.applyFacadeSnapshot(snap as never);
      editor.scheduler.requestRender();
      editor.workspace.notifyVisualChange();
      return true;
    }
    return false; // fall through to legacy TS history
  } catch {
    // Rust command rejected - fall through to legacy TS history.
    return false;
  }
}
