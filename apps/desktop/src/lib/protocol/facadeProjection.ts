// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Facade projection snapshot maintenance.
//
// The projection snapshot is the layer vector the next routed op rebuilds the TS
// model from (DocumentEngine.applyFacadeSnapshot replaces model.layers with the
// snapshot's vector), so it must stay in step with the TS model after every
// legacy mutation. Two seams keep it current:
//   - the post-mutation choke point (DocumentEngine.notifyChange), which covers
//     the dominant mutation path, per engine, and
//   - the commit-shim external-transition record (facadeRegistry), a redundant-
//     by-design second net for a future mutation path that bypasses the choke
//     point. No current test fails if it is removed; it is kept because it is
//     cheap and guards that future path.
// Both project the SAME fields through toFacadeProjectionLayer; this module is
// the single owner of that mapping and is imported by both. It lives apart from
// facadeRegistry so the engine can refresh without importing the registry
// (which imports the engine - a back-import would be a cycle).
//
// The flag gate (photrez.facade) lives in the refresh entry points
// (refreshFacadeSnapshotFromEngine / refreshFacadeSnapshotFromModelLayers). The
// pure toFacadeProjectionLayer mapping is not gated: it is only reachable from a
// caller that already passed the gate.
import type { RenderLayer } from "./types";
import type { BasicAdjustment } from "@/engine/layerAdjustments";
import { isFacadeEnabled } from "./bridge";
import { peekFacade } from "./selectionMirror";

// Engine-layer shape the facade projection consumes. DocumentEngine.applyFacadeSnapshot
// reads these fields authoritatively (an omitted field is projected as the cleared
// default), so every field it consumes must be carried across.
export type FacadeProjectionLayer = {
  id: string;
  name: string;
  visible: boolean;
  opacity: number;
  isBackground?: boolean;
  locked?: boolean;
  lockTransparency?: boolean;
  lockPosition?: boolean;
  lockRotation?: boolean;
  blendMode?: string;
  hasAdjustments?: boolean;
  basicAdjustment?: BasicAdjustment;
  transform: { x: number; y: number; scaleX: number; scaleY: number; rotation: number };
};

// Build the flat facade-projection descriptor for one engine layer. Shared by the
// one-time seed and every refresh so all project the same fields. Deep-copy the
// adjustment so neither aliases the model object.
export function toFacadeProjectionLayer(l: FacadeProjectionLayer): RenderLayer {
  return {
    id: l.id,
    name: l.name,
    visible: l.visible,
    opacity: l.opacity,
    isBackground: l.isBackground,
    locked: l.locked,
    lockTransparency: l.lockTransparency,
    lockPosition: l.lockPosition,
    lockRotation: l.lockRotation,
    blendMode: l.blendMode,
    hasAdjustments: l.hasAdjustments,
    basicAdjustment: l.basicAdjustment ? { ...l.basicAdjustment } : undefined,
    x: l.transform.x,
    y: l.transform.y,
    scaleX: l.transform.scaleX,
    scaleY: l.transform.scaleY,
    rotation: l.transform.rotation,
    resourceId: 0,
  };
}

// Re-project a layer vector into the facade projection snapshot. The transition
// mutated the TS model outside the facade command path, and the snapshot is the
// source the next routed projection rebuilds the model from, so without this the
// projection would drop the externally added/removed layers. `dims` carries the
// caller's document size: the engine owns canvas dims, so a resize (or an undo
// that restored an earlier size) must not leave the snapshot on the old size.
// Omitted dims keep the projection's prior size. Gated: no-op when the facade
// flag is OFF or no facade exists for the doc.
export function refreshFacadeSnapshotFromEngine(
  docId: string,
  layers: RenderLayer[],
  dims?: { width: number; height: number },
): void {
  if (!isFacadeEnabled()) return;
  const f = peekFacade(docId);
  if (!f) return;
  try {
    f.seedSnapshot({
      version: f.renderedVersion,
      layers,
      width: dims?.width ?? f.snapshot.width,
      height: dims?.height ?? f.snapshot.height,
      selection: f.snapshot.selection,
    });
  } catch {
    // never let instrumentation break the legacy caller
  }
}

// Project the model's own layer vector and refresh the snapshot from it. Used by
// DocumentEngine.notifyChange(), which runs AFTER the mutation and is keyed by
// the mutating engine, so a cross-document edit refreshes its OWN document's
// facade even when the commit shim resolved another (active) engine. Carries the
// model's document size for the same reason: the engine owns canvas dims.
export function refreshFacadeSnapshotFromModelLayers(
  docId: string,
  layers: readonly FacadeProjectionLayer[],
  dims?: { width: number; height: number },
): void {
  if (!isFacadeEnabled()) return;
  // Peek before the map: a doc with no facade (the common case) skips the
  // per-layer projection entirely.
  if (!peekFacade(docId)) return;
  refreshFacadeSnapshotFromEngine(docId, layers.map(toFacadeProjectionLayer), dims);
}
