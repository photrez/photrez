import { createSignal } from "solid-js";

/**
 * Manual save status — module-level signal so CanvasViewport, BottomStatusBar,
 * and useEditorCommands share the same reactive state.
 */
export const [saveInProgress, setSaveInProgress] = createSignal(false);
export const [saveProgressText, setSaveProgressText] = createSignal("");
export const [saveActive, setSaveActive] = createSignal(false);

let dismissTimer: ReturnType<typeof setTimeout> | null = null;

/** Cancel any pending dismiss timer to prevent stale timer racing (save→save within 2s). */
export function cancelPendingSaveDismiss(): void {
  if (dismissTimer) {
    clearTimeout(dismissTimer);
    dismissTimer = null;
  }
}

/** Schedule auto-dismiss of the save overlay after a delay. */
export function scheduleSaveDismiss(delay = 2000): void {
  cancelPendingSaveDismiss();
  dismissTimer = setTimeout(() => {
    setSaveInProgress(false);
    setSaveProgressText("");
    dismissTimer = null;
  }, delay);
}
