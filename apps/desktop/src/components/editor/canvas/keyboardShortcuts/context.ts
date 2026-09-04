// SPDX-License-Identifier: AGPL-3.0-or-later
import { useEditor } from "../../shell/EditorContext";
import type { useLayerActions } from "../../layers/useLayerActions";

/** Return type of the shared layer-actions hook the panel/menu paths use. */
export type LayerActions = ReturnType<typeof useLayerActions>;

/**
 * Options for useCanvasKeyboard (extracted so handler modules can type their
 * options parameter without importing the hook — avoids a cycle).
 */
export interface CanvasKeyboardOptions {
  isSpacePressed: () => boolean;
  setIsSpacePressed: (pressed: boolean) => void;
  isAltPressed: () => boolean;
  setIsAltPressed: (pressed: boolean) => void;
  isPanning: () => boolean;
  setIsPanning: (panning: boolean) => void;
  stopMomentum: () => void;
  onSelectionChange?: () => void;
  fitToScreenAndRender: (animated?: boolean) => void;
  syncViewport: () => void;
  getCanvasContainerRef: () => HTMLDivElement | undefined;
}

/**
 * Everything the keyboard shortcut handler modules need from the editor.
 * `ReturnType<typeof useEditor>` keeps this in sync with EditorContext —
 * no manual duplication of ~70 accessor signatures.
 */
export type EditorAccessors = ReturnType<typeof useEditor>;

export interface KeyboardShortcutContext {
  editor: EditorAccessors;
  options: CanvasKeyboardOptions;
  /** Layer action funnels shared with the panel/menu paths (add/delete route
   *  through these so the facade flag-branch is decided in ONE place). */
  layerActions: LayerActions;
}
