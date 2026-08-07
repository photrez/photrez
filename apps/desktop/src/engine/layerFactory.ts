import type { LayerNode, BlendMode } from "./types";

export function createLayerNode(name: string, width: number, height: number): LayerNode {
  return {
    id: `layer-${crypto.randomUUID()}`,
    name,
    type: "raster",
    visible: true,
    opacity: 1.0,
    locked: false,
    isBackground: undefined,
    blendMode: "normal",
    transform: {
      x: 0,
      y: 0,
      scaleX: 1.0,
      scaleY: 1.0,
      rotation: 0,
      flipH: false,
      flipV: false
    },
    width,
    height,
    imageBitmap: null
  };
}

export function duplicateLayerNode(layer: LayerNode): LayerNode {
  let clonedBitmap: ImageBitmap | null = null;
  if (layer.imageBitmap) {
    const offscreen = new OffscreenCanvas(layer.width, layer.height);
    const ctx = offscreen.getContext("2d");
    if (ctx) {
      ctx.drawImage(layer.imageBitmap, 0, 0);
      clonedBitmap = offscreen.transferToImageBitmap();
    }
  }

  let clonedBaseBitmap: ImageBitmap | null = null;
  if (layer.baseImageBitmap) {
    const offscreen = new OffscreenCanvas(layer.width, layer.height);
    const ctx = offscreen.getContext("2d");
    if (ctx) {
      ctx.drawImage(layer.baseImageBitmap, 0, 0);
      clonedBaseBitmap = offscreen.transferToImageBitmap();
    }
  }

  return {
    id: `layer-${crypto.randomUUID()}`,
    name: `${layer.name} copy`,
    type: layer.type,
    visible: layer.visible,
    opacity: layer.opacity,
    locked: false,
    isBackground: undefined,
    blendMode: layer.blendMode,
    transform: { ...layer.transform },
    width: layer.width,
    height: layer.height,
    imageBitmap: clonedBitmap,
    baseImageBitmap: clonedBaseBitmap,
    basicAdjustment: layer.basicAdjustment ? { ...layer.basicAdjustment } : undefined,
    hasAdjustments: layer.hasAdjustments,
    shapeParams: layer.shapeParams ? { ...layer.shapeParams } : undefined
  };
}

export function createMergedLayerNode(
  name: string,
  width: number,
  height: number,
  imageBitmap: ImageBitmap | null,
  locked: boolean,
  blendMode: BlendMode
): LayerNode {
  return {
    id: `layer-${crypto.randomUUID()}`,
    name,
    type: "raster",
    visible: true,
    opacity: 1.0,
    locked,
    isBackground: undefined,
    blendMode,
    transform: {
      x: 0,
      y: 0,
      scaleX: 1.0,
      scaleY: 1.0,
      rotation: 0,
      flipH: false,
      flipV: false
    },
    width,
    height,
    imageBitmap
  };
}
