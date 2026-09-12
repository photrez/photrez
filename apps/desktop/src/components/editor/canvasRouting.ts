// Routing seam for the two document-size ops (resize canvas, apply crop). The
// facade command owns the document size in the native engine; the host
// re-uploads layer textures after a successful dispatch. The legacy path is
// byte-identical: when the route returns "legacy" the caller runs the original
// engine statements unchanged.
//
// Guard order (all BEFORE any dispatch):
//   1. flag + native authority (same gate as the structural routes),
//   2. pixel-baking crop variants defer to the legacy host path,
//   3. finite / positive / device-max floors on the FINAL size,
//   4. resize memory budget (the same projection DocumentEngine.resizeCanvas
//      uses to throw E_RESOURCE_LIMIT).
//
// Each route first checks for a document-size divergence between the TS model
// and the native engine. A legacy resize/crop undone through the TS history
// moves the TS size without touching the native engine's stored size, so the
// native canvas arms would center layers against stale bounds. When the two
// differ, the TS size is pushed into the native engine first, then the real
// command runs.

import type { DocumentEngine } from "@/engine/document";
import type { WebGL2Backend } from "@/renderer/webgl2";
import type { RenderScheduler } from "@/renderer/scheduler";
import { getEffectiveMaxDim } from "@/engine/types";
import { resizeCanvasExceedsBudget } from "@/engine/document";
import { getSnapshot } from "@/lib/protocol/bridge";
import {
  commitFacadeApplyCrop,
  commitFacadeResizeCanvas,
  isFacadeEnabled,
  isNativeAuthority,
} from "@/lib/protocol/facadeRegistry";

export type CanvasRouteStatus = "applied" | "legacy" | "error";

export type CanvasCropOptions = {
  deleteCroppedPixels?: boolean;
  targetSize?: { w: number; h: number } | null;
  rotation?: number;
  fillBackgroundColor?: string | null;
};

function finitePositive(v: number): boolean {
  return Number.isFinite(v) && v > 0;
}

// Push the TS document size into the native engine when the native stored size
// has drifted (a legacy canvas entry undone/redone in TS). No-op when the sizes
// already agree or the native snapshot carries no size yet.
async function rebaseNativeDimsIfDiverged(engine: DocumentEngine): Promise<void> {
  const snap = await getSnapshot(engine.getId());
  if (snap.width === undefined || snap.height === undefined) return;
  if (snap.width === engine.getWidth() && snap.height === engine.getHeight()) return;
  await commitFacadeResizeCanvas(engine as never, engine.getWidth(), engine.getHeight());
}

// Re-upload every layer texture and request a redraw after the document size
// changed. resizeToViewport stays with the caller (it needs viewport dims).
function reuploadLayers(
  engine: DocumentEngine,
  renderer: WebGL2Backend,
  scheduler: RenderScheduler,
): void {
  for (const layer of engine.getLayers()) {
    if (layer.imageBitmap) renderer.uploadImage(layer.id, layer.imageBitmap);
  }
  scheduler.requestRender();
}

export async function routeResizeCanvas(
  engine: DocumentEngine,
  _history: unknown,
  renderer: WebGL2Backend,
  scheduler: RenderScheduler,
  w: number,
  h: number,
): Promise<CanvasRouteStatus> {
  if (!isFacadeEnabled() || !isNativeAuthority()) return "legacy";
  if (!finitePositive(w) || !finitePositive(h)) return "error";
  if (w > getEffectiveMaxDim() || h > getEffectiveMaxDim()) return "error";
  if (
    resizeCanvasExceedsBudget(
      engine.getWidth(),
      engine.getHeight(),
      engine.getLayers().length,
      engine.calculateMemoryUsage(),
      w,
      h,
    )
  ) {
    return "error";
  }
  try {
    await rebaseNativeDimsIfDiverged(engine);
  } catch {
    return "error";
  }
  let r: { status: string; count?: number };
  try {
    r = await commitFacadeResizeCanvas(engine as never, w, h);
  } catch {
    return "error";
  }
  if (r.status === "legacy") return "legacy";
  if (r.status !== "applied") return "error";
  reuploadLayers(engine, renderer, scheduler);
  return "applied";
}

export async function routeApplyCrop(
  engine: DocumentEngine,
  _history: unknown,
  renderer: WebGL2Backend,
  scheduler: RenderScheduler,
  x: number,
  y: number,
  w: number,
  h: number,
  options: CanvasCropOptions,
): Promise<CanvasRouteStatus> {
  if (!isFacadeEnabled() || !isNativeAuthority()) return "legacy";
  // The pixel-baking variants (delete cropped pixels / fill background) are not
  // expressible as the recenter-only native arm; keep them on the host path.
  if (options.deleteCroppedPixels || options.fillBackgroundColor) return "legacy";
  if (!Number.isFinite(x) || !Number.isFinite(y)) return "error";
  if (!finitePositive(w) || !finitePositive(h)) return "error";
  // The native document size comes from the target size when one is set, exactly
  // like DocumentEngine.applyCrop.
  const targetSize = options.targetSize ?? null;
  const finalW = targetSize ? targetSize.w : w;
  const finalH = targetSize ? targetSize.h : h;
  if (!finitePositive(finalW) || !finitePositive(finalH)) return "error";
  if (finalW > getEffectiveMaxDim() || finalH > getEffectiveMaxDim()) return "error";

  // A crop that keeps the whole canvas (no offset, no rotation, no target size)
  // changes neither the layer geometry nor the document size. Dispatching would
  // still make the native engine record an entry, but a same-size entry carries
  // no document-size delta and (with an empty layer set) no layer delta either,
  // so its undo reports "no change" and the undo orchestrator drops the step - a
  // double-step. Treat the whole-canvas crop as the no-op it is: report applied
  // so the caller still closes the crop session, but skip the dispatch and its
  // phantom entry. Legacy committed a real step here, but the native undo never
  // restored the cleared selection either way, so no visible state is lost;
  // selection is still cleared below, exactly like the legacy crop.
  const wholeCanvasCrop =
    !targetSize &&
    x === 0 &&
    y === 0 &&
    (options.rotation ?? 0) === 0 &&
    finalW === engine.getWidth() &&
    finalH === engine.getHeight();
  if (wholeCanvasCrop) {
    engine.clearSelection();
    return "applied";
  }

  try {
    await rebaseNativeDimsIfDiverged(engine);
  } catch {
    return "error";
  }
  let r: { status: string; count?: number };
  try {
    r = await commitFacadeApplyCrop(
      engine as never,
      x,
      y,
      w,
      h,
      options.rotation,
      targetSize?.w,
      targetSize?.h,
    );
  } catch {
    return "error";
  }
  if (r.status === "legacy") return "legacy";
  if (r.status !== "applied") return "error";
  // The native crop arm clears its own selection. Mirror that onto the TS model
  // with the canonical engine method (rust clear + model.selection = null +
  // notifyChange), the same user-visible clear the legacy applyCrop performs.
  engine.clearSelection();
  reuploadLayers(engine, renderer, scheduler);
  return "applied";
}
