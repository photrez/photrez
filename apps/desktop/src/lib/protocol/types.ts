// Ticket 1 — Minimal protocol types mirrors crates/core/src/protocol.rs
// contractVersion (schema) vs documentVersion (state) are distinct.

export const CONTRACT_VERSION = 1 as const;

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
};

export type StrokePoint = { x: number; y: number; pressure: number };
export type BrushSettings = { size: number; hardness: number; opacity: number; flow: number };

export type Command =
  | { type: "noop" }
  | { type: "ping"; echo: string }
  | { type: "addLayer"; name: string }
  | { type: "deleteLayer"; id: string }
  | { type: "transformLayer"; id: string; transform: TransformPatch }
  | { type: "setOpacity"; id: string; opacity: number }
  | { type: "brushStroke"; layerId: string; points: StrokePoint[]; settings: BrushSettings }
  | { type: "undo" }
  | { type: "redo" }
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
