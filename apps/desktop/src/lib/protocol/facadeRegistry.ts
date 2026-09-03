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
  getHistoryQuery,
  historyCursorCommit,
  isFacadeEnabled,
} from "./bridge";
import { CONTRACT_VERSION } from "./types";
export { isFacadeEnabled };

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

export function applyFacadePreviews(rs: RenderState): RenderState {
  const tp = transformPreview();
  const op = opacityPreview();
  let layers = rs.layers;
  if (tp) {
    layers = layers.map((l) => (l.id === tp.layerId ? { ...l, transform: tp.transform } : l));
  }
  if (op) {
    layers = layers.map((l) => (l.id === op.layerId ? { ...l, opacity: op.opacity } : l));
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

export function commitFacadeOpacity(
  engine: {
    getId(): string;
    applyFacadeSnapshot(s: unknown): void;
  },
  ids: string[],
  opacity: number,
  facadeOverride?: EditorFacade
): { status: OpacityRouteStatus; count?: number } {
  const route = resolveSelectionRoute(ids);
  if (route.mode === "empty") return { status: "empty" };
  if (route.mode === "mixed-rejected") return { status: "mixed-rejected" };
  if (route.mode !== "facade") return { status: "legacy" };
  const f = facadeOverride ?? getFacade(engine.getId());
  let last: unknown = null;
  for (const id of route.ownedIds) {
    last = f.setOpacity(id, opacity);
  }
  if (last) engine.applyFacadeSnapshot(last);
  return { status: "applied", count: route.ownedIds.length };
}
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

const facadeByDoc = new Map<string, EditorFacade>();

export function getFacade(docId: string): EditorFacade {
  let f = facadeByDoc.get(docId);
  if (!f) {
    f = new EditorFacade();
    facadeByDoc.set(docId, f);
  }
  return f;
}

// ── Transient drag preview ────────────────────────────────────────────────
// Written by useSelectionTransformDrag during a facade drag (pointermove),
// read by EditorShell's render scheduler (renderer model-matrix override)
// and by the selection-overlay memos (handle/HUD tracking). Never persisted:
// commitTransform()'s Rust delta is authoritative and clears it.
export interface FacadeTransformPreview {
  layerId: string;
  transform: Transform2D;
}
const [transformPreview, setTransformPreview] = createSignal<FacadeTransformPreview | null>(null);
export { transformPreview, setTransformPreview };
export function clearTransformPreview(): void {
  setTransformPreview(null);
}

// ── Numeric transform commit (Ticket 2.2 refinement) ─────────────────────
// One committed numeric edit/group = exactly ONE TransformLayer Rust command
// with expectedVersion, then authoritative projection. No persistent TS
// mutation: the TS model only ever receives the Rust RenderDelta snapshot.
export function facadeCommitNumericTransform(
  engine: {
    getId(): string;
    getLayer(id: string): { id: string; locked: boolean; transform: Transform2D } | null | undefined;
    applyFacadeSnapshot(s: unknown): void;
  },
  layerId: string,
  patch: Partial<Transform2D>
): boolean {
  const layer = engine.getLayer(layerId);
  if (!layer || layer.locked) return false;
  const next = { ...layer.transform, ...patch };
  const same =
    next.x === layer.transform.x &&
    next.y === layer.transform.y &&
    next.scaleX === layer.transform.scaleX &&
    next.scaleY === layer.transform.scaleY &&
    next.rotation === layer.transform.rotation &&
    next.flipH === layer.transform.flipH &&
    next.flipV === layer.transform.flipV;
  if (same) return false;
  const f = getFacade(engine.getId());
  f.beginTransform(layerId, { ...layer.transform });
  f.updateTransform(next);
  const snap = f.commitTransform();
  if (snap) engine.applyFacadeSnapshot(snap);
  return true;
}

// ── One-time facade seeding ──────────────────────────────────────────────
// A fresh facade knows nothing about pre-existing TS layers; without seeding,
// its first full-snapshot projection would CLOBBER them (applyFacadeSnapshot
// replaces the model with the facade view). Seed once from the engine's
// current layers as an initial metadata-only snapshot.
export function seedFacadeFromEngine(
  engine: {
    getId(): string;
    getLayers(): Array<{
      id: string;
      name: string;
      visible: boolean;
      opacity: number;
      transform: { x: number; y: number; scaleX: number; scaleY: number; rotation: number };
    }>;
  },
  facade: EditorFacade
): void {
  if (facade.snapshot.layers.length === 0 && engine.getLayers().length > 0) {
    facade.seedSnapshot({
      version: 0,
      layers: engine.getLayers().map((l) => ({
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
    } as never);
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

function peekFacade(docId: string): EditorFacade | undefined {
  return facadeByDoc.get(docId);
}

function syncAuthoritativeVersion(docId: string, dv: number): void {
  const f = facadeByDoc.get(docId);
  if (f) f.syncRenderedVersionTo(dv);
}

export function recordExternalTransitionFor(
  docId: string,
  rec: { label: string; affectedLayerIds: string[]; snapshot: unknown }
): { ok: boolean; seq?: number } {
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
  try {
    const res = applyCommand({
      contractVersion: CONTRACT_VERSION,
      expectedVersion: undefined, // mirrors may land after facade ops; engine DV is authority
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
    pendingMarkers = pendingMarkers.filter((m) => m !== marker);
    return { ok: true, seq: res.externalSeq ?? undefined };
  } catch (e) {
    void e;
    setHistoryDegraded({ reason: "UNRECORDED_EXTERNAL_TRANSITION", markers: [...pendingMarkers] });
    return { ok: false };
  }
}

export function confirmExternalCursor(
  docId: string,
  seq: number,
  direction: "undo" | "redo"
): { ok: boolean } {
  // Deterministic degraded behavior (ADR 0008 H0 review): fail fast, keep the
  // degraded reason, never appear healthy while cursor/state may diverge.
  const deg = historyDegraded();
  if (deg) return { ok: false };
  const marker: Marker = { kind: "pendingConfirm", seq, direction };
  pendingMarkers.push(marker);
  try {
    const res = historyCursorCommit(seq, direction);
    pendingMarkers = pendingMarkers.filter((m) => m !== marker);
    syncAuthoritativeVersion(docId, res.documentVersion);
    return { ok: true };
  } catch (e) {
    void e;
    setHistoryDegraded({ reason: "CURSOR_COMMIT_FAILED", markers: [...pendingMarkers] });
    return { ok: false };
  }
}

export function getHistoryProjection(docId: string): HistoryQueryResult & {
  docId: string;
  degraded: boolean;
  degradeReason?: string;
  unrecordedTokens: string[];
} {
  const q = getHistoryQuery();
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
  };
  const original = proto.commit;
  // Forward ALL arguments — callers may pass a third `imperative` payload
  // (HistoryTilePatches) that the tile-memento undo/redo model requires.
  proto.commit = function (this: unknown, ...args: unknown[]) {
    const [snap, label] = args as [unknown, string | undefined];
    (original as unknown as (...callArgs: unknown[]) => void).apply(this, args);
    // Feature gate + zero-cost when OFF:
    if (!isFacadeEnabled()) return;
    try {
      const engine = providers.getEngine();
      if (!engine) return;
      const affected: string[] = [];
      const preIds = new Set(
        ((snap as { layers?: Array<{ id: string }> })?.layers ?? []).map((l) => l.id),
      );
      for (const l of engine.getLayers()) {
        if (!preIds.has(l.id) || preIds.has(l.id)) affected.push(l.id); // H0: conservative superset
      }
      recordExternalTransitionFor(providers.getDocId() || engine.getId(), {
        label: label ?? "Legacy Edit",
        affectedLayerIds: affected,
        snapshot: snap,
      });
    } catch {
      // never let instrumentation break the legacy caller
    }
  };
}

export function __resetFacadeRegistryForTests(): void {
  facadeByDoc.clear();
  resetFacadeBridgeForTests();
  clearTransformPreview();
  tsPayloadStore.clear();
  pendingMarkers = [];
  setHistoryDegraded(null);
}
