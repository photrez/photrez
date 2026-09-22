// Build the exact `CanonicalDocument` JSON shape the native Rust engine
// deserializes (crates/core/src/canonical_model.rs, serde camelCase). This is an
// ADDITIVE native-authority seed: it mirrors the open-path layer seed but carries
// the FULL document content (every layer field + selection), so the native engine
// holds a complete typed shadow of the authoritative TS model at open time.
//
// It is only ever invoked from the gated native-authority path (see bridge
// seedNativeCanonical); the wasm opt-out path never calls it, so production is
// byte-identical.
//
// Field rules (cross-checked against canonical_model.rs serde attributes):
//  - camelCase keys; `type` carries the layer `type`.
//  - resourceId is OMITTED (None at open - the workspace seed uses 0 = absent).
//  - imageBitmap / baseImageBitmap / bitmapEpoch are transient pixel references
//    and are never serialized into the canonical content model.
//  - activeLayerId / viewport / dirty / format / version are UI/session/save
//    state and are excluded from the canonical content model.
//  - optional fields (isBackground, locks, basicAdjustment, shapeParams,
//    textData) are included only when present on the LayerNode; if a claimed
//    field is missing we omit the key rather than invent a value.
//  - REQUIRED fields (id, name, type, visible, opacity, locked, blendMode,
//    transform, width, height) are ALWAYS emitted. Legacy file-restored layers
//    (editorOpenImage.ts JSON.parse as DocumentModel, snapshot.ts verbatim copies)
//    may omit `locked` or `transform.flipH` on old files. We do NOT backfill them:
//    fabricating a value would be inventing document content, which is forbidden.
//    A missing required key makes the WHOLE document fail native validation (the
//    workspace surfaces it via a visible console.warn). The shadow seed fails
//    whole-document by design - partial/per-layer defaulting is not an option.

import type { DocumentEngine } from "@/engine/document";
import type { LayerNode } from "@/engine/types";
import { canonicalPayloadByDoc, isNativeAuthority, seedNativeCanonical, setCanonicalPending } from "./bridge";

type JsonObject = Record<string, unknown>;

function buildCanonicalLayer(layer: LayerNode): JsonObject {
  const out: JsonObject = {
    id: layer.id,
    name: layer.name,
    type: layer.type,
    visible: layer.visible,
    opacity: layer.opacity,
    locked: layer.locked,
    blendMode: layer.blendMode,
    transform: layer.transform,
    width: layer.width,
    height: layer.height,
  };
  // Optional fields: include only when present (serde(default) fills the rest).
  if (layer.isBackground !== undefined) out.isBackground = layer.isBackground;
  if (layer.lockTransparency !== undefined) out.lockTransparency = layer.lockTransparency;
  if (layer.lockPosition !== undefined) out.lockPosition = layer.lockPosition;
  if (layer.lockRotation !== undefined) out.lockRotation = layer.lockRotation;
  if (layer.hasAdjustments !== undefined) out.hasAdjustments = layer.hasAdjustments;
  if (layer.basicAdjustment !== undefined) out.basicAdjustment = layer.basicAdjustment;
  if (layer.shapeParams !== undefined) out.shapeParams = layer.shapeParams;
  if (layer.textData !== undefined) out.textData = layer.textData;
  // resourceId intentionally omitted: None at open.
  // imageBitmap / baseImageBitmap / bitmapEpoch are transient and never serialized.
  return out;
}

/**
 * Produce the canonical-document seed payload for `engine`. Returns a JSON string
 * that deserializes into `CanonicalDocument` on the native side.
 */
export function buildCanonicalDocumentPayload(engine: DocumentEngine): string {
  const doc: JsonObject = {
    id: engine.getId(),
    name: engine.getName(),
    width: engine.getWidth(),
    height: engine.getHeight(),
    layers: engine.getLayers().map(buildCanonicalLayer),
  };
  const selection = engine.getSelection();
  if (selection) doc.selection = selection;
  return JSON.stringify(doc);
}

// Re-push the full canonical-document shadow into the native engine for a doc.
// Used after a TS-side mutation the native reconciliation could not observe
// directly (facade addLayer under native authority, or a mirrored external
// transition): the native engine mints/advances the document model but cannot
// reconstruct the canonical-only fields, so the TS model is re-pushed in full.
// Gated by native authority (wasm opt-out => no-op, production unchanged). The
// native seed replaces the shadow unconditionally, so a re-push is idempotent
// and safe to fire after every such event.
//
// Ordering: the re-push's invoke promise overwrites the per-doc canonical slot
// (setCanonicalPending), which flushExternalTransitions - awaited by
// syncFromEngine before every facade command - drains before the next command
// dispatches, so the shadow is complete when the next command reads it.
//
// Crash-window honesty: the slot keeps only the LATEST re-push, so an
// intermediate payload can be superseded before any flush awaits it, and an
// older invoke already on the wire can land after a newer one. The shadow can
// therefore stay stale longer than under the old chained barrier. That matches
// today's mid-push death semantics exactly: if the process dies mid-push the
// native shadow is already stale-or-absent while the TS model stays
// authoritative, and the next successful re-push heals the shadow in full
// because every payload is complete, never a delta. Coalescing widens a window
// that already exists; it adds no new divergence class.
export async function repushCanonicalDocument(docId: string, engine: DocumentEngine): Promise<void> {
  if (!isNativeAuthority()) return;
  const key = docId === "" ? "default" : docId;
  const payload = buildCanonicalDocumentPayload(engine);
  // Cheap duplicate guard: a byte-identical payload means the shadow already
  // holds exactly this content, so skip the invoke AND the slot write. String
  // equality is a full-content compare far cheaper than an IPC round-trip.
  if (canonicalPayloadByDoc.get(key) === payload) return;
  canonicalPayloadByDoc.set(key, payload);
  const p = seedNativeCanonical(docId, payload);
  setCanonicalPending(key, p);
  try {
    await p;
  } catch (e) {
    // A failed push heals nothing: evict the record only if it is still ours,
    // so the next identical re-push retries instead of skipping forever. A newer
    // payload recorded meanwhile stays (its own push heals the shadow in full).
    if (canonicalPayloadByDoc.get(key) === payload) canonicalPayloadByDoc.delete(key);
    throw e;
  }
}

// Test seam for the payload-compare guard. The record itself lives in bridge
// (canonicalPayloadByDoc) so workspace-close eviction clears it; reset per test.
export function __resetCanonicalRepushForTests(): void {
  canonicalPayloadByDoc.clear();
}
