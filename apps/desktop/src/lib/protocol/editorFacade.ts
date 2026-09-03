// EditorFacade — Ticket 2. No persistent TS document state.
// Holds only: renderedVersion + snapshot cache (read-only projection) + transient interaction.
// All persistent mutations go via Command -> Rust -> delta. Correctness via expectedVersion + baseVersion.

import { applyCommand, getSnapshot } from "./bridge";
import { CONTRACT_VERSION } from "./types";
import type { DocumentVersion, RenderSnapshot, RenderDelta, TransformPatch } from "./types";
import { isDeltaApplicable } from "./types";

export type TransientTransform = { id: string; start: TransformPatch; live: TransformPatch } | null;
export type TransientStroke = { layerId: string; points: { x: number; y: number; pressure: number }[]; settings: { size: number; hardness: number; opacity: number; flow: number } } | null;

export class EditorFacade {
  renderedVersion: DocumentVersion = 0;
  snapshot: RenderSnapshot = { version: 0, layers: [] };

  transientTransform: TransientTransform = null;
  transientStroke: TransientStroke = null;

  private pending = new Map<number, DocumentVersion>();
  // Ticket 2.2: set by undo()/redo() — true when Rust had no entry (no-op).
  lastHistoryDeltaWasEmpty = false;
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

  addLayer(name: string): RenderSnapshot {
    const res = applyCommand({ contractVersion: CONTRACT_VERSION, expectedVersion: this.renderedVersion, docId: this.docId, command: { type: "addLayer", name } });
    this.pending.set(this.nextSeq++, res.delta.baseVersion);
    if (!this.applyDelta(res.delta)) this.refreshSnapshot();
    return this.snapshot;
  }

  deleteLayer(id: string): RenderSnapshot {
    const res = applyCommand({ contractVersion: CONTRACT_VERSION, expectedVersion: this.renderedVersion, docId: this.docId, command: { type: "deleteLayer", id } });
    this.pending.set(this.nextSeq++, res.delta.baseVersion);
    if (!this.applyDelta(res.delta)) this.refreshSnapshot();
    return this.snapshot;
  }

  beginTransform(id: string, start: TransformPatch): void {
    this.transientTransform = { id, start, live: { ...start } };
  }
  updateTransform(live: TransformPatch): void {
    if (!this.transientTransform) return;
    this.transientTransform.live = { ...live };
  }
  commitTransform(): RenderSnapshot | null {
    if (!this.transientTransform) return null;
    const { id, live } = this.transientTransform;
    this.transientTransform = null;
    const res = applyCommand({
      contractVersion: CONTRACT_VERSION,
      expectedVersion: this.renderedVersion,
      docId: this.docId,
      command: { type: "transformLayer", id, transform: live },
    });
    this.pending.set(this.nextSeq++, res.delta.baseVersion);
    if (!this.applyDelta(res.delta)) this.refreshSnapshot();
    return this.snapshot;
  }
  cancelTransform(): void { this.transientTransform = null; }
  // Test/introspection helper: true while a transient drag session exists.
  transientTransformActive(): boolean { return this.transientTransform !== null; }

  setOpacity(id: string, opacity: number): RenderSnapshot {
    const res = applyCommand({ contractVersion: CONTRACT_VERSION, expectedVersion: this.renderedVersion, docId: this.docId, command: { type: "setOpacity", id, opacity } });
    this.pending.set(this.nextSeq++, res.delta.baseVersion);
    if (!this.applyDelta(res.delta)) this.refreshSnapshot();
    return this.snapshot;
  }

  beginStroke(layerId: string, settings = { size: 20, hardness: 0.5, opacity: 1, flow: 1 }): void {
    this.transientStroke = { layerId, points: [], settings };
  }
  addStrokePoint(p: { x: number; y: number; pressure: number }): void {
    if (!this.transientStroke) return;
    this.transientStroke.points.push(p);
  }
  commitStroke(): RenderSnapshot | null {
    if (!this.transientStroke) return null;
    const { layerId, points, settings } = this.transientStroke;
    this.transientStroke = null;
    if (points.length === 0) return this.snapshot;
    const res = applyCommand({
      contractVersion: CONTRACT_VERSION,
      expectedVersion: this.renderedVersion,
      docId: this.docId,
      command: { type: "brushStroke", layerId, points, settings },
    });
    this.pending.set(this.nextSeq++, res.delta.baseVersion);
    if (!this.applyDelta(res.delta)) this.refreshSnapshot();
    return this.snapshot;
  }
  cancelStroke(): void { this.transientStroke = null; }

  undo(): RenderSnapshot {
    const res = applyCommand({ contractVersion: CONTRACT_VERSION, expectedVersion: this.renderedVersion, docId: this.docId, command: { type: "undo" } });
    // Ticket 2.2 mixed-history routing: Rust Undo on an empty stack is a NO-OP
    // success (empty delta, version still bumps). Callers must treat this flag
    // as "Rust had nothing" and fall through to the legacy TS history store.
    this.lastHistoryDeltaWasEmpty = res.delta.changes.length === 0;
    this.pending.set(this.nextSeq++, res.delta.baseVersion);
    if (!this.applyDelta(res.delta)) this.refreshSnapshot();
    return this.snapshot;
  }
  redo(): RenderSnapshot {
    const res = applyCommand({ contractVersion: CONTRACT_VERSION, expectedVersion: this.renderedVersion, docId: this.docId, command: { type: "redo" } });
    this.lastHistoryDeltaWasEmpty = res.delta.changes.length === 0;
    this.pending.set(this.nextSeq++, res.delta.baseVersion);
    if (!this.applyDelta(res.delta)) this.refreshSnapshot();
    return this.snapshot;
  }

  private refreshSnapshot(): void {
    try { const snap = getSnapshot(this.docId); this.applySnapshot(snap); } catch {}
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
  return { version: delta.version, layers };
}
