import type { RenderState, ViewportState } from "../engine/types";
import type { BasicAdjustment } from "../engine/layerAdjustments";

export interface RenderCapabilities {
  backend: "webgl2" | "webgpu" | "cpu";
  maxTextureSize: number;
  supportsFloatTextures: boolean;
  supportsLinearFilteringFloat: boolean;
}

export interface DirtyRectLike {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface TextureRef {
  id: string;
  texture: WebGLTexture;
  width: number;
  height: number;
}

export interface RenderBackend {
  readonly name: string;
  readonly capabilities: RenderCapabilities;

  initialize(canvas: HTMLCanvasElement): void;
  uploadImage(layerId: string, source: ImageBitmap, dirtyRect?: DirtyRectLike): TextureRef;
  destroyTexture(layerId: string): void;
  render(state: RenderState, viewProjectionMatrix?: Float32Array): void;
  resize(docWidth: number, docHeight: number, zoom: number, dpr: number): void;
  resizeToViewport(width: number, height: number, dpr: number): void;
  dispose(): void;

  readPixel(x: number, y: number): [number, number, number, number] | null;
  getCanvas(): HTMLCanvasElement | null;
  getLogicalWidth(): number;
  getLogicalHeight(): number;

  /**
   * Bake a layer's basic adjustment into pixels via the GPU and return an
   * ImageBitmap, or null if baking is unsupported (no context / test env).
   * Caller falls back to the CPU `bakeAdjustmentToBitmap` when this returns
   * null. The returned bitmap is top-left origin, straight-alpha — matching
   * `bakeAdjustmentToBitmap` so it can replace the layer's stored bitmap.
   */
  bakeLayerToBitmap(
    layerId: string,
    width: number,
    height: number,
    adjustment: BasicAdjustment,
  ): ImageBitmap | null;

  /**
   * Optional async variant of `bakeLayerToBitmap` (PBO + `getBufferSubDataAsync`):
   * same contract but the readback does not stall the main thread. The engine
   * prefers this when present, falls back to `bakeLayerToBitmap`, then to the
   * CPU pixel pass. Returns null when the async path is unavailable or fails.
   */
  bakeLayerToBitmapAsync?(
    layerId: string,
    width: number,
    height: number,
    adjustment: BasicAdjustment,
  ): Promise<ImageBitmap | null>;
}
