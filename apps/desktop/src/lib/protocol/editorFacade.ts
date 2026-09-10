// EditorFacade — Ticket 2. No persistent TS document state.
// Holds only: renderedVersion + snapshot cache (read-only projection) + transient interaction.
// All persistent mutations go via Command -> Rust -> delta. Correctness via expectedVersion + baseVersion.

import { applyCommand, flushExternalTransitions, getSnapshot, getVersion, isNativeAuthority } from "./bridge";
import { repushCanonicalDocument } from "./canonicalSeed";
import { CONTRACT_VERSION } from "./types";
import type { DocumentEngine } from "@/engine/document";
import type { Command, DocumentVersion, RenderSnapshot, RenderDelta, TransformPatch, LockKind } from "./types";
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
    if (isNativeAuthority()) {
      // Native-authority path: the Rust engine mints the layer at its OWN host
      // index, so the bridged delta order can disagree with TS tail-append. Issue
      // the command via the authoritative-snapshot path (re-reads the full
      // snapshot) so the facade PROJECTION matches native order, then re-push the
      // canonical (native-order) state. This kills the projection-order artifact at
      // its source for routed ops: the re-push carries the engine's real layer
      // order, not the delta's tail order, so the canonical payload never diverges
      // from native order.
      await this.applyCommandWithRefresh(command);
      this.repushCanonicalAfterAddLayer();
      return this.snapshot;
    }
    await this.syncFromEngine();
    const res = await applyCommand({
      contractVersion: CONTRACT_VERSION,
      expectedVersion: this.renderedVersion,
      docId: this.docId,
      command,
    });
    this.pending.set(this.nextSeq++, res.delta.baseVersion);
    if (!this.applyDelta(res.delta)) await this.refreshSnapshot();
    this.repushCanonicalAfterAddLayer();
    return this.snapshot;
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

  // Refresh-based command path. For commands whose Rust arms emit a FULL ORDERED
  // restatement of all layers plus Removes (the structural/canvas arms — see
  // crates/core/src/document_core_structural.rs), the delta cannot be reconstructed
  // by the production consumer. This issues the command once and then UNCONDITIONALLY
  // re-reads the authoritative snapshot, so the layer ORDER and canvas DIMS come
  // from the engine rather than being carried forward from previous local state.
  // The method is authority-agnostic: the bridge selects the engine (wasm or native).
  // On applyCommand rejection it propagates the error and performs NO refresh, so the
  // host never clobbers local state with a stale snapshot after a failed command.
  async applyCommandWithRefresh(command: Command): Promise<RenderSnapshot> {
    await this.syncFromEngine();
    const res = await applyCommand({
      contractVersion: CONTRACT_VERSION,
      expectedVersion: this.renderedVersion,
      docId: this.docId,
      command,
    });
    this.pending.set(this.nextSeq++, res.delta.baseVersion);
    // Refresh the full authoritative snapshot directly (do NOT swallow failures).
    // A swallowed getSnapshot error would leave the local snapshot at its pre-command
    // state while reporting the command as success — silent staleness. On rejection,
    // throw a distinct REFRESH_FAILED error so callers see "command applied, refresh
    // failed" instead of a stale-success.
    try {
      const snap = await getSnapshot(this.docId);
      this.applySnapshot(snap);
    } catch (e) {
      throw new Error("REFRESH_FAILED:" + (e instanceof Error ? e.message : String(e)));
    }
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

function applyDeltaToSnapshot(snap: RenderSnapshot, delta: RenderDelta): RenderSnapshot {
  const layers = [...snap.layers];
  for (const ch of delta.changes) {
    if (ch.kind === "upsert") {
      const idx = layers.findIndex((l) => l.id === ch.layer.id);
      if (idx >= 0) layers[idx] = ch.layer;
      else layers.push(ch.layer);
    } else if (ch.kind === "remove") {
      const idx = layers.findIndex((l) => l.id === ch.id);
      if (idx >= 0) layers.splice(idx, 1);
    }
  }
  // Carry canvas dims + selection forward: ResizeCanvas/CropCanvas/ApplyCrop emit
  // an empty (or layer-only) delta but change the document size, which rides the
  // snapshot's width/height. Selection is engine-local UI state on the snapshot.
  return { version: delta.version, layers, width: snap.width, height: snap.height, selection: snap.selection };
}
