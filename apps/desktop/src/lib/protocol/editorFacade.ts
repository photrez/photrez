// EditorFacade — Ticket 2. No persistent TS document state.
// Holds only: renderedVersion + snapshot cache (read-only projection) + transient interaction.
// All persistent mutations go via Command -> Rust -> delta. Correctness via expectedVersion + baseVersion.

import { applyCommand, flushExternalTransitions, getSnapshot, getVersion, isNativeAuthority } from "./bridge";
import { repushCanonicalDocument } from "./canonicalSeed";
import { CONTRACT_VERSION } from "./types";
import type { DocumentEngine } from "@/engine/document";
import type { Command, DocumentVersion, RenderSnapshot, RenderDelta, RenderLayer, TransformPatch, LockKind } from "./types";
import { isDeltaApplicable } from "./types";

export type TransientTransform = { id: string; start: TransformPatch; live: TransformPatch } | null;

export class EditorFacade {
  renderedVersion: DocumentVersion = 0;
  snapshot: RenderSnapshot = { version: 0, layers: [] };

  transientTransform: TransientTransform = null;

  private pending = new Map<number, DocumentVersion>();
  // Ticket 2.2: set by undo()/redo() — true when Rust had no entry (no-op).
  lastHistoryDeltaWasEmpty = false;
  // Native-authority shadow re-push: the TS engine bound during
  // seedFacadeFromEngine so addLayer can re-push the full canonical document
  // after Rust mints a layer (Rust cannot populate the canonical-only fields).
  // Null unless native authority is active; the re-push is itself gated, so an
  // unset engine is a no-op and production (default authority) is unchanged.
  private engine: DocumentEngine | null = null;
  bindEngine(engine: DocumentEngine): void {
    this.engine = engine;
  }
  // External history handoff (ADR 0008 H0): set by undo()/redo() when the
  // walker lands on a legacy (external) entry and returns status:"external".
  // The host must clear the engine's pending-external barrier (via
  // confirmExternalCursor) before issuing another facade command, or every
  // subsequent facade command permanently rejects with E_EXTERNAL_PENDING.
  lastExternalHandoff: { seq: number; direction: "undo" | "redo" } | null = null;
  private nextSeq = 1;

  constructor(initial?: RenderSnapshot, readonly docId = "default") {
    if (initial) {
      this.snapshot = initial;
      this.renderedVersion = initial.version;
    }
  }

  applyDelta(delta: RenderDelta): boolean {
    if (!isDeltaApplicable(delta, this.renderedVersion)) return false;
    this.snapshot = applyDeltaToSnapshot(this.snapshot, delta);
    this.snapshot.version = delta.version;
    this.renderedVersion = delta.version;
    return true;
  }

  applySnapshot(snap: RenderSnapshot): boolean {
    if (snap.version <= this.renderedVersion) return false;
    this.snapshot = snap;
    this.renderedVersion = snap.version;
    return true;
  }
  // Ticket 2.2: one-time initial seeding from the TS engine view. Unlike
  // applySnapshot this bypasses the monotonic-version guard — a fresh facade
  // sits at version 0 and the engine seed is ALSO version 0, which
  // applySnapshot would silently reject (latent bug surfaced by the
  // mixed-history tests). Seeding is metadata-only and happens before any
  // Rust command exists for this document.
  seedSnapshot(snap: RenderSnapshot): void {
    this.snapshot = snap;
    this.renderedVersion = snap.version;
  }

  // Invariant: facade commands must be awaited before issuing another in a
  // single logical step. expectedVersion reads this.renderedVersion, which is
  // bumped inside a post-await microtask (applyDelta). A second command issued
  // in the same synchronous task would read a stale expectedVersion and catch a
  // spurious version-mismatch rejection.
  async addLayer(name: string, width = 100, height = 100, index = 0): Promise<RenderSnapshot> {
    // Host-owned identity: mint the layer id in TS (layer-<rand>) so it matches
    // the TS engine's id space; the native AddLayer arm no longer mints its own.
    const id = `layer-${Math.random().toString(36).slice(2, 10)}`;
    const command: Command = { type: "addLayer", id, name, width, height, index };
    await this.syncFromEngine();
    const res = await applyCommand({
      contractVersion: CONTRACT_VERSION,
      expectedVersion: this.renderedVersion,
      docId: this.docId,
      command,
    });
    this.pending.set(this.nextSeq++, res.delta.baseVersion);
    if (!this.applyDelta(res.delta)) {
      await this.refreshSnapshot();
    } else if (isNativeAuthority()) {
      // NATIVE only: the Rust engine inserts at the clamped host index while
      // applyDelta tail-appends, so reposition locally to keep the projection
      // order equal to the engine order (and the canonical re-push below
      // faithful to it) WITHOUT any snapshot re-read. Clamp mirrors the
      // engine's insert_at: max(0, min(index, length-before-insert)), matching
      // bridge_emu and document_core_apply AddLayer. Under wasm authority the
      // facade keeps its historical tail-append (the mirror is retired with
      // the wasm path; no native consumers exist there).
      this.repositionAppended(id, index);
    }
    this.repushCanonicalAfterAddLayer();
    return this.snapshot;
  }

  // Move a just-appended layer id to the engine-equivalent insert position.
  // Same clamp the engines use at insertion time, applied to the pre-insert
  // vector (which is what remains after removing the appended id).
  private repositionAppended(id: string, index: number): void {
    const layers = this.snapshot.layers;
    const at = layers.findIndex((l) => l.id === id);
    if (at < 0) return;
    const [layer] = layers.splice(at, 1);
    const target = Math.max(0, Math.min(index, layers.length));
    layers.splice(target, 0, layer);
  }

  async deleteLayer(id: string): Promise<RenderSnapshot> {
    await this.syncFromEngine();
    const res = await applyCommand({ contractVersion: CONTRACT_VERSION, expectedVersion: this.renderedVersion, docId: this.docId, command: { type: "deleteLayer", id } });
    this.pending.set(this.nextSeq++, res.delta.baseVersion);
    if (!this.applyDelta(res.delta)) await this.refreshSnapshot();
    return this.snapshot;
  }

  beginTransform(id: string, start: TransformPatch): void {
    this.transientTransform = { id, start, live: { ...start } };
  }
  updateTransform(live: TransformPatch): void {
    if (!this.transientTransform) return;
    this.transientTransform.live = { ...live };
  }
  async commitTransform(): Promise<RenderSnapshot | null> {
    if (!this.transientTransform) return null;
    const { id, live } = this.transientTransform;
    this.transientTransform = null;
    await this.syncFromEngine();
    const res = await applyCommand({
      contractVersion: CONTRACT_VERSION,
      expectedVersion: this.renderedVersion,
      docId: this.docId,
      command: { type: "transformLayer", id, transform: live },
    });
    this.pending.set(this.nextSeq++, res.delta.baseVersion);
    if (!this.applyDelta(res.delta)) await this.refreshSnapshot();
    return this.snapshot;
  }
  cancelTransform(): void { this.transientTransform = null; }
  // Test/introspection helper: true while a transient drag session exists.
  transientTransformActive(): boolean { return this.transientTransform !== null; }

  async setOpacity(id: string, opacity: number): Promise<RenderSnapshot> {
    await this.syncFromEngine();
    const res = await applyCommand({ contractVersion: CONTRACT_VERSION, expectedVersion: this.renderedVersion, docId: this.docId, command: { type: "setOpacity", id, opacity } });
    this.pending.set(this.nextSeq++, res.delta.baseVersion);
    if (!this.applyDelta(res.delta)) await this.refreshSnapshot();
    return this.snapshot;
  }

  // Metadata command arms — mirror setOpacity exactly: expectedVersion-enforced
  // envelope + applyDelta, with refreshSnapshot fallback on an inapplicable delta.
  async setLayerVisibility(id: string, visible: boolean): Promise<RenderSnapshot> {
    await this.syncFromEngine();
    const res = await applyCommand({ contractVersion: CONTRACT_VERSION, expectedVersion: this.renderedVersion, docId: this.docId, command: { type: "setVisible", id, visible } });
    this.pending.set(this.nextSeq++, res.delta.baseVersion);
    if (!this.applyDelta(res.delta)) await this.refreshSnapshot();
    return this.snapshot;
  }

  async setLayerName(id: string, name: string): Promise<RenderSnapshot> {
    await this.syncFromEngine();
    const res = await applyCommand({ contractVersion: CONTRACT_VERSION, expectedVersion: this.renderedVersion, docId: this.docId, command: { type: "rename", id, name } });
    this.pending.set(this.nextSeq++, res.delta.baseVersion);
    if (!this.applyDelta(res.delta)) await this.refreshSnapshot();
    return this.snapshot;
  }

  async setLayerLocked(id: string, kind: LockKind, locked: boolean): Promise<RenderSnapshot> {
    await this.syncFromEngine();
    const res = await applyCommand({ contractVersion: CONTRACT_VERSION, expectedVersion: this.renderedVersion, docId: this.docId, command: { type: "setLocked", id, kind, locked } });
    this.pending.set(this.nextSeq++, res.delta.baseVersion);
    if (!this.applyDelta(res.delta)) await this.refreshSnapshot();
    return this.snapshot;
  }

  async setLayerBlendMode(id: string, mode: string): Promise<RenderSnapshot> {
    await this.syncFromEngine();
    const res = await applyCommand({ contractVersion: CONTRACT_VERSION, expectedVersion: this.renderedVersion, docId: this.docId, command: { type: "setBlendMode", id, mode } });
    this.pending.set(this.nextSeq++, res.delta.baseVersion);
    if (!this.applyDelta(res.delta)) await this.refreshSnapshot();
    return this.snapshot;
  }

  // Structural command arm: the Reorder arm emits an ordered FULL RESTATEMENT
  // of every layer (and the history diff restates order on undo/redo of a
  // pure move), so applyDeltaToSnapshot's full-restatement branch rebuilds the
  // sequence from the delta alone - no snapshot re-read, and the same
  // delta-only shape every other command method uses.
  async reorderLayer(id: string, to: number): Promise<RenderSnapshot> {
    // Delta path by design; the ordered full restatement the Reorder arm emits
    // carries the new order, which applyDeltaToSnapshot adopts (see its
    // full-restatement branch). No snapshot re-read, no refresh read.
    await this.syncFromEngine();
    const res = await applyCommand({ contractVersion: CONTRACT_VERSION, expectedVersion: this.renderedVersion, docId: this.docId, command: { type: "reorder", id, to } });
    this.pending.set(this.nextSeq++, res.delta.baseVersion);
    if (!this.applyDelta(res.delta)) await this.refreshSnapshot();
    return this.snapshot;
  }

  // Structural command arms: the native engine owns the graph mutation
  // (identity/layer-set restatement); pixel compositing stays host-side. Each
  // method mirrors deleteLayer EXACTLY — syncFromEngine -> applyCommand ->
  // pending.set -> applyDelta -> return snapshot — with NO snapshot re-read
  // beyond applyDelta's fallback. The structural arms emit an ordered full
  // restatement (victims removed, merged/clone appears at the engine position
  // with a NULL bitmap), which applyDeltaToSnapshot adopts (see its
  // full-restatement branch). The caller composites the pixels host-side and
  // attaches them via setLayerImageBitmap.
  async duplicateLayer(id: string, newId: string): Promise<RenderSnapshot> {
    await this.syncFromEngine();
    const res = await applyCommand({ contractVersion: CONTRACT_VERSION, expectedVersion: this.renderedVersion, docId: this.docId, command: { type: "duplicateLayer", id, newId } });
    this.pending.set(this.nextSeq++, res.delta.baseVersion);
    if (!this.applyDelta(res.delta)) await this.refreshSnapshot();
    return this.snapshot;
  }

  async mergeDown(id: string, mergedId: string): Promise<RenderSnapshot> {
    await this.syncFromEngine();
    const res = await applyCommand({ contractVersion: CONTRACT_VERSION, expectedVersion: this.renderedVersion, docId: this.docId, command: { type: "mergeDown", id, mergedId } });
    this.pending.set(this.nextSeq++, res.delta.baseVersion);
    if (!this.applyDelta(res.delta)) await this.refreshSnapshot();
    return this.snapshot;
  }

  async mergeSelectedLayers(ids: string[], mergedId: string): Promise<RenderSnapshot> {
    await this.syncFromEngine();
    const res = await applyCommand({ contractVersion: CONTRACT_VERSION, expectedVersion: this.renderedVersion, docId: this.docId, command: { type: "mergeSelected", ids, mergedId } });
    this.pending.set(this.nextSeq++, res.delta.baseVersion);
    if (!this.applyDelta(res.delta)) await this.refreshSnapshot();
    return this.snapshot;
  }

  async flattenLayers(mergedId: string): Promise<RenderSnapshot> {
    await this.syncFromEngine();
    const res = await applyCommand({ contractVersion: CONTRACT_VERSION, expectedVersion: this.renderedVersion, docId: this.docId, command: { type: "flatten", mergedId } });
    this.pending.set(this.nextSeq++, res.delta.baseVersion);
    if (!this.applyDelta(res.delta)) await this.refreshSnapshot();
    return this.snapshot;
  }

  async rasterizeLayer(id: string): Promise<RenderSnapshot> {
    await this.syncFromEngine();
    const res = await applyCommand({ contractVersion: CONTRACT_VERSION, expectedVersion: this.renderedVersion, docId: this.docId, command: { type: "rasterizeLayer", id } });
    this.pending.set(this.nextSeq++, res.delta.baseVersion);
    if (!this.applyDelta(res.delta)) await this.refreshSnapshot();
    return this.snapshot;
  }

  async undo(): Promise<RenderSnapshot> {
    this.lastExternalHandoff = null;
    await this.syncFromEngine();
    const res = await applyCommand({ contractVersion: CONTRACT_VERSION, expectedVersion: this.renderedVersion, docId: this.docId, command: { type: "undo" } });
    // External history handoff: the walker landed on a legacy (external) entry
    // and set the engine's pending-external barrier. Surface the handoff so the
    // production path can clear the barrier via confirmExternalCursor.
    if (res.status === "external" && res.externalSeq !== undefined) {
      this.lastExternalHandoff = { seq: res.externalSeq, direction: "undo" };
    }
    // Ticket 2.2 mixed-history routing: Rust Undo on an empty stack is a NO-OP
    // success (empty delta, version still bumps). Callers must treat this flag
    // as "Rust had nothing" and fall through to the legacy TS history store.
    this.lastHistoryDeltaWasEmpty = res.delta.changes.length === 0;
    this.pending.set(this.nextSeq++, res.delta.baseVersion);
    // Delta-only projection by design: a full-snapshot re-read would drop layers
    // the engine never learned (unrouted structural mutations live in TS only).
    // Layer ORDER from a routed reorder arrives through the delta itself - the
    // Reorder arm emits an ordered full restatement the consumer applies (see
    // applyDeltaToSnapshot). Undo bumps the version; the delta bookkeeping keeps
    // renderedVersion aligned with it (pending.set above + applyDelta).
    if (!this.applyDelta(res.delta)) await this.refreshSnapshot();
    return this.snapshot;
  }
  async redo(): Promise<RenderSnapshot> {
    this.lastExternalHandoff = null;
    await this.syncFromEngine();
    const res = await applyCommand({ contractVersion: CONTRACT_VERSION, expectedVersion: this.renderedVersion, docId: this.docId, command: { type: "redo" } });
    if (res.status === "external" && res.externalSeq !== undefined) {
      this.lastExternalHandoff = { seq: res.externalSeq, direction: "redo" };
    }
    this.lastHistoryDeltaWasEmpty = res.delta.changes.length === 0;
    this.pending.set(this.nextSeq++, res.delta.baseVersion);
    if (!this.applyDelta(res.delta)) await this.refreshSnapshot();
    return this.snapshot;
  }

  private async refreshSnapshot(): Promise<void> {
    try { const snap = await getSnapshot(this.docId); this.applySnapshot(snap); } catch {}
  }

  // Native-authority shadow re-push (gated, default OFF => no-op). After a
  // successful addLayer the native engine has minted the layer but lacks its
  // canonical-only fields, and the TS engine has not yet received the layer
  // (the production caller projects it only after this returns). Project the
  // facade snapshot into the engine first, then re-push the full canonical
  // shadow so the native copy is complete. Fire-and-forget: a rejected re-push
  // is logged, never surfaced as an unhandled rejection.
  private repushCanonicalAfterAddLayer(): void {
    if (!isNativeAuthority() || !this.engine) return;
    try {
      this.engine.applyFacadeSnapshot(this.snapshot);
    } catch {
      // Projection is auxiliary to the command's success; never fail addLayer on it.
    }
    void repushCanonicalDocument(this.docId, this.engine).catch((e) =>
      console.warn("[canonical-repush] shadow re-push failed", e),
    );
  }

  // ADR 0014 native-authority version sync: under native authority the legacy
  // history mirror bumps the engine documentVersion outside this facade's command
  // envelope and is fire-and-forget. Await any in-flight mirror, then read the
  // authoritative engine version, so renderedVersion matches the native engine
  // before we build expectedVersion. This closes the fill -> setOpacity interleave
  // that rejected with E_VERSION_MISMATCH. The wasm default path no-ops here, so
  // behavior is unchanged. The native engine remains the single version owner.
  private async syncFromEngine(): Promise<void> {
    if (!isNativeAuthority()) return;
    await flushExternalTransitions(this.docId);
    try {
      const v = await getVersion(this.docId);
      this.syncRenderedVersionTo(v);
    } catch {}
  }

  // ADR 0008 C2: external records advance the authoritative DocumentVersion
  // outside this facade's own commands. Call after RecordExternalTransition
  // succeeds so subsequent expectedVersion checks stay correct.
  syncRenderedVersionTo(dv: number): void {
    if (dv > this.renderedVersion) this.renderedVersion = dv;
  }

  hasPersistentLayersField(): boolean {
    return (this as unknown as Record<string, unknown>).layers !== undefined;
  }
}

export function applyDeltaToSnapshot(snap: RenderSnapshot, delta: RenderDelta): RenderSnapshot {
  // TWO-PASS order-aware consumer.
  //
  // Pass 1: apply every Remove against a copy. This drops removed ids (delete,
  // merge-down source) BEFORE we decide whether the upserts restate order - so a
  // Remove-bearing delta is treated on its remaining set, not the pre-remove one.
  const layers = [...snap.layers];
  for (const ch of delta.changes) {
    if (ch.kind === "remove") {
      const idx = layers.findIndex((l) => l.id === ch.id);
      if (idx >= 0) layers.splice(idx, 1);
    }
  }
  // Pass 2: if the upserts cover the entire remaining id set (every surviving
  // layer is restated, so membership is either unchanged or a deleted layer
  // reappears), adopt the upsert SEQUENCE as the new order. The walker emits an
  // ordered Upsert of EVERY layer for undo/redo and the structural arms emit
  // ordered-full restatements, so this reconstructs the exact snapshot order
  // from the delta alone - no snapshot re-read, and a restored/merged layer
  // lands at its SNAPSHOT position instead of the stack bottom.
  const restated = delta.changes
    .filter((c): c is { kind: "upsert"; layer: RenderLayer } => c.kind === "upsert")
    .map((c) => c.layer);
  const restatedIds = new Set(restated.map((l) => l.id));
  const adoptsSequence = restated.length > 0 && layers.every((l) => restatedIds.has(l.id));
  if (adoptsSequence) {
    // Carry canvas dims + selection forward (see below).
    return { version: delta.version, layers: restated, width: snap.width, height: snap.height, selection: snap.selection };
  }
  // Fallback: in-place upsert (replace by id) + append unknown ids. Covers
  // count-changing-but-not-restatement deltas such as a single addLayer upsert
  // (which must append, not reorder).
  for (const ch of delta.changes) {
    if (ch.kind === "upsert") {
      const idx = layers.findIndex((l) => l.id === ch.layer.id);
      if (idx >= 0) layers[idx] = ch.layer;
      else layers.push(ch.layer);
    }
  }
  // Carry canvas dims + selection forward: ResizeCanvas/CropCanvas/ApplyCrop emit
  // an empty (or layer-only) delta but change the document size, which rides the
  // snapshot's width/height. Selection is engine-local UI state on the snapshot.
  return { version: delta.version, layers, width: snap.width, height: snap.height, selection: snap.selection };
}
