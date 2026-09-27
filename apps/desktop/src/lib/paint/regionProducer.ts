export type Region = { x: number; y: number; w: number; h: number };
function isFiniteRegion(region: Region | null): region is Region {
  return region !== null && [region.x, region.y, region.w, region.h].every(Number.isFinite);
}
export function computeDirtyRegion(scratchBBox: Region | null, selectionBounds: Region | null): Region | null {
  if (!isFiniteRegion(scratchBBox) || (selectionBounds !== null && !isFiniteRegion(selectionBounds))) return null;
  if (scratchBBox.w <= 0 || scratchBBox.h <= 0) return null;
  const bounds = selectionBounds ?? scratchBBox;
  const x = Math.max(Math.floor(scratchBBox.x), Math.floor(bounds.x));
  const y = Math.max(Math.floor(scratchBBox.y), Math.floor(bounds.y));
  const right = Math.min(Math.ceil(scratchBBox.x + scratchBBox.w), Math.ceil(bounds.x + bounds.w));
  const bottom = Math.min(Math.ceil(scratchBBox.y + scratchBBox.h), Math.ceil(bounds.y + bounds.h));
  if (![x, y, right, bottom].every(Number.isFinite)) return null;
  return right > x && bottom > y ? { x, y, w: right - x, h: bottom - y } : null;
}
export function clampRegionToLayer(region: Region, layerW: number, layerH: number): Region {
  if (!isFiniteRegion(region) || !Number.isFinite(layerW) || !Number.isFinite(layerH) || region.w <= 0 || region.h <= 0 || layerW <= 0 || layerH <= 0) {
    throw new RangeError("invalid region or layer dimensions");
  }
  // Non-positive input dimensions are rejected, never silently clamped to zero.
  // Finite components can still overflow their sum, so reject that output too.
  if (!Number.isFinite(region.x + region.w) || !Number.isFinite(region.y + region.h)) {
    throw new RangeError("region origin plus size overflows");
  }
  const x = Math.max(0, Math.floor(region.x));
  const y = Math.max(0, Math.floor(region.y));
  const right = Math.min(layerW, Math.ceil(region.x + region.w));
  const bottom = Math.min(layerH, Math.ceil(region.y + region.h));
  if (right <= x || bottom <= y) throw new RangeError("region does not overlap layer");
  return { x, y, w: right - x, h: bottom - y };
}
