// Ticket 2.1/2.2: shared per-document EditorFacade registry + facade flag +
// transient transform-preview store.
//
// Pattern C ownership (ADR 0007): TS may hold TRANSIENT interaction state;
// only Rust holds persistent editor state. The preview signal below is
// strictly transient — it exists only between pointerdown and pointerup of a
// facade drag and is cleared on commit/cancel. The Rust RenderDelta projected
// at commit is the authoritative transform.
import { createSignal } from "solid-js";
import type { Transform2D, RenderState } from "@/engine/types";
import { HistoryQueryResult } from "./types";
import { CommandHistory } from "@/engine/history";
import { isFacadeOwnedLayer } from "@/engine/document";
import { EditorFacade } from "./editorFacade";
import {
  __resetEmulatedForTests as resetFacadeBridgeForTests,
  applyCommand,
  ensureNativeEngineSeeded,
  getHistoryQuery,
  getSnapshot,
  historyCursorCommit,
  isFacadeEnabled,
  isNativeAuthority,
  registerPayloadAdapter,
  setExternalTransitionPending,
  resetWasmDoc,
} from "./bridge";
import { repushCanonicalDocument } from "./canonicalSeed";
import type { DocumentEngine } from "@/engine/document";
import { CONTRACT_VERSION } from "./types";
import type { LockKind, LayerParamsPatch } from "./types";
import type { BasicAdjustment } from "@/engine/layerAdjustments";
import {
  clearFacadeStoreForTests,
  facadeDocIdsForTests,
  getFacade,
  peekFacade,
} from "./selectionMirror";
import type { FacadeProjectionSink } from "./selectionMirror";
import {
  refreshFacadeSnapshotFromEngine,
  toFacadeProjectionLayer,
} from "./facadeProjection";
import type { FacadeProjectionLayer } from "./facadeProjection";
export { isFacadeEnabled, isNativeAuthority };

// Selection-mirror funnel + the shared per-document facade store live in
// selectionMirror.ts (file-size guard). The public selection API is re-exported
// here so existing call sites keep importing from this module.
export {
  commitFacadeClearSelection,
  commitFacadeInvertSelection,
  commitFacadeSelectAll,
  commitFacadeSetSelection,
  getFacade,
  mirrorRestoredSelection,
  mirrorSelectionCommand,
  peekFacade,
  removeFacade,
} from "./selectionMirror";
export type { FacadeProjectionSink, SelectionRouteStatus } from "./selectionMirror";

// ── ADR 0008/Opacity: transient render previews ──────────────────────────
// Pure merge applied to the OUTGOING RenderState in EditorShell's scheduler.
// The engine model stays untouched during facade gestures; committed Rust
// deltas are authoritative. Unit-testable without a renderer.

export interface FacadeOpacityPreview {
  layerId: string;
  opacity: number;
}
const [opacityPreview, setOpacityPreview] = createSignal<FacadeOpacityPreview | null>(null);
export { opacityPreview, setOpacityPreview };
export function clearOpacityPreview(): void {
  setOpacityPreview(null);
}

// Same transient contract as the opacity preview, for basic adjustments: the
// slider writes the WHOLE gesture value here (render preview only, zero protocol
// commands) and the single SetAdjustment command fires at the gesture boundary
// (see AdjustmentsPanel.finishAdjustmentEdit). The engine model stays untouched
// during the drag, so the routed path cannot create one native undo entry per
// pointermove.
export interface FacadeAdjustmentPreview {
  layerId: string;
  adjustment: BasicAdjustment;
}
const [adjustmentPreview, setAdjustmentPreview] = createSignal<FacadeAdjustmentPreview | null>(null);
export { adjustmentPreview, setAdjustmentPreview };
export function clearAdjustmentPreview(): void {
  setAdjustmentPreview(null);
}

export function applyFacadePreviews(rs: RenderState): RenderState {
  const tps = transformPreview();
  const op = opacityPreview();
  const ap = adjustmentPreview();
  let layers = rs.layers;
  if (tps.length) {
    const byId = new Map(tps.map((p) => [p.layerId, p]));
    layers = layers.map((l) => {
      const p = byId.get(l.id);
      if (!p) return l;
      return {
        ...l,
        transform: p.transform,
        ...(p.width !== undefined ? { width: p.width } : {}),
        ...(p.height !== undefined ? { height: p.height } : {}),
      };
    });
  }
  if (op) {
    layers = layers.map((l) => (l.id === op.layerId ? { ...l, opacity: op.opacity } : l));
  }
  if (ap) {
    layers = layers.map((l) => (l.id === ap.layerId ? { ...l, basicAdjustment: ap.adjustment } : l));
  }
  return { ...rs, layers };
}

// ── Opacity commit funnel (ADR 0008 Opacity ticket) ──────────────────────
// Routes a possibly-multi target opacity edit through the shared selection
// policy. Facade ids get ONE SetOpacity command each (expectedVersion enforced
// inside EditorFacade.setOpacity) + authoritative projection. Legacy callers
// fall back to their untouched path when status==="legacy".

export type OpacityRouteStatus =
  | "applied"
  | "legacy"
  | "empty"
  | "mixed-rejected"
  | "noop";

export async function commitFacadeOpacity(
  engine: FacadeProjectionSink,
  ids: string[],
  opacity: number,
  facadeOverride?: EditorFacade
): Promise<{ status: OpacityRouteStatus; count?: number }> {
  const route = resolveSelectionRoute(ids);
  if (route.mode === "empty") return { status: "empty" };
  if (route.mode === "mixed-rejected") return { status: "mixed-rejected" };
  if (route.mode !== "facade") return { status: "legacy" };
  const f = facadeOverride ?? getFacade(engine.getId());
  let last: unknown = null;
  for (const id of route.ownedIds) {
    last = await f.setOpacity(id, opacity);
  }
  if (last) engine.applyFacadeSnapshot(last, { dimsAuthoritative: f.lastProjectionDimsAuthoritative });
  return { status: "applied", count: route.ownedIds.length };
}

// ── Metadata commit funnels (mirror commitFacadeOpacity / ADR 0008) ───────
// Route a possibly-multi target metadata edit through the shared selection
// policy. Facade ids get ONE command each (expectedVersion enforced inside the
// EditorFacade metadata method) + authoritative projection; legacy callers
// fall back to their untouched path when status==="legacy". Same ownership
// resolution, version bookkeeping, error propagation, and TS projection as
// commitFacadeOpacity — the only difference is the facade method + command
// shape (see editorFacade.ts setLayerVisibility / setLayerName /
// setLayerLocked / setLayerBlendMode, already wired to the native Rust arms
// SetVisible / Rename / SetLocked / SetBlendMode).
export type FacadeRouteStatus =
  | "applied"
  | "legacy"
  | "empty"
  | "mixed-rejected"
  | "noop";

export async function commitFacadeVisibility(
  engine: FacadeProjectionSink,
  ids: string[],
  visible: boolean,
  facadeOverride?: EditorFacade,
): Promise<{ status: FacadeRouteStatus; count?: number }> {
  const route = resolveSelectionRoute(ids);
  if (route.mode === "empty") return { status: "empty" };
  if (route.mode === "mixed-rejected") return { status: "mixed-rejected" };
  if (route.mode !== "facade") return { status: "legacy" };
  const f = facadeOverride ?? getFacade(engine.getId());
  let last: unknown = null;
  for (const id of route.ownedIds) {
    last = await f.setLayerVisibility(id, visible);
  }
  if (last) engine.applyFacadeSnapshot(last, { dimsAuthoritative: f.lastProjectionDimsAuthoritative });
  return { status: "applied", count: route.ownedIds.length };
}

export async function commitFacadeRename(
  engine: FacadeProjectionSink,
  ids: string[],
  name: string,
  facadeOverride?: EditorFacade,
): Promise<{ status: FacadeRouteStatus; count?: number }> {
  const route = resolveSelectionRoute(ids);
  if (route.mode === "empty") return { status: "empty" };
  if (route.mode === "mixed-rejected") return { status: "mixed-rejected" };
  if (route.mode !== "facade") return { status: "legacy" };
  const f = facadeOverride ?? getFacade(engine.getId());
  let last: unknown = null;
  for (const id of route.ownedIds) {
    last = await f.setLayerName(id, name);
  }
  if (last) engine.applyFacadeSnapshot(last, { dimsAuthoritative: f.lastProjectionDimsAuthoritative });
  return { status: "applied", count: route.ownedIds.length };
}

export async function commitFacadeLock(
  engine: FacadeProjectionSink,
  ids: string[],
  kind: LockKind,
  locked: boolean,
  facadeOverride?: EditorFacade,
): Promise<{ status: FacadeRouteStatus; count?: number }> {
  const route = resolveSelectionRoute(ids);
  if (route.mode === "empty") return { status: "empty" };
  if (route.mode === "mixed-rejected") return { status: "mixed-rejected" };
  if (route.mode !== "facade") return { status: "legacy" };
  const f = facadeOverride ?? getFacade(engine.getId());
  let last: unknown = null;
  for (const id of route.ownedIds) {
    last = await f.setLayerLocked(id, kind, locked);
  }
  if (last) engine.applyFacadeSnapshot(last, { dimsAuthoritative: f.lastProjectionDimsAuthoritative });
  return { status: "applied", count: route.ownedIds.length };
}

export async function commitFacadeBlendMode(
  engine: FacadeProjectionSink,
  ids: string[],
  mode: string,
  facadeOverride?: EditorFacade,
): Promise<{ status: FacadeRouteStatus; count?: number }> {
  const route = resolveSelectionRoute(ids);
  if (route.mode === "empty") return { status: "empty" };
  if (route.mode === "mixed-rejected") return { status: "mixed-rejected" };
  if (route.mode !== "facade") return { status: "legacy" };
  const f = facadeOverride ?? getFacade(engine.getId());
  let last: unknown = null;
  for (const id of route.ownedIds) {
    last = await f.setLayerBlendMode(id, mode);
  }
  if (last) engine.applyFacadeSnapshot(last, { dimsAuthoritative: f.lastProjectionDimsAuthoritative });
  return { status: "applied", count: route.ownedIds.length };
}

// Basic-adjustment funnel: one optional-payload command per owned id. A defined
// adjustment sets it; undefined clears it (the same method serves apply + reset).
// Mirrors commitFacadeBlendMode exactly: shared selection policy, expectedVersion
// enforced inside setLayerAdjustment, authoritative projection, and a legacy
// fall-through when no target is facade-owned.
export async function commitFacadeAdjustment(
  engine: FacadeProjectionSink,
  ids: string[],
  adjustment?: BasicAdjustment,
  facadeOverride?: EditorFacade,
): Promise<{ status: FacadeRouteStatus; count?: number }> {
  const route = resolveSelectionRoute(ids);
  if (route.mode === "empty") return { status: "empty" };
  if (route.mode === "mixed-rejected") return { status: "mixed-rejected" };
  if (route.mode !== "facade") return { status: "legacy" };
  const f = facadeOverride ?? getFacade(engine.getId());
  let last: unknown = null;
  for (const id of route.ownedIds) {
    last = await f.setLayerAdjustment(id, adjustment);
  }
  if (last) engine.applyFacadeSnapshot(last, { dimsAuthoritative: f.lastProjectionDimsAuthoritative });
  return { status: "applied", count: route.ownedIds.length };
}

// Parametric-payload funnel (text/shape params). Mirrors commitFacadeAdjustment:
// shared selection policy, expectedVersion enforced inside setLayerParams,
// authoritative projection, legacy fall-through when no target is facade-owned.
// Takes an id SET so the mixed-ownership rejection is reachable at this boundary
// exactly like the other metadata funnels; the production caller passes one id.
export async function commitFacadeParams(
  engine: FacadeProjectionSink,
  ids: string[],
  params: LayerParamsPatch,
  facadeOverride?: EditorFacade,
): Promise<{ status: FacadeRouteStatus; count?: number }> {
  const route = resolveSelectionRoute(ids);
  if (route.mode === "empty") return { status: "empty" };
  if (route.mode === "mixed-rejected") return { status: "mixed-rejected" };
  if (route.mode !== "facade") return { status: "legacy" };
  const f = facadeOverride ?? getFacade(engine.getId());
  let last: unknown = null;
  for (const id of route.ownedIds) {
    last = await f.setLayerParams(id, params);
  }
  if (last) engine.applyFacadeSnapshot(last, { dimsAuthoritative: f.lastProjectionDimsAuthoritative });
  return { status: "applied", count: route.ownedIds.length };
}

// Reorder is a SINGLE-target op (one layer moves to a destination index), so the
// funnel takes a single id rather than a batch. The destination index comes from
// the production caller's existing toIndex (the legacy reorderLayer(from,to)
// contract); the command arm uses the same post-removal insertion index, so the
// value passes through unchanged. Consumes the command through the delta path:
// the Reorder arm emits an ordered FULL RESTATEMENT which applyDeltaToSnapshot
// adopts, so the projection order matches the authoritative engine WITHOUT any
// snapshot re-read.
export async function commitFacadeReorder(
  engine: FacadeProjectionSink,
  id: string,
  toIndex: number,
  facadeOverride?: EditorFacade,
): Promise<{ status: FacadeRouteStatus; count?: number }> {
  const route = resolveSelectionRoute([id]);
  if (route.mode === "empty") return { status: "empty" };
  if (route.mode === "mixed-rejected") return { status: "mixed-rejected" };
  if (route.mode !== "facade") return { status: "legacy" };
  // Reorder is a structural move that must not round-trip through the wasm
  // engine: under wasm authority the engine only ever knows facade-created
  // layers (legacy TS layers are never seeded into it), so the command would
  // reorder a PARTIAL set and the restated order would contradict the TS
  // model. The legacy TS reorder still preserves order in TS-only state, so
  // defer to it. Under native authority the engine
  // holds the FULL layer set (ensureNativeEngineSeeded), and the routed command
  // restores order through its ordered restatement delta (see editorFacade
  // reorderLayer + applyDeltaToSnapshot - no snapshot re-read anywhere).
  if (!isNativeAuthority()) return { status: "legacy" };
  const f = facadeOverride ?? getFacade(engine.getId());
  const last = await f.reorderLayer(id, toIndex);
  if (last) engine.applyFacadeSnapshot(last, { dimsAuthoritative: f.lastProjectionDimsAuthoritative });
  return { status: "applied", count: 1 };
}

// ── Structural commit funnels (duplicate / merge / flatten / rasterize) ──
// Route a structural graph op's GRAPH half to the native command arms; pixel
// compositing stays host-side (the caller composites with the same pure helpers
// the legacy engine uses, then attaches via engine.setLayerImageBitmap).
//
// Ownership policy mirrors commitFacadeOpacity resolution EXACTLY: single-target
// ops resolve [id]; mergeSelected resolves over the full id set and MUST return
// {status:"mixed-rejected"} with ZERO commands when ownership is mixed
// (atomicity precedent — no partial native mutation). Unlike metadata funnels,
// structural ops are gated to native authority: under wasm authority the engine
// knows only facade-created layers, so a merge/flatten would operate on a
// PARTIAL set and drop layers (data-loss class) — the same reason commitFacadeReorder
// defers to legacy there. Under native authority the engine holds the FULL layer
// set (ensureNativeEngineSeeded), so the routed command restores graph order
// through its ordered restatement delta with no snapshot re-read. Returns the
// applied command count.
export async function commitFacadeDuplicate(
  engine: FacadeProjectionSink,
  id: string,
  newId: string,
  facadeOverride?: EditorFacade,
): Promise<{ status: FacadeRouteStatus; count?: number }> {
  if (!isFacadeEnabled() || !isNativeAuthority()) return { status: "legacy" };
  const route = resolveSelectionRoute([id]);
  if (route.mode === "empty") return { status: "empty" };
  if (route.mode === "mixed-rejected") return { status: "mixed-rejected" };
  if (route.mode !== "facade") return { status: "legacy" };
  const f = facadeOverride ?? getFacade(engine.getId());
  const last = await f.duplicateLayer(id, newId);
  if (last) engine.applyFacadeSnapshot(last, { dimsAuthoritative: f.lastProjectionDimsAuthoritative });
  return { status: "applied", count: 1 };
}

export async function commitFacadeMergeDown(
  engine: FacadeProjectionSink,
  id: string,
  mergedId: string,
  facadeOverride?: EditorFacade,
): Promise<{ status: FacadeRouteStatus; count?: number }> {
  if (!isFacadeEnabled() || !isNativeAuthority()) return { status: "legacy" };
  const route = resolveSelectionRoute([id]);
  if (route.mode === "empty") return { status: "empty" };
  if (route.mode === "mixed-rejected") return { status: "mixed-rejected" };
  if (route.mode !== "facade") return { status: "legacy" };
  const f = facadeOverride ?? getFacade(engine.getId());
  const last = await f.mergeDown(id, mergedId);
  if (last) engine.applyFacadeSnapshot(last, { dimsAuthoritative: f.lastProjectionDimsAuthoritative });
  return { status: "applied", count: 1 };
}

export async function commitFacadeMergeSelected(
  engine: FacadeProjectionSink,
  ids: string[],
  mergedId: string,
  facadeOverride?: EditorFacade,
): Promise<{ status: FacadeRouteStatus; count?: number }> {
  if (!isFacadeEnabled() || !isNativeAuthority()) return { status: "legacy" };
  const route = resolveSelectionRoute(ids);
  if (route.mode === "empty") return { status: "empty" };
  // Mixed ownership across the selected set is rejected ATOMICALLY: zero native
  // commands fire, so no partial merge leaves a dangling half-composited layer.
  if (route.mode === "mixed-rejected") return { status: "mixed-rejected" };
  if (route.mode !== "facade") return { status: "legacy" };
  const f = facadeOverride ?? getFacade(engine.getId());
  const last = await f.mergeSelectedLayers(ids, mergedId);
  if (last) engine.applyFacadeSnapshot(last, { dimsAuthoritative: f.lastProjectionDimsAuthoritative });
  return { status: "applied", count: 1 };
}

export async function commitFacadeFlatten(
  engine: FacadeProjectionSink,
  mergedId: string,
  facadeOverride?: EditorFacade,
): Promise<{ status: FacadeRouteStatus; count?: number }> {
  if (!isFacadeEnabled() || !isNativeAuthority()) return { status: "legacy" };
  const f = facadeOverride ?? getFacade(engine.getId());
  const last = await f.flattenLayers(mergedId);
  if (last) engine.applyFacadeSnapshot(last, { dimsAuthoritative: f.lastProjectionDimsAuthoritative });
  return { status: "applied", count: 1 };
}

export async function commitFacadeRasterize(
  engine: FacadeProjectionSink,
  id: string,
  facadeOverride?: EditorFacade,
): Promise<{ status: FacadeRouteStatus; count?: number }> {
  if (!isFacadeEnabled() || !isNativeAuthority()) return { status: "legacy" };
  const route = resolveSelectionRoute([id]);
  if (route.mode === "empty") return { status: "empty" };
  if (route.mode === "mixed-rejected") return { status: "mixed-rejected" };
  if (route.mode !== "facade") return { status: "legacy" };
  const f = facadeOverride ?? getFacade(engine.getId());
  const last = await f.rasterizeLayer(id);
  if (last) engine.applyFacadeSnapshot(last, { dimsAuthoritative: f.lastProjectionDimsAuthoritative });
  return { status: "applied", count: 1 };
}

// ── Canvas-size commit funnels (resize canvas / apply crop) ──────────────
// Route the document-size ops to the native canvas arms. Same native-authority
// gate as the structural funnels: under wasm authority the routed arm cannot own
// the document size (the projection would contradict the TS model), so defer to
// the legacy engine path. The canvas arms emit an empty layer delta plus the new
// size on delta.width/height; the facade's delta consumer adopts those and marks
// the projection authoritative for the size, then the projection below pushes the
// same size into the TS model (applyFacadeSnapshot writes model width/height only
// for an authoritative projection whose size differs).
export async function commitFacadeResizeCanvas(
  engine: FacadeProjectionSink,
  width: number,
  height: number,
  facadeOverride?: EditorFacade,
): Promise<{ status: FacadeRouteStatus; count?: number }> {
  if (!isFacadeEnabled() || !isNativeAuthority()) return { status: "legacy" };
  const f = facadeOverride ?? getFacade(engine.getId());
  const last = await f.resizeCanvas(width, height);
  if (last) engine.applyFacadeSnapshot(last, { dimsAuthoritative: f.lastProjectionDimsAuthoritative });
  return { status: "applied", count: 1 };
}

export async function commitFacadeApplyCrop(
  engine: FacadeProjectionSink,
  x: number,
  y: number,
  width: number,
  height: number,
  rotation?: number,
  targetWidth?: number,
  targetHeight?: number,
  facadeOverride?: EditorFacade,
): Promise<{ status: FacadeRouteStatus; count?: number }> {
  if (!isFacadeEnabled() || !isNativeAuthority()) return { status: "legacy" };
  const f = facadeOverride ?? getFacade(engine.getId());
  const last = await f.applyCrop(x, y, width, height, rotation, targetWidth, targetHeight);
  if (last) engine.applyFacadeSnapshot(last, { dimsAuthoritative: f.lastProjectionDimsAuthoritative });
  return { status: "applied", count: 1 };
}

// Selection commit mirrors + serialized dispatch moved to selectionMirror.ts (re-exported above).

// ── ADR 0009: mixed-selection policy helper ─────────────────────────────
// Single source of truth for routing a batch target set during the migration
// window. PURE with respect to editor state: reads the ownership set + flag,
// mutates nothing, performs no I/O, emits no UI effects.
export const MIXED_OWNERSHIP_MESSAGE =
  "Cannot apply this to a mixed selection: selected layers span both migrated (Rust) and legacy layers. Select one group at a time.";

export type SelectionRoute =
  | { mode: "empty" } // [] -> silent no-op; never routes anywhere
  | { mode: "legacy" } // execute via existing legacy path
  | { mode: "facade"; ownedIds: string[] } // execute via facade commands
  | { mode: "mixed-rejected" }; // caller MUST show MIXED_OWNERSHIP_MESSAGE + abort

export function resolveSelectionRoute(ids: string[]): SelectionRoute {
  // Dedupe first (ADR 0009 §5): ["a","a","b"] ≡ ["a","b"]; multiplicity is
  // intentionally not preserved. First-occurrence order kept for ownedIds.
  const unique = [...new Set(ids)];
  if (unique.length === 0) return { mode: "empty" };
  const ownedIds = unique.filter((id) => isFacadeOwnedLayer(id));
  if (ownedIds.length === 0) return { mode: "legacy" };
  if (ownedIds.length === unique.length) return { mode: "facade", ownedIds };
  return { mode: "mixed-rejected" };
}

// ── Transient drag preview ────────────────────────────────────────────────
// Written during a facade drag (pointermove) and read by the render scheduler's
// outgoing RenderState (applyFacadePreviews above) and by the selection-overlay
// memos (handle/HUD tracking). Never persisted: the committed Rust delta is
// authoritative and clears it. A list rather than one entry because the canvas
// move gesture drags every selected layer at once, and a dragged layer whose
// preview is missing reads as a frozen layer.
export interface FacadeTransformPreview {
  layerId: string;
  transform: Transform2D;
  // Box size for a gesture that resizes the layer's own quad instead of only
  // moving it (the text overlay's corner resize). The renderer draws the quad
  // from RenderState width/height, and a routed resize leaves the model size
  // untouched, so without these the quad would move to the new position still
  // drawn at the old size. Optional: the move/scale gestures below write the
  // transform only and keep the model size.
  width?: number;
  height?: number;
}
const [transformPreview, setTransformPreview] = createSignal<FacadeTransformPreview[]>([]);
export { transformPreview, setTransformPreview };
export function clearTransformPreview(): void {
  setTransformPreview([]);
}
// Overlay geometry (selection box, transform handles, HUD) reads this so it tracks
// a gesture the model never sees: a routed transform gesture writes only the preview,
// so a box computed off the raw model would sit still while the pixels move.
export function previewedTransformOf(layerId: string, fallback: Transform2D): Transform2D {
  const p = transformPreview().find((entry) => entry.layerId === layerId);
  return p ? p.transform : fallback;
}

// ── Numeric transform commit (Ticket 2.2 refinement) ─────────────────────
// One committed numeric edit/group = exactly ONE TransformLayer Rust command
// with expectedVersion, then authoritative projection. No persistent TS
// mutation: the TS model only ever receives the Rust RenderDelta snapshot.
//
// WHY THE PER-DOCUMENT CHAIN: the facade holds ONE transient transform slot and
// only advances renderedVersion after a command resolves. Two commits started in
// the same tick (two quick X/Y field submits, a keyboard flip issued while an
// earlier commit is still in flight, one Align batch over several layers)
// therefore read the same expectedVersion and the arm rejects the second with
// E_VERSION_MISMATCH; and a commit that begins while a pointer gesture owns the
// slot overwrites that gesture, whose own commitTransform then finds an empty
// slot and drops the drag. Running each call's whole read -> begin -> update ->
// commit -> project sequence as one hop on a chain keyed by the facade's document
// id fixes both: commits land in issue order and each reads the model the
// previous one projected. Keep the map bounded: drop the entry once that hop has
// settled, unless a newer dispatch already replaced it.
//
// A patch may also be a function of the layer's CURRENT projected transform. The
// flip callers use that: the flag to send is decided inside the hop, so a second
// flip queued behind the first negates what the first actually committed instead
// of re-sending the flag read at click time.
export type NumericTransformPatch =
  | Partial<Transform2D>
  | ((current: Transform2D) => Partial<Transform2D>);

const numericCommitTailByDoc = new Map<string, { tail: Promise<unknown>; queuedAt: number }>();

// A hop that never settles (a protocol call that hangs) parks every later commit
// on the same document with no trace. Warn about it instead of timing it out: an
// in-flight command must be allowed to finish, and a watchdog timer that keeps
// the process alive is worse than the hang it reports.
const STALLED_TAIL_WARN_MS = 5000;

function serializeNumericCommit<T>(docKey: string, run: () => Promise<T>): Promise<T> {
  const waiting = numericCommitTailByDoc.get(docKey);
  if (waiting) {
    const waitedMs = Date.now() - waiting.queuedAt;
    if (waitedMs > STALLED_TAIL_WARN_MS) {
      console.warn(
        `[numeric-transform-commit] ${docKey} queued a commit behind one that has not settled for ${Math.round(waitedMs)}ms; a protocol call may be stuck.`,
      );
    }
  }
  const previous = waiting?.tail ?? Promise.resolve();
  const settled = previous.then(run, run);
  const tail = settled.then(
    () => undefined,
    () => undefined,
  );
  const entry = { tail, queuedAt: Date.now() };
  numericCommitTailByDoc.set(docKey, entry);
  void tail.then(() => {
    if (numericCommitTailByDoc.get(docKey) === entry) numericCommitTailByDoc.delete(docKey);
  });
  return settled;
}

export function facadeCommitNumericTransform(
  engine: {
    getId(): string;
    getLayer(id: string): { id: string; locked: boolean; transform: Transform2D } | null | undefined;
    applyFacadeSnapshot(s: unknown): void;
  },
  layerId: string,
  patch: NumericTransformPatch,
): Promise<boolean> {
  // Self-guard, not a caller convention (same shape as mirrorSelectionCommand):
  // flag OFF never reaches the queue, so the legacy paths keep their synchronous
  // shape no matter which caller is added later.
  if (!isFacadeEnabled()) return Promise.resolve(false);
  // "Nothing to do" is decided at ISSUE time, inside the click handler: a layer
  // that is already gone or locked stays a silent no-op, as it always was. What
  // changes between issue and hop is re-checked inside the hop, where it is a
  // visible failure instead (see commitNumericTransformOnce).
  const atIssue = engine.getLayer(layerId);
  if (!atIssue || atIssue.locked) return Promise.resolve(false);
  // getFacade resolves the same key the slot lives on, so chain and facade
  // always address one document.
  const facade = getFacade(engine.getId());
  return serializeNumericCommit(facade.docId, () =>
    commitNumericTransformOnce(engine, facade, layerId, patch),
  );
}

async function commitNumericTransformOnce(
  engine: {
    getLayer(id: string): { id: string; locked: boolean; transform: Transform2D } | null | undefined;
    applyFacadeSnapshot(s: unknown): void;
  },
  facade: EditorFacade,
  layerId: string,
  patch: NumericTransformPatch,
): Promise<boolean> {
  const layer = engine.getLayer(layerId);
  // The layer was there at issue time and is gone (or locked) now: a Delete or a
  // lock toggle landed while an earlier commit was in flight. Returning false
  // here would throw the edit away silently, so it fails loud and the caller's
  // error toast makes it visible.
  if (!layer) throw new Error(`Layer ${layerId} no longer exists.`);
  if (layer.locked) throw new Error(`Layer ${layerId} is locked.`);
  const next = { ...layer.transform, ...(typeof patch === "function" ? patch(layer.transform) : patch) };
  const same =
    next.x === layer.transform.x &&
    next.y === layer.transform.y &&
    next.scaleX === layer.transform.scaleX &&
    next.scaleY === layer.transform.scaleY &&
    next.rotation === layer.transform.rotation &&
    next.flipH === layer.transform.flipH &&
    next.flipV === layer.transform.flipV;
  if (same) return false;
  if (facade.transientTransformActive()) {
    // A pointer gesture holds the slot; beginning here would take it over and
    // the gesture's own commit would find nothing to apply. Refusing out loud is
    // the only option that keeps both edits: the queue cannot help, because the
    // gesture spans an unbounded number of pointermove tasks.
    throw new Error(
      `Layer ${layerId} cannot be committed while a transform gesture is in progress. Release the current handle first.`,
    );
  }
  facade.beginTransform(layerId, { ...layer.transform });
  facade.updateTransform(next);
  const snap = await facade.commitTransform();
  if (!snap) {
    // Defensive net, not a known failure path: commitTransform() null-checks the
    // slot synchronously and there is no await between beginTransform and this
    // call, so no other task can take the slot here. Kept so a future change to
    // that method can never make a no-command edit report success.
    throw new Error(`Transform commit for layer ${layerId} produced no command result.`);
  }
  engine.applyFacadeSnapshot(snap);
  return true;
}

// ── One-time facade seeding ──────────────────────────────────────────────
// A fresh facade knows nothing about pre-existing TS layers; without seeding,
// its first full-snapshot projection would CLOBBER them (applyFacadeSnapshot
// replaces the model with the facade view). Seed once from the engine's
// current layers as an initial metadata-only snapshot. The same layer mapping
// (toFacadeProjectionLayer, facadeProjection.ts) re-projects the model into the
// snapshot after a mirrored external transition and at DocumentEngine.notifyChange
// (the post-mutation choke point).
export async function seedFacadeFromEngine(
  engine: {
    getId(): string;
    getLayers(): Array<FacadeProjectionLayer>;
  },
  facade: EditorFacade
): Promise<void> {
  if (facade.snapshot.layers.length === 0 && engine.getLayers().length > 0) {
    facade.seedSnapshot({
      version: 0,
      layers: engine.getLayers().map(toFacadeProjectionLayer),
    });
  }
  // Native-authority cutover seed: mirror the live TS/facade state into the
  // Rust REGISTRY engine so the first rerouted command's expectedVersion matches.
  // Gated by isNativeAuthority() so the default (wasm) path is byte-identical.
  // Carries the live renderedVersion (not a hardcoded 0) so the seeded engine
  // starts at the same document version the facade holds.
  if (isNativeAuthority()) {
    // Bind the TS engine to the facade so addLayer can re-push the full canonical
    // document after Rust mints a layer (Rust cannot populate the canonical-only
    // fields). Gated so the default (wasm) path is byte-identical.
    facade.bindEngine(engine as unknown as DocumentEngine);
    await ensureNativeEngineSeeded(
      facade.docId,
      facade.renderedVersion,
      engine.getLayers().map((l) => ({
        id: l.id,
        name: l.name,
        visible: l.visible,
        opacity: l.opacity,
        x: l.transform.x,
        y: l.transform.y,
        scaleX: l.transform.scaleX,
        scaleY: l.transform.scaleY,
        rotation: l.transform.rotation,
        resourceId: 0,
      })),
    );
    // The native engine is the SINGLE owner of document version. Read its actual
    // version (protocol_snapshot_native returns RenderSnapshot.version === the
    // engine's documentVersion) and align the facade up-only, so a subsequent
    // facade command's expectedVersion matches. Up-only: a raw assign could move
    // renderedVersion backward if pixel commits had already advanced past the seed.
    const snap = await getSnapshot(facade.docId);
    facade.syncRenderedVersionTo(snap.version);
  }
}

// ── ADR 0008 H0: external-transition recording + degraded state ──────────
// Feature-gated commit shim: when photrez.facade=1, every legacy TS
// CommandHistory.commit is mirrored into the canonical Rust stream via
// RecordExternalTransition (payload stays TS-side behind the external
// adapter). When the flag is OFF the wrapper exits immediately — zero
// runtime work on the legacy path.

export interface ExternalRecordPayloadCell {
  label: string;
  affectedLayerIds: string[];
  snapshot: unknown; // scoped legacy payload (H0: whole pre-snapshot)
}
const tsPayloadStore = new Map<string, ExternalRecordPayloadCell>();

type Marker =
  | { kind: "pendingRecord"; token: string; label: string }
  | { kind: "pendingConfirm"; seq: number; direction: "undo" | "redo" };
let pendingMarkers: Marker[] = [];

const [historyDegraded, setHistoryDegraded] = createSignal<
  { reason: string; markers: Marker[] } | null
>(null);
export { historyDegraded };

function syncAuthoritativeVersion(docId: string, dv: number): void {
  const f = peekFacade(docId);
  if (f) f.syncRenderedVersionTo(dv);
}

// Native-authority version sync (ADR 0014): when the native engine is the
// authority, pixel commits and facade commands share ONE ProtocolEngine instance,
// so a pixel commit between two facade commands advances the same documentVersion
// the facade reads. Push the pixel commit's returned version into the facade so
// the next facade command is not rejected with E_VERSION_MISMATCH. No-op unless
// native authority is active and a facade exists for the doc, so the wasm default
// path is byte-identical. The native engine is the single owner of document
// version; this is the only place that mirrors it into the facade, up-only.
export function syncFacadeVersionFromPixel(docId: string, version: number): void {
  if (!isNativeAuthority()) return;
  // Ensure a facade exists before syncing: a pixel commit can advance the native
  // engine DV before any facade has been lazily created (getFacade is lazy on the
  // opacity/transform path). Without this the sync no-ops and a later facade
  // command is rejected with E_VERSION_MISMATCH. Gated by isNativeAuthority above.
  if (!peekFacade(docId)) getFacade(docId);
  const f = peekFacade(docId);
  if (f) f.syncRenderedVersionTo(version);
}

export async function recordExternalTransitionFor(
  docId: string,
  rec: { label: string; affectedLayerIds: string[]; snapshot: unknown },
  engine?: DocumentEngine
): Promise<{ ok: boolean; seq?: number }> {
  // Deterministic degraded behavior (ADR 0008 H0 review): while degraded, no
  // further protocol attempts — fail fast without touching the engine.
  const deg = historyDegraded();
  if (deg) return { ok: false };
  const token = `ts:${docId}:${crypto.randomUUID()}`;
  tsPayloadStore.set(token, {
    label: rec.label,
    affectedLayerIds: rec.affectedLayerIds,
    snapshot: rec.snapshot,
  });
  const marker: Marker = { kind: "pendingRecord", token, label: rec.label };
  pendingMarkers.push(marker); // detection marker BEFORE the record call (ADR)
  // The projection refresh below reads the model AFTER the record round-trip
  // (see the refresh call). Several legacy paths commit BEFORE the mutation they
  // are about to make (Stamp Visible, cross-document move/import), so a vector
  // captured HERE, at commit time, would be missing the layer the mutation adds;
  // projecting it would teach the snapshot a pre-mutation set and the next routed
  // op would drop the new layer. The post-mutation choke point
  // (DocumentEngine.notifyChange) has already refreshed from the same model by the
  // time the await resolves, so this read re-projects the same post-mutation
  // vector: it is redundant by design, not a demonstrated necessity. It is kept
  // because it is cheap and guards a future mutation path that bypasses the choke
  // point. It is auxiliary and gated, so a minimal/throwing engine stub degrades
  // to "no refresh" and never fails the history mirror.
  try {
    // Per-document engine owns its adapters, so register "ts-external" on this
    // doc's engine before every external transition. Rust `register_adapter` is
    // idempotent (protocol.rs) and the emulator Set dedupes, so this is a free
    // unconditional call that is safe even after a per-doc engine reset.
    registerPayloadAdapter("ts-external", docId);
    const res = await applyCommand({
      contractVersion: CONTRACT_VERSION,
      expectedVersion: undefined, // mirrors may land after facade ops; engine DV is authority
      docId,
      command: {
        type: "recordExternalTransition",
        label: rec.label,
        affectedLayerIds: rec.affectedLayerIds,
        adapterId: "ts-external",
        token,
        memoryCostBytes: JSON.stringify(rec.snapshot ?? {}).length,
      },
    });
    // Authoritative-version refresh (ADR C2): mirrors advance DV outside
    // facade calls — every envelope builder must see the new DV.
    if (!peekFacade(docId)) getFacade(docId); // ensure instance exists for future refresh
    syncAuthoritativeVersion(docId, res.documentVersion);
    // Redundant-by-design refresh (see the note above the try): the mirrored
    // transition changed the TS model outside the facade command path, and this
    // reads the same post-mutation vector DocumentEngine.notifyChange already
    // projected. Kept as a cheap net over a future mutation path that bypasses
    // the choke point. Gated: no-op when the facade flag is OFF (default path
    // byte-identical).
    if (engine && isFacadeEnabled()) {
      try {
        refreshFacadeSnapshotFromEngine(docId, engine.getLayers().map(toFacadeProjectionLayer));
      } catch {
        // never let instrumentation break the legacy caller
      }
    }
    // Native-authority shadow re-push (gated, default OFF => no-op). A mirrored
    // external transition changes the TS model outside the facade command path, so
    // re-push the full canonical shadow so the native copy stays complete. The
    // commit shim passes the live engine; direct callers may omit it and skip the
    // re-push. Fire-and-forget; a rejected re-push is logged, never surfaced as an
    // unhandled rejection.
    if (engine) {
      void repushCanonicalDocument(docId, engine).catch((e) =>
        console.warn("[canonical-repush] shadow re-push failed", e),
      );
    }
    pendingMarkers = pendingMarkers.filter((m) => m !== marker);
    return { ok: true, seq: res.externalSeq ?? undefined };
  } catch (e) {
    void e;
    setHistoryDegraded({ reason: "UNRECORDED_EXTERNAL_TRANSITION", markers: [...pendingMarkers] });
    return { ok: false };
  }
}

export async function confirmExternalCursor(
  docId: string,
  seq: number,
  direction: "undo" | "redo",
): Promise<{ ok: boolean }> {
  // External-handoff undo/redo: the walker landed on a legacy (external) entry and
  // set the engine's pending-external barrier. This function clears the barrier via
  // the cursor commit; it does NOT re-push the canonical shadow. The native-authority
  // heal re-push is intentionally OUT of this path: it runs AFTER the legacy TS
  // restore completes (useEditorCommands.restoreHistorySnapshot, handoff-fallthrough
  // branch), so the re-pushed payload reflects the post-restore state. Keeping it
  // here would carry the pre-restore snapshot and fire even when the facade branch
  // returns early. Gated: historyDegraded => no-op, production unchanged.
  // Deterministic degraded behavior (ADR 0008 H0 review): fail fast, keep the
  // degraded reason, never appear healthy while cursor/state may diverge.
  const deg = historyDegraded();
  if (deg) return { ok: false };
  const marker: Marker = { kind: "pendingConfirm", seq, direction };
  pendingMarkers.push(marker);
  try {
    const res = await historyCursorCommit(seq, direction, docId);
    pendingMarkers = pendingMarkers.filter((m) => m !== marker);
    syncAuthoritativeVersion(docId, res.documentVersion);
    return { ok: true };
  } catch (e) {
    void e;
    setHistoryDegraded({ reason: "CURSOR_COMMIT_FAILED", markers: [...pendingMarkers] });
    return { ok: false };
  }
}

export async function getHistoryProjection(docId: string): Promise<HistoryQueryResult & {
  docId: string;
  degraded: boolean;
  degradeReason?: string;
  unrecordedTokens: string[];
}> {
  const q = await getHistoryQuery(docId);
  const deg = historyDegraded();
  return {
    ...q,
    degradedHint: q.degradedHint || !!deg,
    degraded: !!deg,
    degradeReason: deg?.reason,
    unrecordedTokens: [...tsPayloadStore.keys()],
    docId,
  };
}

let shimInstalled = false;
export function installFacadeCommitShim(providers: {
  getEngine: () => { getId(): string; getLayers(): Array<{ id: string }> } | null;
  getDocId: () => string;
}): void {
  if (shimInstalled) return;
  shimInstalled = true;
  const proto = CommandHistory.prototype as unknown as {
    commit: (snap: unknown, label?: string) => void;
    recordSnapshotHistory: (
      before: unknown,
      after: unknown,
      label?: string,
    ) => void;
  };

  const originalCommit = proto.commit;
  // Forward ALL arguments — callers may pass a third `imperative` payload
  // (HistoryTilePatches) that the tile-memento undo/redo model requires.
  proto.commit = function (this: unknown, ...args: unknown[]) {
    const [snap, label] = args as [unknown, string | undefined];
    (originalCommit as unknown as (...callArgs: unknown[]) => void).apply(this, args);
    // Feature gate + zero-cost when OFF:
    if (!isFacadeEnabled()) return;
    try {
      const engine = providers.getEngine();
      if (!engine) return;
      // conservative superset — intentionally NOT filtered (H0); do not
      // "optimize" this into an empty set for deletes.
      const affected: string[] = [];
      for (const l of engine.getLayers()) {
        affected.push(l.id);
      }
      // Same docId as the external-cursor handoff (facadeHistoryHandoff) so both
      // seams address one engine; engine.getId() is that shared expression.
      // Register the in-flight mirror so a following facade command can await it
      // (native-authority version-sync barrier; the wasm default path no-ops).
      setExternalTransitionPending(
        engine.getId(),
        recordExternalTransitionFor(
          engine.getId(),
          { label: label ?? "Legacy Edit", affectedLayerIds: affected, snapshot: snap },
          engine as unknown as DocumentEngine,
        ).then(() => {}),
      );
    } catch {
      // never let instrumentation break the legacy caller
    }
  };

  const originalSnapshot = proto.recordSnapshotHistory;
  // Mirror the single-Delete legacy branch into the WASM engine via the SAME
  // External path the commit wrapper uses (reused, no new command/method).
  // A non-owned layer is never in the WASM layer set, so a metadata-only
  // External marker keeps cursor/ordering unified without a native entry.
  // Forward ALL args (before/after/label): the undo-point is the PRE-action
  // `before` state, and that is exactly what the External marker carries. The
  // commit wrapper does the same — it records the commit's pre-action argument,
  // not any derived post-state. Both mirrored payloads are the same undo point.
  proto.recordSnapshotHistory = function (this: unknown, ...args: unknown[]) {
    const [before, after, label] = args as [unknown, unknown, string | undefined];
    (originalSnapshot as unknown as (...callArgs: unknown[]) => void).apply(this, args);
    // Feature gate + zero-cost when OFF:
    if (!isFacadeEnabled()) return;
    try {
      const engine = providers.getEngine();
      if (!engine) return;
      // conservative superset — intentionally NOT filtered (H0); do not
      // "optimize" this into an empty set for deletes.
      const affected: string[] = [];
      for (const l of engine.getLayers()) {
        affected.push(l.id);
      }
      // Route the mirror to the SAME engine the external-cursor handoff
      // (facadeHistoryHandoff) reads/walks: the active engine's id. Both seams
      // use engine.getId() so they address one engine. Register the in-flight
      // mirror for the native-authority version-sync barrier; wasm no-ops.
      setExternalTransitionPending(
        engine.getId(),
        recordExternalTransitionFor(
          engine.getId(),
          { label: label ?? "Legacy Edit", affectedLayerIds: affected, snapshot: before },
          engine as unknown as DocumentEngine,
        ).then(() => {}),
      );
    } catch {
      // never let instrumentation break the legacy caller
    }
  };
}

export function __resetFacadeRegistryForTests(): void {
  // Reset the per-document wasm engine for each doc id this registry created,
  // so engine state does not leak across tests using real doc ids.
  for (const docId of facadeDocIdsForTests()) resetWasmDoc(docId);
  clearFacadeStoreForTests();
  resetFacadeBridgeForTests();
  clearTransformPreview();
  tsPayloadStore.clear();
  numericCommitTailByDoc.clear();
  pendingMarkers = [];
  setHistoryDegraded(null);
}
