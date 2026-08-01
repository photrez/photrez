// SPDX-License-Identifier: AGPL-3.0-or-later
import { useEditor } from "../../shell/EditorContext";
import type { PaintToolSettings } from "../../brushToolState";
import type { CropSnapTargets } from "@/viewport/cropSnap";
import type { ToolContext } from "@/viewport/input-handler";
import type { HudMode } from "../../TransformHud";

/**
 * Everything the pointer-tool handler modules need. `ReturnType<typeof
 * useEditor>` stays in sync with EditorContext — no manual duplication of
 * ~60 accessor signatures.
 */
export type EditorAccessors = ReturnType<typeof useEditor>;

export type SelectionBoxData = {
  x: number;
  y: number;
  w: number;
  h: number;
  angle: number;
  shape?: "rect" | "ellipse";
  inverted?: boolean;
} | null;

export type HudData = {
  mode: HudMode;
  clientX: number;
  clientY: number;
  deltaX: number;
  deltaY: number;
  width: number;
  height: number;
  scalePercent: number;
  angle: number;
  snapActive: boolean;
};

export type SnapLine = { x1: number; y1: number; x2: number; y2: number };

export interface PointerToolContext {
  editor: EditorAccessors;
  getCanvasContainerRef: () => HTMLDivElement | undefined;
  getCanvasRef: () => HTMLCanvasElement | undefined;
  isSpacePressed: () => boolean;
  isPanning: () => boolean;
  isAltPressed: () => boolean;
  stopMomentum: () => void;
  onPaintStroke?: (
    points: { x: number; y: number }[],
    isEraser: boolean,
    settings: PaintToolSettings,
    isFinal?: boolean,
  ) => void;
  cropSnapTargets?: () => CropSnapTargets | undefined;
  moveSnapEnabled?: () => boolean;
  /** Document-space pointer coords (signal-based; see getDocCoords). */
  getDocCoords: (e: PointerEvent) => { x: number; y: number };
  /** Local selection-marquee signal owned by the hook. */
  selectionBox: () => SelectionBoxData;
  setSelectionBoxSignal: (box: SelectionBoxData) => void;
  setSnapLines: (lines: SnapLine[]) => void;
  /** Hook-local HUD wrapper (converts to container-relative coords). */
  setHudInfo: (hud: HudData | null) => void;
  /** Local crop drag-preview signal owned by the hook. */
  setCropDragPreview: (preview: { x: number; y: number; w: number; h: number } | null) => void;
}

/**
 * Mutable drag state shared across pointer handlers (plain object, not
 * signals — matches the original `let` variables in useCanvasPointerTools).
 */
export interface ModernDragState {
  start: { x: number; y: number } | null;
  exceededThreshold: boolean;
  end: { x: number; y: number } | null;
  snappedPreview: { x: number; y: number; w: number; h: number } | null;
  isPendingCropClick: boolean;
  reset: () => void;
}

export interface GradientDragState {
  start: { x: number; y: number } | null;
  end: { x: number; y: number } | null;
  isDragging: boolean;
  reset: () => void;
}
