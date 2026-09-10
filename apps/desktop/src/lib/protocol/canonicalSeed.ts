// Build the exact `CanonicalDocument` JSON shape the native Rust engine
// deserializes (crates/core/src/canonical_model.rs, serde camelCase). This is an
// ADDITIVE native-authority seed: it mirrors the open-path layer seed but carries
// the FULL document content (every layer field + selection), so the native engine
// holds a complete typed shadow of the authoritative TS model at open time.
//
// It is only ever invoked from the gated native-authority path (see bridge
// seedNativeCanonical); the default (wasm) path never calls it, so production is
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
import { isNativeAuthority, seedNativeCanonical, setExternalTransitionPending } from "./bridge";

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
// Gated by native authority (default OFF => no-op, production unchanged). The
// native seed replaces the shadow unconditionally, so a re-push is idempotent
// and safe to fire after every such event.
//
// Ordering: the re-push's invoke promise is registered in the per-doc
// external-transition barrier (flushExternalTransitions, awaited by syncFromEngine
// before every facade command) so a re-push for command 1 is guaranteed to land
// before command 2 dispatches — otherwise a stale native doc_size could be read
// by the next command. The barrier chain swallows a rejected re-push so it never
// wedges the barrier.
export async function repushCanonicalDocument(docId: string, engine: DocumentEngine): Promise<void> {
  if (!isNativeAuthority()) return;
  const key = docId === "" ? "default" : docId;
  const p = seedNativeCanonical(docId, buildCanonicalDocumentPayload(engine));
  setExternalTransitionPending(key, p);
  await p;
}
