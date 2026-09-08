// Ticket 1 — Minimal protocol types mirrors crates/core/src/protocol.rs
// contractVersion (schema) vs documentVersion (state) are distinct.

export const CONTRACT_VERSION = 2 as const;

export type DocumentVersion = number; // u64 in Rust, safe integer for now
export type ResourceId = number;

export type Rect = {
  x: number;
  y: number;
  width: number;
  height: number;
};

export type RenderLayer = {
  id: string;
  name: string;
  visible: boolean;
  opacity: number;
  resourceId: number;
  x: number;
  y: number;
  scaleX: number;
  scaleY: number;
  rotation: number;
  dirtyRect?: Rect | null;
  // Canonical metadata subset mirrored from the Rust RenderLayer (serde Option
  // fields, omitted when absent). Carried on add/upsert so the native engine
  // holds the full TS layer shape.
  layerType?: string;
  blendMode?: string;
  locked?: boolean;
  lockTransparency?: boolean;
  lockPosition?: boolean;
  lockRotation?: boolean;
  isBackground?: boolean;
  hasAdjustments?: boolean;
  flipH?: boolean;
  flipV?: boolean;
  width?: number;
  height?: number;
  // Nested parametric payloads (mirror Rust RenderLayer shape_params /
  // text_data / basic_adjustment). Omitted when absent so a v2 envelope without
  // them still parses. The typed-add / setLayerParams / setAdjustment arms set
  // these; the merge bridge takes a Some (Some(v) overrides the canonical base).
  shapeParams?: ShapeParams;
  textData?: TextData;
  basicAdjustment?: BasicAdjustment;
};

export type RenderLayerChange =
  | { kind: "upsert"; layer: RenderLayer }
  // ADR 0008 DeleteLayer ticket: carries resourceId for future resource
  // lifecycle (release/retain) without implementing the registry yet.
  | { kind: "remove"; id: string; resourceId: ResourceId };

export type RenderSnapshot = {
  version: DocumentVersion;
  layers: RenderLayer[];
};

export type RenderDelta = {
  baseVersion: DocumentVersion;
  version: DocumentVersion;
  changes: RenderLayerChange[];
};

export type TransformPatch = {
  x: number;
  y: number;
  scaleX: number;
  scaleY: number;
  rotation: number;
  flipH?: boolean;
  flipV?: boolean;
};

export type StrokePoint = { x: number; y: number; pressure: number };
export type BrushSettings = { size: number; hardness: number; opacity: number; flow: number };

// ── Parametric layer payloads (mirror the Rust canonical_model serde types) ──
// Field names match the Rust serde camelCase exactly (arrowHead, fontFamily,
// fontSize, fontWeight, fontStyle, lineHeight, letterSpacing, boxMode, boxWidth,
// boxHeight) so the wire JSON round-trips without remapping.
export type BasicAdjustment = {
  brightness: number;
  contrast: number;
  saturation: number;
};

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

export type ShapeFillKind = "none" | "solid";
export type ShapeFill = { kind: ShapeFillKind; color: string };
export type ShapeStroke = { enabled: boolean; color: string; width: number };
export type ShapeParams = {
  kind: ShapeKind;
  width: number;
  height: number;
  radius: number;
  fill: ShapeFill;
  stroke: ShapeStroke;
  arrowHead: boolean;
};

export type TextStrokeAlign = "outside" | "center" | "inside";
export type TextStroke = { width: number; color: string; align?: TextStrokeAlign };
export type TextFontStyle = "normal" | "italic";
export type TextAlign = "left" | "center" | "right";
export type TextBoxMode = "point" | "area";
export type TextData = {
  content: string;
  fontFamily: string;
  fontSize: number;
  fontWeight: number;
  fontStyle: TextFontStyle;
  color: string;
  align: TextAlign;
  lineHeight: number;
  letterSpacing: number;
  boxMode: TextBoxMode;
  boxWidth: number;
  boxHeight: number;
  stroke: TextStroke;
  underline?: boolean;
  strikethrough?: boolean;
  uppercase?: boolean;
};

export type Command =
  | { type: "noop" }
  | { type: "ping"; echo: string }
  // Host owns identity + placement: the id (TS-minted) and insertion index
  // (above the active layer) travel with the command. width/height seed the new
  // layer's dimensions. Mirrors the Rust AddLayer arm (command.rs).
  // Host owns identity + placement: the id (TS-minted) and insertion index
  // (above the active layer) travel with the command. width/height seed the new
  // layer's dimensions. Optional typed-add fields mirror the Rust AddLayer arm:
  // when present they project the real shape/text layer (blendMode stays Normal,
  // the matching nested payload rides verbatim); when absent the layer stays a
  // raster/normal layer (backward-compatible with v2 envelopes).
  | {
      type: "addLayer";
      id: string;
      name: string;
      width: number;
      height: number;
      index: number;
      layerType?: string;
      shapeParams?: ShapeParams;
      textData?: TextData;
    }
  | { type: "deleteLayer"; id: string }
  | { type: "transformLayer"; id: string; transform: TransformPatch }
  | { type: "setOpacity"; id: string; opacity: number }
  | { type: "brushStroke"; layerId: string; points: StrokePoint[]; settings: BrushSettings }
  | { type: "undo" }
  | { type: "redo" }
  // Metadata command arms (mirror the TS layer mutation ops so the native
  // ProtocolEngine owns them). Unknown id is a silent no-op on every arm,
  // matching the DeleteLayer arm and the TS apply ops (which guard on a missing
  // id) for bug-compatibility with the legacy engine.
  | { type: "setVisible"; id: string; visible: boolean }
  | { type: "setLocked"; id: string; kind: LockKind; locked: boolean }
  | { type: "rename"; id: string; name: string }
  | { type: "reorder"; id: string; to: number }
  | { type: "setBackgroundFlag"; id: string }
  | { type: "setBlendMode"; id: string; mode: string }
  // Typed parametric layer payload: mirror TS updateShapeParams/updateTextData.
  // Whichever of shapeParams/textData is present is written; both absent is an
  // invalid no-op (E_INVALID). Unknown id is a silent no-op.
  | { type: "setLayerParams"; id: string; shapeParams?: ShapeParams; textData?: TextData }
  // Non-destructive basic adjustment: Some sets it (hasAdjustments derived from the
  // values), undefined clears it and sets hasAdjustments false. Unknown id is a
  // silent no-op. Mirror TS applyBasicAdjustment/clearBasicAdjustments.
  | { type: "setAdjustment"; id: string; adjustment?: BasicAdjustment }
  // ADR 0008 H0: records a legacy TS transition into the canonical stream.
  // Advances DocumentVersion exactly once; payload stays behind the EXTERNAL
  // PayloadAdapter (token only).
  | {
      type: "recordExternalTransition";
      label: string;
      affectedLayerIds: string[];
      adapterId: string;
      token: string;
      memoryCostBytes: number;
    };

// The four TS layer-lock kinds (setLayerLocked + setLayerLock{Transparency,
// Position,Rotation}). `base` maps to RenderLayer.locked; the other three map to
// their named lock fields.
export type LockKind = "base" | "transparency" | "position" | "rotation";

export type CommandEnvelope = {
  contractVersion: number;
  expectedVersion?: DocumentVersion;
  command: Command;
  /** Document-scoped engine routing (per-document isolation). Absent routes to
   *  the reserved "default" engine (legacy / non-facade path). */
  docId?: string;
};

export type CommandResult = {
  documentVersion: DocumentVersion;
  delta: RenderDelta;
  /** Walker handoff (ADR 0008): "external" = host must execute via its
   *  adapter then confirm with historyCursorCommit. Absent = applied. */
  status?: "external" | "external-recorded" | "external-confirmed";
  externalSeq?: number;
};

// ── History query/projection (ADR 0008 H0) ───────────────────────────────
export type HistoryEntryView = {
  seq: number;
  groupId: number;
  origin: string; // "native" | "external:<adapterId>"
  label: string;
  affectedLayerIds: string[];
  versionBefore: DocumentVersion;
  versionAfter: DocumentVersion;
  memoryCostBytes: number;
  payloadRef?: string | null;
};

export type HistoryQueryResult = {
  cursor: number;
  lastSeq: number;
  degradedHint: boolean; // engine-side always false; TS merges markers
  /** Set while a walker handoff awaits host cursor_commit (ADR 0008 H0). */
  pendingExternal?: { seq: number; direction: string } | null;
  entries: HistoryEntryView[];
};

export type ProtocolError = {
  code: string;
  message: string;
};

export function isDeltaApplicable(
  delta: RenderDelta,
  renderedVersion: DocumentVersion,
): boolean {
  return delta.baseVersion === renderedVersion;
}
