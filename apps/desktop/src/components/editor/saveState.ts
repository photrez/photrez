import { createSignal } from "solid-js";

/**
 * Save progress signal for manual saves (Ctrl+S / File → Save / Save As).
 * Replaces the old saveInProgress / saveActive / saveProgressText triple.
 *
 * Phase field transitions:
 *   idle → encoding → writing → done → idle (auto-dismiss)
 *   idle → encoding → writing → error → idle
 *   idle → encoding → cancelled → idle
 */
export type SavePhase = "idle" | "encoding" | "writing" | "done" | "error" | "cancelled";

export interface SaveProgress {
  phase: SavePhase;
  /** Human-readable label shown in the status bar (e.g. "Saving project…") */
  label: string;
  /** 0..1 progress fraction (0 indeterminate, 1 complete) */
  fraction: number;
  /** Optional cancel callback — available during encoding/writing */
  cancel?: () => void;
}

export const [saveProgress, setSaveProgress] = createSignal<SaveProgress>({
  phase: "idle",
  label: "",
  fraction: 0,
});

let dismissTimer: ReturnType<typeof setTimeout> | null = null;

/** Cancel any pending dismiss timer to prevent stale timer racing. */
export function cancelPendingSaveDismiss(): void {
  if (dismissTimer) {
    clearTimeout(dismissTimer);
    dismissTimer = null;
  }
}

/** Schedule auto-dismiss of save progress (done/error/cancelled → idle) after a delay. */
export function scheduleSaveDismiss(delay = 2000): void {
  cancelPendingSaveDismiss();
  dismissTimer = setTimeout(() => {
    setSaveProgress((prev) =>
      prev.phase === "done" || prev.phase === "error" || prev.phase === "cancelled"
        ? { phase: "idle" as const, label: "", fraction: 0 }
        : prev,
    );
    dismissTimer = null;
  }, delay);
}

// ── Save queue — prevents concurrent save race while keeping non-blocking UI ──
// Lives here (not in useEditorCommands) so EditorContext's autosave timer can
// schedule through the same queue without a module cycle.
let _saveRunning = false;
let _pendingSave: (() => void) | null = null;

async function _runSaveQueue(): Promise<void> {
  while (_pendingSave) {
    const fn = _pendingSave;
    _pendingSave = null;
    try {
      await fn();
    } catch {
      // Error already handled inside fn() via catch block
    }
  }
  _saveRunning = false;
}

/**
 * Queue a save operation. Only one save runs at a time (SaveWorkerPool
 * rejects concurrent encodes). If another save is in flight, the new request
 * replaces the pending slot and runs after the current save completes.
 * This handles rapid Ctrl+S presses without blocking the keyboard handler.
 *
 * `lowPriority` (autosave) jobs are skipped entirely when any save is running
 * or queued — they must never preempt or displace a manual save.
 */
export function scheduleSave(fn: () => void, lowPriority = false): void {
  if (lowPriority && (_pendingSave || _saveRunning)) return;
  _pendingSave = fn;
  if (!_saveRunning) {
    _saveRunning = true;
    _runSaveQueue();
  }
}

/** True while a save (manual or autosave) is running or queued. */
export function isSaveRunning(): boolean {
  return _saveRunning || _pendingSave !== null;
}
