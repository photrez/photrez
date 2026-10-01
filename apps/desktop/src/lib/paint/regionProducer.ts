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

export interface DirtyRect {
  x0: number;
  y0: number;
  x1: number;
  y1: number;
}

export function emptyDirtyRect(): DirtyRect {
  return { x0: Number.MAX_SAFE_INTEGER, y0: Number.MAX_SAFE_INTEGER, x1: -1, y1: -1 };
}

export function expandDirtyRect(
  rect: DirtyRect,
  x: number,
  y: number,
  radius: number,
): DirtyRect {
  return {
    x0: Math.min(rect.x0, Math.floor(x - radius)),
    y0: Math.min(rect.y0, Math.floor(y - radius)),
    x1: Math.max(rect.x1, Math.ceil(x + radius) + 1),
    y1: Math.max(rect.y1, Math.ceil(y + radius) + 1),
  };
}

export function clampDirtyRect(rect: DirtyRect, w: number, h: number): DirtyRect {
  return {
    x0: Math.max(0, rect.x0),
    y0: Math.max(0, rect.y0),
    x1: Math.min(w, rect.x1),
    y1: Math.min(h, rect.y1),
  };
}

export function unionDirtyRect(a: DirtyRect, b: DirtyRect): DirtyRect {
  return {
    x0: Math.min(a.x0, b.x0),
    y0: Math.min(a.y0, b.y0),
    x1: Math.max(a.x1, b.x1),
    y1: Math.max(a.y1, b.y1),
  };
}

/**
 * Pre-IPC fence for rust_pixels_write_region: identity and geometry are
 * validated before any epoch read or write attempt, so a malformed target
 * surfaces as RangeError instead of reaching the native boundary.
 * Reuses clampRegionToLayer for non-finite/zero/negative/overflow/no-overlap
 * inputs, then requires the region to survive clamping unchanged (i.e. it was
 * already exactly inside the surface).
 *
 * The two branches are not equally live:
 * - identity (empty docId/layerId) IS reachable: a deferred commit can fire
 *   after the active document was closed, and that is the only case proven to
 *   stop before any IPC (zero rust_pixels_get_epoch, zero write attempt).
 * - geometry is dead by construction on a live stroke: every production
 *   caller derives the region from computeDirtyRegion clamped to these same
 *   surface dims (see useBrushOverlay's single strokeRegion), so a rect that
 *   fails to survive clamping cannot reach here today. It stays as a
 *   defense-in-depth pin so a future call site that builds its own rect gets
 *   the same RangeError instead of a bogus read at the native boundary.
 */
export function assertWriteRegionTarget(
  docId: string,
  layerId: string,
  region: Region,
  surfaceW: number,
  surfaceH: number,
): void {
  if (!docId || !layerId) {
    throw new RangeError("write region requires a non-empty docId and layerId");
  }
  const clamped = clampRegionToLayer(region, surfaceW, surfaceH);
  if (clamped.x !== region.x || clamped.y !== region.y || clamped.w !== region.w || clamped.h !== region.h) {
    throw new RangeError(
      `write region ${JSON.stringify(region)} is not inside the ${surfaceW}x${surfaceH} surface`,
    );
  }
}

/** Exact byte-length fence: the rgba payload must cover region.w*region.h*4 bytes, no more, no less. */
export function assertWriteRegionBytes(rgbaByteLength: number, region: Region): void {
  const expected = region.w * region.h * 4;
  if (rgbaByteLength !== expected) {
    throw new RangeError(
      `write region rgba is ${rgbaByteLength} bytes; expected ${expected} for ${region.w}x${region.h}`,
    );
  }
}

/**
 * Rebuild a full-layer RGBA buffer (width*height*4) from 256-grid Rust tiles.
 * Tile `data` arrives as a plain number[] (JSON), so copy by index.
 *
 * Lives here, not in a pointer tool, because every Rust-recorded pixel op needs
 * it (bucket, fill, gradient, selection delete) and a tool module imports the
 * selection feature - importing from there would close an import cycle.
 */
export function reconstructLayerBuffer(
  tiles: { x: number; y: number; w: number; h: number; data: number[] }[],
  width: number,
  height: number,
): Uint8ClampedArray {
  const buf = new Uint8ClampedArray(width * height * 4);
  for (const t of tiles) {
    const rowBytes = t.w * 4;
    for (let row = 0; row < t.h; row++) {
      const srcOff = rowBytes * row;
      const dstOff = ((t.y + row) * width + t.x) * 4;
      for (let i = 0; i < rowBytes; i++) {
        buf[dstOff + i] = t.data[srcOff + i];
      }
    }
  }
  return buf;
}

/**
 * Pure helper: find the bounding box of pixels that differ between `before`
 * and `after` (both width*height*4 RGBA) and extract the `after` sub-region.
 * Returns null when nothing changed. The changed region is what gets pushed to
 * Rust as one canonical write (localized, not the whole layer).
 */
export function computeChangedRegion(
  before: Uint8ClampedArray,
  after: Uint8ClampedArray,
  width: number,
  height: number,
): { x: number; y: number; w: number; h: number; rgba: Uint8ClampedArray } | null {
  let minX = width, minY = height, maxX = -1, maxY = -1;
  const n = width * height * 4;
  for (let i = 0; i < n; i += 4) {
    if (
      before[i] !== after[i] ||
      before[i + 1] !== after[i + 1] ||
      before[i + 2] !== after[i + 2] ||
      before[i + 3] !== after[i + 3]
    ) {
      const p = i / 4;
      const px = p % width;
      const py = (p / width) | 0;
      if (px < minX) minX = px;
      if (py < minY) minY = py;
      if (px > maxX) maxX = px;
      if (py > maxY) maxY = py;
    }
  }
  if (maxX < 0) return null;
  const x = minX, y = minY, w = maxX - minX + 1, h = maxY - minY + 1;
  const rgba = new Uint8ClampedArray(w * h * 4);
  for (let row = 0; row < h; row++) {
    const src = ((y + row) * width + x) * 4;
    const dst = row * w * 4;
    rgba.set(after.subarray(src, src + w * 4), dst);
  }
  return { x, y, w, h, rgba };
}
