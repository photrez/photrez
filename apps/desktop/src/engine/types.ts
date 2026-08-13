import type { BasicAdjustment } from "./layerAdjustments";
import type { TextData } from "./textTypes";

// ─── Identifiers ───
export type DocumentId = string;
export type LayerId = string;

// ─── Blend Modes ───
// Order follows shaders.ts blendColors() case ids (0-11). The simple modes
// (multiply/screen/overlay via source-over canvas ops + shader ids 1-3) shipped
// in v0.1.0; the advanced set (ids 4-11) was added in 0.2.0.
export type BlendMode =
  | "normal"
  | "multiply"
  | "screen"
  | "overlay"
  | "darken"
  | "lighten"
  | "color-dodge"
  | "color-burn"
  | "soft-light"
  | "hard-light"
  | "difference"
  | "exclusion";

// ─── Transform ───
export interface Transform2D {
  x: number;       // position x
  y: number;       // position y
  scaleX: number;  // default 1
  scaleY: number;  // default 1
  rotation: number; // degrees, default 0
  flipH: boolean;
  flipV: boolean;
}

// ─── Shape layers (spec #2, 2026-08-07) ───
export type ShapeKind =
  | "rect"
  | "ellipse"
  | "line"
  | "triangle"
  | "star"
  | "block-arrow"
  | "heart"
  | "diamond"
  | "speech-bubble"
  | "hexagon";

export interface ShapeStroke {
  enabled: boolean;
  color: string; // hex, e.g. "#E15A17" — matches fgColor/bgColor convention
  width: number; // px, clamped >= 1 at render
}

export interface ShapeFill {
  kind: "none" | "solid";
  color: string; // hex
}

export interface ShapeParams {
  kind: ShapeKind;
  width: number;  // shape box width, local space, >= 0 (0 for vertical line)
  height: number; // shape box height, local space, >= 0 (0 for horizontal line)
  radius: number; // rect corner radius, px, >= 0, clamped <= min(w,h)/2
  fill: ShapeFill;
  stroke: ShapeStroke;
  arrowHead: boolean; // line only
}

// ─── Layer ───
export interface LayerNode {
  id: LayerId;
  name: string;
  type: "raster" | "adjustment" | "group" | "shape" | "text";
  visible: boolean;
  opacity: number;   // 0.0 - 1.0
  locked: boolean;
  isBackground?: boolean;   // true for the initial document Background layer
  lockTransparency?: boolean;
  lockPosition?: boolean;
  lockRotation?: boolean;
  hasAdjustments?: boolean;
  basicAdjustment?: BasicAdjustment;
  baseImageBitmap?: ImageBitmap | null;
  blendMode: BlendMode;
  transform: Transform2D;
  width: number;
  height: number;
  // Pixel data reference — actual pixels stored as ImageBitmap or
  // texture handle, NOT as raw RGBA array in JS (too expensive)
  imageBitmap: ImageBitmap | null;
  /** Shape layer only: parametric source of truth. Absent for other types. */
  shapeParams?: ShapeParams;
  /** Text layer only: parametric source of truth. Absent for other types. */
  textData?: TextData;
}

// ─── Selection ───
export interface SelectionState {
  x: number;
  y: number;
  width: number;
  height: number;
  angle: number;
  /** Marquee shape. Rect = existing behavior; ellipse = ellipse marquee. Default "rect". */
  shape?: "rect" | "ellipse";
  /** When true, the selected pixels are everything outside these bounds. */
  inverted?: boolean;
}

// ─── Viewport ───
export interface ViewportState {
  panX: number;
  panY: number;
  zoom: number;      // 1.0 = 100%
  rotation: number;  // degrees, default 0
}

// ─── Document Model ───
export interface DocumentModel {
  id: DocumentId;
  name: string;
  width: number;
  height: number;
  layers: LayerNode[];
  activeLayerId: LayerId | null;
  selection: SelectionState | null;
  viewport: ViewportState;
  dirty: boolean;
}

// ─── Render State (sent to renderer) ───
export interface TextureHandle {
  id: string;
  glTexture?: WebGLTexture;
}

export interface RenderLayer {
  id: LayerId;
  textureHandle: TextureHandle;
  visible: boolean;
  opacity: number;
  blendMode: BlendMode;
  transform: Transform2D;
  width: number;
  height: number;
  basicAdjustment?: BasicAdjustment;
}

export interface RenderState {
  documentId: DocumentId;
  viewport: ViewportState;
  documentSize: { width: number; height: number };
  layers: RenderLayer[];
  selection: SelectionState | null;
  checkerboard: boolean;
  backgroundColor: [number, number, number, number]; // RGBA
}

// ─── Dirty Region ───
export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

// ─── Tiles ───
export const TILE_SIZE = 256;

export interface Tile {
  gridX: number;
  gridY: number;
  imageBitmap: ImageBitmap | null;
  width: number;
  height: number;
}

// ─── Constants ───
export const MAX_LAYERS = 200;
export const MAX_OPEN_DOCUMENTS = 16;
export const MAX_HISTORY_DEPTH = 50;
export const MAX_PIXEL_BUDGET = 1024 * 1024 * 1024; // 1 GB
// already costs 1 GB; anything above this is almost certainly a typo or
// a malicious input and would OOM the WebView2 process. 16384 matches
// WebGL2's common max texture size and the SavedWindowState clamp (Rust side).
export const MAX_CANVAS_DIM = 16384;

// ─── Device-adaptive dimension limit ───
// MAX_CANVAS_DIM is the app-level ceiling. At runtime, the renderer
// queries gl.MAX_TEXTURE_SIZE and calls setDeviceMaxTextureSize().
// getEffectiveMaxDim() returns Math.min(MAX_CANVAS_DIM, deviceMax)
// so low-end GPUs (8192) auto-clamp while high-end ones keep the full 16k.
let deviceMaxTextureSize: number = MAX_CANVAS_DIM;

export function setDeviceMaxTextureSize(size: number): void {
  deviceMaxTextureSize = Math.min(size, MAX_CANVAS_DIM);
}

export function getEffectiveMaxDim(): number {
  return deviceMaxTextureSize;
}

export interface DocumentTabSummary {
  id: DocumentId;
  displayName: string;
  isDirty: boolean;
}

export const DEFAULT_DOCUMENT_WIDTH = 800;
export const DEFAULT_DOCUMENT_HEIGHT = 600;
