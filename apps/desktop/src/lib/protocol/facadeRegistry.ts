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

export async function commitFacadeOpacity(
  engine: {
    getId(): string;
    applyFacadeSnapshot(s: unknown): void;
  },
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

// Normalize an empty doc id to the reserved "default" key. The native-authority
// engine (bridge/native client) resolves "" -> "default"; on that path the facade
// registry must agree so a facade keyed by the native engine's id meets the bridge.
// On the default (wasm) path this normalization is deliberately NOT applied:
// getFacade/removeFacade use the raw doc id so the wasm default path is
// byte-identical to before the native-authority reroute.
function resolveFacadeDocKey(docId: string): string {
  return docId === "" ? "default" : docId;
}

// Resolve the facade map key for the active authority. On the native-authority
// path empty ids normalize to "default" (matching the native engine); on the
// default wasm path the raw doc id is used unchanged (byte-identical behavior).
function facadeKey(docId: string): string {
  return isNativeAuthority() ? resolveFacadeDocKey(docId) : docId;
}

export function getFacade(docId: string): EditorFacade {
  const key = facadeKey(docId);
  let f = facadeByDoc.get(key);
  if (!f) {
    f = new EditorFacade(undefined, key);
    facadeByDoc.set(key, f);
  }
  return f;
}

// Evict a doc's facade (called on document close, alongside clearNativeSeed) so a
// reopened doc id gets a FRESH facade seeded from the freshly-reseeded native
// engine, never a stale one. Gated callers (WorkspaceManager.removeDocument)
// decide when this runs; the registry itself stays authority-agnostic.
export function removeFacade(docId: string): void {
  facadeByDoc.delete(facadeKey(docId));
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
export async function facadeCommitNumericTransform(
  engine: {
    getId(): string;
    getLayer(id: string): { id: string; locked: boolean; transform: Transform2D } | null | undefined;
    applyFacadeSnapshot(s: unknown): void;
  },
  layerId: string,
  patch: Partial<Transform2D>
): Promise<boolean> {
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
  const snap = await f.commitTransform();
  if (snap) engine.applyFacadeSnapshot(snap);
  return true;
}

// ── One-time facade seeding ──────────────────────────────────────────────
// A fresh facade knows nothing about pre-existing TS layers; without seeding,
// its first full-snapshot projection would CLOBBER them (applyFacadeSnapshot
// replaces the model with the facade view). Seed once from the engine's
// current layers as an initial metadata-only snapshot.
export async function seedFacadeFromEngine(
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
): Promise<void> {
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

function peekFacade(docId: string): EditorFacade | undefined {
  return facadeByDoc.get(facadeKey(docId));
}

function syncAuthoritativeVersion(docId: string, dv: number): void {
  const f = facadeByDoc.get(facadeKey(docId));
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
  const f = facadeByDoc.get(facadeKey(docId));
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
  for (const docId of facadeByDoc.keys()) resetWasmDoc(docId);
  facadeByDoc.clear();
  resetFacadeBridgeForTests();
  clearTransformPreview();
  tsPayloadStore.clear();
  pendingMarkers = [];
  setHistoryDegraded(null);
}
