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
