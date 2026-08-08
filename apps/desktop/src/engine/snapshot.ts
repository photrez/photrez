import type { DocumentModel } from "./types";

export function createSnapshot(model: DocumentModel): DocumentModel {
  return {
    id: model.id,
    name: model.name,
    width: model.width,
    height: model.height,
    activeLayerId: model.activeLayerId,
    selection: model.selection ? { ...model.selection } : null,
    viewport: { ...model.viewport },
    dirty: model.dirty,
    layers: model.layers.map(l => ({
      id: l.id,
      name: l.name,
      type: l.type,
      visible: l.visible,
      opacity: l.opacity,
      locked: l.locked,
      isBackground: l.isBackground,
      lockTransparency: l.lockTransparency,
      lockPosition: l.lockPosition,
      lockRotation: l.lockRotation,
      hasAdjustments: l.hasAdjustments,
      basicAdjustment: l.basicAdjustment ? { ...l.basicAdjustment } : undefined,
      baseImageBitmap: l.baseImageBitmap,
      blendMode: l.blendMode,
      transform: { ...l.transform },
      width: l.width,
      height: l.height,
      imageBitmap: l.imageBitmap, // Reuse reference to immutable ImageBitmap
      shapeParams: l.shapeParams ? { ...l.shapeParams } : undefined,
      // TextData is a flat object; spread is a full deep copy. Undo/redo of
      // any text op MUST restore the params or the layer loads as plain text
      // without content (shape-tool bug class d1b1403).
      textData: l.textData ? { ...l.textData } : undefined
    }))
  };
}

export function restoreSnapshot(snapshot: DocumentModel): DocumentModel {
  return {
    id: snapshot.id,
    name: snapshot.name,
    width: snapshot.width,
    height: snapshot.height,
    activeLayerId: snapshot.activeLayerId,
    selection: snapshot.selection ? { ...snapshot.selection } : null,
    viewport: { ...snapshot.viewport },
    dirty: snapshot.dirty,
    layers: snapshot.layers.map(l => ({
      id: l.id,
      name: l.name,
      type: l.type,
      visible: l.visible,
      opacity: l.opacity,
      locked: l.locked,
      isBackground: l.isBackground,
      lockTransparency: l.lockTransparency,
      lockPosition: l.lockPosition,
      lockRotation: l.lockRotation,
      hasAdjustments: l.hasAdjustments,
      basicAdjustment: l.basicAdjustment ? { ...l.basicAdjustment } : undefined,
      baseImageBitmap: l.baseImageBitmap,
      blendMode: l.blendMode,
      transform: { ...l.transform },
      width: l.width,
      height: l.height,
      imageBitmap: l.imageBitmap,
      shapeParams: l.shapeParams ? { ...l.shapeParams } : undefined,
      // TextData is a flat object; spread is a full deep copy. Undo/redo of
      // any text op MUST restore the params or the layer loads as plain text
      // without content (shape-tool bug class d1b1403).
      textData: l.textData ? { ...l.textData } : undefined
    }))
  };
}
