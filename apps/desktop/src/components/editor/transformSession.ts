import type { DocumentModel, Transform2D } from "@/engine/types";
import type { LayerTransformSession } from "./tools/editorState";

export interface TransformSessionEngine {
  getId(): string;
  snapshot(): DocumentModel;
  restore(snapshot: DocumentModel): void;
  getLayer(id: string): { id: string; transform: Transform2D } | null | undefined;
  transformLayer(id: string, transform: Partial<Transform2D>): void;
}

export interface TransformSessionHistory {
  commit(snapshot: DocumentModel, label?: string): void;
}

export function isSessionForEngine(
  session: LayerTransformSession | null,
  engine: TransformSessionEngine | null | undefined
): session is LayerTransformSession {
  return Boolean(session && engine && session.documentId === engine.getId());
}

function transformsEqual(a: Transform2D, b: Transform2D): boolean {
  return (
    a.x === b.x &&
    a.y === b.y &&
    a.scaleX === b.scaleX &&
    a.scaleY === b.scaleY &&
    a.rotation === b.rotation &&
    a.flipH === b.flipH &&
    a.flipV === b.flipV
  );
}

export function commitLayerTransformSession(
  session: LayerTransformSession | null,
  engine: TransformSessionEngine | null | undefined,
  history: TransformSessionHistory | null | undefined
): boolean {
  if (!session || !engine || !history) return false;
  if (!isSessionForEngine(session, engine)) return false;
  const layer = engine.getLayer(session.layerId);
  if (!layer) return true;

  // Skip the commit if the layer transform did not actually change during the
  // session. Otherwise pressing Apply on an unchanged session pushes a ghost
  // entry to the undo stack — undoing it produces no visual change and makes
  // the user feel they can't undo all the way back to the original state.
  // (@regression 2026-06-18 follow-up.)
  if (!layer.transform || transformsEqual(layer.transform, session.originalTransform)) {
    return true;
  }

  history.commit(session.originalSnapshot, "Transform Layer");
  return true;
}

export function cancelLayerTransformSession(
  session: LayerTransformSession | null,
  engine: TransformSessionEngine | null | undefined
): boolean {
  if (!session || !engine) return false;
  if (!isSessionForEngine(session, engine)) return false;
  engine.restore(session.originalSnapshot);
  return true;
}

export function resetLayerTransformPreview(
  session: LayerTransformSession | null,
  engine: TransformSessionEngine | null | undefined
): boolean {
  if (!session || !engine) return false;
  if (!isSessionForEngine(session, engine)) return false;
  engine.transformLayer(session.layerId, session.originalTransform);
  return true;
}
