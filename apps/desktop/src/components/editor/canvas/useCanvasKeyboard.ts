// SPDX-License-Identifier: AGPL-3.0-or-later
import { onMount, onCleanup } from "solid-js";
import { useEditor } from "../shell/EditorContext";
import { registerShortcut } from "../keyboardRegistry";
import type { CanvasKeyboardOptions, KeyboardShortcutContext } from "./keyboardShortcuts/context";
import { handleLayerFillKey } from "./keyboardShortcuts/layerFill";
import { handleTransformSessionKey } from "./keyboardShortcuts/transformSession";
import { handleCropToolKey } from "./keyboardShortcuts/cropTool";
import { handleSelectionToolKey } from "./keyboardShortcuts/selectionTool";
import { handleLayerOpsKey } from "./keyboardShortcuts/layerOps";
import { handleToolNavKey } from "./keyboardShortcuts/toolNav";

export type { CanvasKeyboardOptions } from "./keyboardShortcuts/context";

/**
 * Canvas keyboard shortcuts. The individual shortcut domains live in
 * keyboardShortcuts/ modules; this hook owns listener lifetime (capture-phase
 * keydown, keyup, window blur) and the shared editor/options context.
 *
 * Chain-of-responsibility with useEditorCommands: canvas captures keydown
 * FIRST (capture phase) so it can intercept Ctrl+Z/Y for transform mini-
 * undo/redo. When the mini stack is empty, it falls through by NOT calling
 * stopPropagation → useEditorCommands receives the event in bubble phase.
 */
export function useCanvasKeyboard(options: CanvasKeyboardOptions) {
  const editor = useEditor();
  const ctx: KeyboardShortcutContext = { editor, options };

  onMount(() => {
    // ── Register keyboard shortcuts (conflict detection) ──
    registerShortcut("Ctrl+Z", "useCanvasKeyboard", { intentional: true });     // transform undo → fallthrough
    registerShortcut("Ctrl+Y", "useCanvasKeyboard", { intentional: true });     // transform redo → fallthrough
    registerShortcut("Ctrl+Shift+Z", "useCanvasKeyboard", { intentional: true });
    // Canvas-only shortcuts:
    registerShortcut("Ctrl+Shift+T", "useCanvasKeyboard");
    registerShortcut("Ctrl+Shift+Alt+E", "useCanvasKeyboard");
    registerShortcut("Ctrl+E", "useCanvasKeyboard");
    registerShortcut("Ctrl+J", "useCanvasKeyboard");
    registerShortcut("Ctrl+Shift+N", "useCanvasKeyboard");
    registerShortcut("Ctrl+]", "useCanvasKeyboard");
    registerShortcut("Ctrl+[", "useCanvasKeyboard");
    registerShortcut("Ctrl+Shift+]", "useCanvasKeyboard");
    registerShortcut("Ctrl+Shift+[", "useCanvasKeyboard");
    registerShortcut("Ctrl+G", "useCanvasKeyboard");
    registerShortcut("Ctrl+Shift+G", "useCanvasKeyboard");
    registerShortcut("Ctrl+0", "useCanvasKeyboard");
    registerShortcut("B", "useCanvasKeyboard");
    registerShortcut("E", "useCanvasKeyboard");
    registerShortcut("V", "useCanvasKeyboard");
    registerShortcut("M", "useCanvasKeyboard");
    registerShortcut("C", "useCanvasKeyboard");
    registerShortcut("I", "useCanvasKeyboard");
    registerShortcut("Space", "useCanvasKeyboard");
    registerShortcut("Alt", "useCanvasKeyboard");
    registerShortcut("[", "useCanvasKeyboard");
    registerShortcut("]", "useCanvasKeyboard");
    registerShortcut("Shift+[", "useCanvasKeyboard");
    registerShortcut("Shift+]", "useCanvasKeyboard");
    registerShortcut("0-9", "useCanvasKeyboard");
    registerShortcut("Delete", "useCanvasKeyboard");
    registerShortcut("Backspace", "useCanvasKeyboard");
    registerShortcut("Alt+Delete", "useCanvasKeyboard");
    registerShortcut("Ctrl+Delete", "useCanvasKeyboard");
    registerShortcut("Alt+Backspace", "useCanvasKeyboard");
    registerShortcut("Ctrl+Backspace", "useCanvasKeyboard");
    registerShortcut("F2", "useCanvasKeyboard");
    registerShortcut("Escape", "useCanvasKeyboard");
    registerShortcut("Enter", "useCanvasKeyboard");
    registerShortcut("Arrow keys", "useCanvasKeyboard");

    const handleKeyDown = async (e: KeyboardEvent) => {
      const active = document.activeElement;
      if (
        active &&
        (active.tagName === "INPUT" ||
          active.tagName === "TEXTAREA" ||
          (active as HTMLElement).isContentEditable)
      ) {
        return;
      }

      if (document.querySelector('[aria-modal="true"]')) return;

      options.stopMomentum();
      if (e.defaultPrevented) return;

      // Ctrl+Shift+T: Toggle UI chrome (panels and toolbars)
      if (e.key === "T" && e.ctrlKey && e.shiftKey && !e.altKey && !e.metaKey) {
        e.preventDefault();
        e.stopPropagation();
        editor.setChromeVisible((v) => !v);
        return;
      }

      // Dev-only: F5 reloads the webview when HMR gets stuck
      if (import.meta.env.DEV && e.key === "F5") {
        e.preventDefault();
        window.location.reload();
        return;
      }

      const engine = editor.workspace.getActiveEngine();
      if (!engine) return;

      const history = editor.workspace.getActiveHistory();
      if (!history) return;

      if (handleLayerFillKey(ctx, e, engine, history)) return;
      if (handleTransformSessionKey(ctx, e, engine, history)) return;
      if (handleCropToolKey(ctx, e)) return;
      if (handleSelectionToolKey(ctx, e, engine, history)) return;

      const key = e.key.toLowerCase();
      const ctrl = e.ctrlKey || e.metaKey;

      if (handleLayerOpsKey(ctx, e, engine, history, key, ctrl)) return;
      if (handleToolNavKey(ctx, e, engine, history, key, ctrl)) return;
    };

    const handleKeyUp = (e: KeyboardEvent) => {
      if (e.code === "Space") {
        options.setIsSpacePressed(false);
      }
      if (e.key === "Alt") {
        options.setIsAltPressed(false);
      }
    };

    const handleWindowBlur = () => {
      options.setIsSpacePressed(false);
      options.setIsPanning(false);
      options.setIsAltPressed(false);
    };

    // Capture phase: fires BEFORE bubble-phase handlers (useEditorCommands,
    // which also listens on window). This ensures canvas gets first dibs on
    // Ctrl+Z/Y for transform mini-undo. When canvas falls through (no mini
    // undo), it doesn't stopPropagation → editor commands handles it in bubble.
    window.addEventListener("keydown", handleKeyDown, { capture: true });
    window.addEventListener("keyup", handleKeyUp);
    window.addEventListener("blur", handleWindowBlur);

    onCleanup(() => {
      window.removeEventListener("keydown", handleKeyDown, { capture: true });
      window.removeEventListener("keyup", handleKeyUp);
      window.removeEventListener("blur", handleWindowBlur);
    });
  });
}
