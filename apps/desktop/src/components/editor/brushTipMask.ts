import {
  BRUSH_HARD_EDGE_THRESHOLD,
  MIN_RELIABLE_BRUSH_DIAMETER_PX,
  brushAlpha,
  getBrushProfileSupportNorm,
} from "./brushHardnessProfile";

export type BrushFalloffCurve = "cosine" | "smoothstep" | "quadratic" | "soft";

export interface BrushTipOptions {
  size: number;
  hardness: number;
  curve?: BrushFalloffCurve;
}

export interface BrushTip {
  width: number;
  height: number;
  radius: number;
  diameter: number;
  R_nominal: number;
  data: Float32Array;
  /** Size of one side of the square data array (max MAX_BRUSH_TIP_DATA_SIZE).
   *  For tips with diameter > MAX_BRUSH_TIP_DATA_SIZE, this is smaller than `diameter`
   *  and the browser upscales via drawImage destination dimensions. */
  dataSize: number;
}

/** Maximum pixel dimension of the brush tip Float32Array backing store.
 *  Values above this are downsampled: the data array stores a smaller grid
 *  and drawImage with destination dimensions lets the browser upscale.
 *  256×256 = 65K elements (~2ms rasterize) vs 2126×2126 = 4.5M elements (~485ms). */
export const MAX_BRUSH_TIP_DATA_SIZE = 256;

export interface BrushPoint {
  x: number;
  y: number;
}

export interface DabInterpolationResult {
  dabs: BrushPoint[];
  carry: number;
}

export function clamp01(value: number): number {
  if (!Number.isFinite(value)) return 0;
  return Math.max(0, Math.min(1, value));
}

export function falloff(x: number, curve: BrushFalloffCurve = "soft"): number {
  const v = clamp01(x);
  if (curve === "smoothstep") return v * v * (3 - 2 * v);
  if (curve === "quadratic") return v * v;
  if (curve === "cosine") return 0.5 - 0.5 * Math.cos(Math.PI * v);
  return Math.pow(v, 0.7);
}

export function getBrushTipOuterRadius(
  radius: number,
  hardness: number,
  curve: BrushFalloffCurve = "soft",
): number {
  if (curve === "soft") {
    if (radius * 2 < MIN_RELIABLE_BRUSH_DIAMETER_PX) return radius + 0.5;
    return radius * getBrushProfileSupportNorm(hardness);
  }
  return radius;
}

function smallRoundAlpha(distance: number, radius: number): number {
  return clamp01(radius - Math.max(distance, 0) + 0.5);
}

export function brushAlphaAtDistance(
  distance: number,
  radius: number,
  hardness: number,
  curve: BrushFalloffCurve = "soft",
): number {
  if (radius <= 0) return 0;
  const h = clamp01(hardness);

  if (curve === "soft") {
    if (radius * 2 < MIN_RELIABLE_BRUSH_DIAMETER_PX) {
      return smallRoundAlpha(distance, radius);
    }
    return brushAlpha(Math.max(distance, 0) / radius, h);
  }

  const outerRadius = getBrushTipOuterRadius(radius, h, curve);
  if (distance >= outerRadius) return 0;
  if (h >= 1) return 1;

  const coreRadius = radius * h;
  if (distance <= coreRadius) return 1;

  const featherWidth = Math.max(0.0001, outerRadius - coreRadius);
  const t = (distance - coreRadius) / featherWidth;
  const v = 1 - t;
  return falloff(v, curve);
}

export function getBrushTipCacheKey(options: BrushTipOptions): string {
  const size = Math.max(1, Math.round(options.size));
  const hardness = Math.round(clamp01(options.hardness) * 100);
  return `${size}:${hardness}:${options.curve ?? "soft"}`;
}

const BRUSH_TIP_CACHE_MAX = 32;
const brushTipCache = new Map<string, BrushTip>();

function rasterizeBrushTipWithCurve(
  brushDiameter: number,
  hardness: number,
  curve: BrushFalloffCurve,
): BrushTip {
  const _t0 = performance.now();
  const diameterNominal = Number.isFinite(brushDiameter) ? Math.max(1, brushDiameter) : 1;
  const R_nominal = diameterNominal / 2;
  const h = clamp01(hardness);
  const supportNorm = diameterNominal < MIN_RELIABLE_BRUSH_DIAMETER_PX
    ? 1
    : curve === "soft" ? getBrushProfileSupportNorm(h) : 1;
  const diameter = Math.ceil(R_nominal * supportNorm) * 2 + 2;
  // Cap data array to MAX_BRUSH_TIP_DATA_SIZE for performance.
  // The alpha profile is smooth (super-gaussian), so downsampled data
  // produces visually identical output when upscaled via drawImage.
  const dataSize = Math.min(diameter, MAX_BRUSH_TIP_DATA_SIZE);
  const data = new Float32Array(dataSize * dataSize);
  const center = diameter / 2;
  // Scale factor: maps data-space pixel to spatial pixel coordinate
  const scale = diameter / dataSize;

  // Perf (2026-08-21): profile is radially symmetric AND the grid is mirror-
  // symmetric, so compute one quadrant and mirror it (negation of offsets is
  // exact => mirrored alphas are bit-identical). Also sqrt instead of hypot
  // (hypot's overflow safety costs several-fold for no benefit here).
  const xOff = new Float64Array(dataSize);
  for (let dx = 0; dx < dataSize; dx += 1) {
    xOff[dx] = (dx + 0.5) * scale - center;
  }
  // Compute one QUADRANT (incl. center line/cell when odd); mirror writes
  // cover the rest exactly. Total expensive evaluations = dataSize^2 / 4.
  const midRow = Math.ceil(dataSize / 2);
  const midCol = Math.ceil(dataSize / 2);
  const modeSmall = diameterNominal < MIN_RELIABLE_BRUSH_DIAMETER_PX
    || (curve === "soft" && h >= BRUSH_HARD_EDGE_THRESHOLD);

  for (let dy = 0; dy < midRow; dy += 1) {
    const yOff = (dy + 0.5) * scale - center;
    const yy = yOff * yOff;
    const rowBase = dy * dataSize;
    const mirrorRowBase = (dataSize - 1 - dy) * dataSize;
    for (let dx = 0; dx < midCol; dx += 1) {
      const xo = xOff[dx];
      const distance = Math.sqrt(xo * xo + yy);
      const alpha = modeSmall
        ? smallRoundAlpha(distance, R_nominal)
        : curve === "soft"
          ? brushAlpha(distance / R_nominal, h)
          : brushAlphaAtDistance(distance, R_nominal, h, curve);
      const mdx = dataSize - 1 - dx;
      const mrow = dataSize - 1 - dy;
      data[rowBase + dx] = alpha;
      data[rowBase + mdx] = alpha;
      data[mirrorRowBase + dx] = alpha;
      data[mirrorRowBase + mdx] = alpha;
    }
  }
  const _dt = performance.now() - _t0;
  if (_dt > 1) console.warn(`[perf] rasterizeBrushTipWithCurve: ${_dt.toFixed(1)}ms (d=${diameter}, pixel=${dataSize}x${dataSize} → ${data.length}, scale=${scale.toFixed(2)})`);

  return {
    width: dataSize,
    height: dataSize,
    radius: R_nominal,
    diameter,
    R_nominal,
    data,
    dataSize,
  };
}

export function rasterizeBrushTip(brushDiameter: number, hardness: number): BrushTip {
  return rasterizeBrushTipWithCurve(brushDiameter, hardness, "soft");
}

/** @deprecated Prefer rasterizeBrushTip/getCachedBrushTip for the canonical soft tip path. */
export function createBrushTip(options: BrushTipOptions): BrushTip {
  return rasterizeBrushTipWithCurve(options.size, options.hardness, options.curve ?? "soft");
}

export function getCachedBrushTip(brushDiameter: number, hardness: number): BrushTip {
  const _t0 = performance.now();
  const diameter = Number.isFinite(brushDiameter) ? Math.max(1, brushDiameter) : 1;
  const h = clamp01(hardness);
  const key = `soft:${diameter}:${h}`;
  const cached = brushTipCache.get(key);
  if (cached) {
    // Move to end (most-recently-used)
    brushTipCache.delete(key);
    brushTipCache.set(key, cached);
    return cached;
  }
  const tip = rasterizeBrushTip(diameter, h);
  // LRU eviction: if at capacity, remove the oldest entry
  if (brushTipCache.size >= BRUSH_TIP_CACHE_MAX) {
    const oldestKey = brushTipCache.keys().next().value;
    if (oldestKey !== undefined) brushTipCache.delete(oldestKey);
  }
  brushTipCache.set(key, tip);
  const _dt = performance.now() - _t0;
  if (_dt > 1) console.warn(`[perf] getCachedBrushTip: ${_dt.toFixed(1)}ms (d=${diameter}, h=${h.toFixed(2)}, cache=${cached ? "HIT" : "MISS"})`);
  return tip;
}

/** @deprecated Kept temporarily for legacy curve callers while production converges on getCachedBrushTip. */
export function getBrushTip(options: BrushTipOptions): BrushTip {
  if ((options.curve ?? "soft") === "soft") {
    return getCachedBrushTip(options.size, options.hardness);
  }
  const key = `${options.curve}:${options.size}:${clamp01(options.hardness)}`;
  const cached = brushTipCache.get(key);
  if (cached) {
    // Move to end (MRU)
    brushTipCache.delete(key);
    brushTipCache.set(key, cached);
    return cached;
  }
  const tip = createBrushTip(options);
  // LRU eviction
  if (brushTipCache.size >= BRUSH_TIP_CACHE_MAX) {
    const oldestKey = brushTipCache.keys().next().value;
    if (oldestKey !== undefined) brushTipCache.delete(oldestKey);
  }
  brushTipCache.set(key, tip);
  return tip;
}

export function clearBrushTipCache(): void {
  brushTipCache.clear();
}

export function getEffectiveFlowMultiplier(_hardness: number): number {
  return 1;
}

export function getBrushDabSpacing(size: number, hardness: number, flow: number): number {
  // 10% of brush size — a standard soft-brush spacing (1-10%). Produces
  // significantly smoother strokes with minimal visible
  // gap between dabs. Hardness already controls the soft profile inside the mask,
  // so spacing stays geometry-agnostic across the hardness range. Flow remains a
  // stroke alpha multiplier and does not change geometric spacing.
  const spacing = size * 0.10;
  return Math.max(1, Math.round(spacing));
}

export function interpolateDabs(
  from: BrushPoint,
  to: BrushPoint,
  spacing: number,
  carry: number,
): DabInterpolationResult {
  const dx = to.x - from.x;
  const dy = to.y - from.y;
  const distance = Math.hypot(dx, dy);
  if (distance <= 0) return { dabs: [], carry };

  const dabs: BrushPoint[] = [];
  let next = spacing - carry;
  while (next <= distance + 0.0001) {
    const t = next / distance;
    dabs.push({ x: from.x + dx * t, y: from.y + dy * t });
    next += spacing;
  }

  return { dabs, carry: distance - (next - spacing) };
}

export function stampBrushTip(
  mask: Uint8ClampedArray,
  maskWidth: number,
  maskHeight: number,
  tip: BrushTip,
  centerX: number,
  centerY: number,
  alphaScale: number,
): void {
  const _t0 = performance.now();
  // dabs on the same pixel accumulate toward saturation as
  //   1 - (1 - alpha)^N
  // matching established image editors. Opacity and flow act as the
  // per-dab alpha cap, so flow=50% + ~10 passes reaches ~99% at the mask
  // center. Replaces the previous max-within-stroke semantics which made
  // brush strokes stop accumulating after the first pass.
  const aScale = clamp01(alphaScale);
  const halfExtent = tip.diameter / 2;
  const centerIndex = halfExtent - 0.5;
  const dataSize = tip.dataSize;
  // Scale factor: maps spatial coordinate to data coordinate.
  // When dataSize === diameter this is 1.0 (identity, no scaling).
  const dataScale = dataSize / tip.diameter;

  const minX = Math.max(0, Math.floor(centerX - halfExtent));
  const maxX = Math.min(maskWidth - 1, Math.ceil(centerX + halfExtent) - 1);
  const minY = Math.max(0, Math.floor(centerY - halfExtent));
  const maxY = Math.min(maskHeight - 1, Math.ceil(centerY + halfExtent) - 1);

  for (let y = minY; y <= maxY; y += 1) {
    const ty = (y - centerY + centerIndex) * dataScale;
    const y0 = Math.floor(ty);
    const y1 = y0 + 1;
    const wy = ty - y0;

    const y0In = y0 >= 0 && y0 < dataSize;
    const y1In = y1 >= 0 && y1 < dataSize;
    const y0Offset = y0 * dataSize;
    const y1Offset = y1 * dataSize;

    const rowIdx = y * maskWidth;

    for (let x = minX; x <= maxX; x += 1) {
      const tx = (x - centerX + centerIndex) * dataScale;
      const x0 = Math.floor(tx);
      const x1 = x0 + 1;
      const wx = tx - x0;

      const x0In = x0 >= 0 && x0 < dataSize;
      const x1In = x1 >= 0 && x1 < dataSize;

      const a00 = (y0In && x0In) ? tip.data[y0Offset + x0] : 0;
      const a10 = (y0In && x1In) ? tip.data[y0Offset + x1] : 0;
      const a01 = (y1In && x0In) ? tip.data[y1Offset + x0] : 0;
      const a11 = (y1In && x1In) ? tip.data[y1Offset + x1] : 0;

      const a0 = a00 * (1 - wx) + a10 * wx;
      const a1 = a01 * (1 - wx) + a11 * wx;
      const interpolatedAlpha = a0 * (1 - wy) + a1 * wy;

      if (interpolatedAlpha <= 0) continue;

      const scaled = interpolatedAlpha * aScale;
      if (scaled <= 0) continue;

      const idx = rowIdx + x;
      const cur = mask[idx];
      if (cur >= 255) continue;
      mask[idx] = cur + Math.round((255 - cur) * scaled);
    }
  }
  const _dt = performance.now() - _t0;
  if (_dt > 1) console.warn(`[perf] stampBrushTip: ${_dt.toFixed(2)}ms (tip=${tip.diameter}px, α=${alphaScale.toFixed(2)})`);
}

export function brushPointsEqual(
  a: BrushPoint | null,
  b: BrushPoint | null,
): boolean {
  return Boolean(
    a
    && b
    && Math.abs(a.x - b.x) <= 0.0001
    && Math.abs(a.y - b.y) <= 0.0001,
  );
}

export function stampTerminalBrushTip(
  mask: Uint8ClampedArray,
  maskWidth: number,
  maskHeight: number,
  tip: BrushTip,
  endpoint: BrushPoint,
  lastDab: BrushPoint | null,
  alphaScale: number,
): boolean {
  if (brushPointsEqual(lastDab, endpoint)) {
    return false;
  }

  stampBrushTip(mask, maskWidth, maskHeight, tip, endpoint.x, endpoint.y, alphaScale);
  return true;
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

export interface RgbaColor {
  r: number;
  g: number;
  b: number;
  a: number;
}

export function parsePaintColor(color: string): RgbaColor {
  const rgba = color.match(/rgba?\(\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)(?:\s*,\s*([\d.]+))?\s*\)/);
  if (rgba) {
    return {
      r: Math.max(0, Math.min(255, Number(rgba[1]))),
      g: Math.max(0, Math.min(255, Number(rgba[2]))),
      b: Math.max(0, Math.min(255, Number(rgba[3]))),
      a: rgba[4] === undefined ? 1 : Math.max(0, Math.min(1, Number(rgba[4]))),
    };
  }

  const hex = color.replace("#", "");
  if (/^[0-9a-fA-F]{6}$/.test(hex)) {
    return {
      r: parseInt(hex.slice(0, 2), 16),
      g: parseInt(hex.slice(2, 4), 16),
      b: parseInt(hex.slice(4, 6), 16),
      a: 1,
    };
  }
  if (/^[0-9a-fA-F]{3}$/.test(hex)) {
    return {
      r: parseInt(hex[0] + hex[0], 16),
      g: parseInt(hex[1] + hex[1], 16),
      b: parseInt(hex[2] + hex[2], 16),
      a: 1,
    };
  }
  return { r: 0, g: 0, b: 0, a: 1 };
}

export function compositeMaskToImageData(
  imageData: ImageData,
  mask: Uint8ClampedArray,
  color: string,
  isEraser: boolean,
): void {
  const data = imageData.data;
  const paint = parsePaintColor(color);
  const strokeAlpha = Math.max(0, Math.min(1, paint.a));

  for (let i = 0; i < data.length; i += 4) {
    const maskAlpha = mask[i / 4] / 255;
    if (maskAlpha <= 0) continue;

    const alpha = maskAlpha * strokeAlpha;

    if (isEraser) {
      data[i + 3] = Math.round(data[i + 3] * (1 - alpha));
      continue;
    }

    const dstA = data[i + 3] / 255;
    const outA = alpha + dstA * (1 - alpha);
    if (outA <= 0) {
      data[i] = 0;
      data[i + 1] = 0;
      data[i + 2] = 0;
      data[i + 3] = 0;
      continue;
    }

    data[i] = Math.round((paint.r * alpha + data[i] * dstA * (1 - alpha)) / outA);
    data[i + 1] = Math.round((paint.g * alpha + data[i + 1] * dstA * (1 - alpha)) / outA);
    data[i + 2] = Math.round((paint.b * alpha + data[i + 2] * dstA * (1 - alpha)) / outA);
    data[i + 3] = Math.round(outA * 255);
  }
}

export function paintMaskToContext(
  ctx: CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D,
  mask: Uint8ClampedArray,
  width: number,
  height: number,
  color: string,
  isEraser: boolean,
): void {
  const imageData = ctx.getImageData(0, 0, width, height);
  compositeMaskToImageData(imageData, mask, color, isEraser);
  ctx.putImageData(imageData, 0, 0);
}

/**
 * Composite mask into ImageData, iterating only over the dirty region.
 * `imageDataOriginX/Y` is the document-space origin of the ImageData
 * sub-region (so we can correctly index into both the full mask array
 * and the sub-region ImageData).
 */
export function compositeMaskToImageDataDirty(
  imageData: ImageData,
  imageDataOriginX: number,
  imageDataOriginY: number,
  mask: Uint8ClampedArray,
  maskWidth: number,
  rect: DirtyRect,
  color: string,
  isEraser: boolean,
): void {
  const data = imageData.data;
  const imgW = imageData.width;
  const paint = parsePaintColor(color);
  const strokeAlpha = Math.max(0, Math.min(1, paint.a));

  for (let y = rect.y0; y < rect.y1; y++) {
    const rowInMask = y * maskWidth;
    const rowInImage = (y - imageDataOriginY) * imgW;
    for (let x = rect.x0; x < rect.x1; x++) {
      const maskIdx = rowInMask + x;
      const maskAlpha = mask[maskIdx] / 255;
      if (maskAlpha <= 0) continue;

      const i = (rowInImage + (x - imageDataOriginX)) << 2;
      const alpha = maskAlpha * strokeAlpha;

      if (isEraser) {
        data[i + 3] = Math.round(data[i + 3] * (1 - alpha));
        continue;
      }

      const dstA = data[i + 3] / 255;
      const outA = alpha + dstA * (1 - alpha);
      if (outA <= 0) {
        data[i] = data[i + 1] = data[i + 2] = data[i + 3] = 0;
        continue;
      }
      data[i]     = Math.round((paint.r * alpha + data[i]     * dstA * (1 - alpha)) / outA);
      data[i + 1] = Math.round((paint.g * alpha + data[i + 1] * dstA * (1 - alpha)) / outA);
      data[i + 2] = Math.round((paint.b * alpha + data[i + 2] * dstA * (1 - alpha)) / outA);
      data[i + 3] = Math.round(outA * 255);
    }
  }
}

/**
 * Paint mask to canvas context using only the dirty region — avoids
 * full-canvas getImageData/putImageData per event.
 */
export function paintMaskToContextDirty(
  ctx: CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D,
  mask: Uint8ClampedArray,
  maskWidth: number,
  maskHeight: number,
  rect: DirtyRect,
  color: string,
  isEraser: boolean,
): void {
  const r = clampDirtyRect(rect, maskWidth, maskHeight);
  if (r.x1 <= r.x0 || r.y1 <= r.y0) return;

  const subWidth = r.x1 - r.x0;
  const subHeight = r.y1 - r.y0;
  const imageData = ctx.getImageData(r.x0, r.y0, subWidth, subHeight);
  compositeMaskToImageDataDirty(imageData, r.x0, r.y0, mask, maskWidth, r, color, isEraser);
  ctx.putImageData(imageData, r.x0, r.y0);
}

// ── EXPERIMENT: compositor-GPU dab (R3 candidate, flag-gated) ────────────────
// Renders a dab as a Canvas2D radialGradient fillRect whose alpha stops sample
// tip.data along the radius — the profile is preserved but the per-pixel work
// moves to the platform compositor (measured ~free vs 10.5ms/dab at d512).
// OFF by default; enable with localStorage["photrez.gradientDab"]="1".
export function isGradientDabEnabled(): boolean {
  try {
    return typeof localStorage !== "undefined" && localStorage.getItem("photrez.gradientDab") === "1";
  } catch {
    return false;
  }
}

/** Sample the tip profile radially into gradient stops (center -> edge). */
export function buildGradientStops(tip: BrushTip, stopCount = 12): Array<[number, number]> {
  const ds = tip.dataSize;
  const c = ds / 2;
  const stops: Array<[number, number]> = [];
  for (let s = 0; s <= stopCount; s++) {
    const t = s / stopCount; // 0=center, 1=edge
    const x = Math.min(ds - 1, Math.round(c + t * (c - 0.5)));
    const a = tip.data[Math.round(c) * ds + x] ?? 0;
    stops.push([t, a]);
  }
  return stops;
}

/**
 * Paint one dab via radialGradient fillRect (compositor-GPU path).
 * Accumulation uses repeated source-over, which approximates (not bit-matches)
 * the legacy mask-saturation math — visual parity is under evaluation.
 */
export function paintGradientDab(
  ctx: CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D,
  tip: BrushTip,
  endpoint: BrushPoint,
  lastDab: BrushPoint | null,
  alphaScale: number,
  color: string,
): boolean {
  if (brushPointsEqual(lastDab, endpoint)) return false;

  const r = tip.diameter / 2;
  const grad = ctx.createRadialGradient(endpoint.x, endpoint.y, 0, endpoint.x, endpoint.y, r);
  const stops = buildGradientStops(tip);
  // parse hex color to rgba parts once
  const hex = color.replace("#", "");
  const cr = parseInt(hex.slice(0, 2), 16) || 0;
  const cg = parseInt(hex.slice(2, 4), 16) || 0;
  const cb = parseInt(hex.slice(4, 6), 16) || 0;
  const base = Math.max(0, Math.min(1, alphaScale));
  for (const [t, a] of stops) {
    grad.addColorStop(Math.min(1, t), `rgba(${cr},${cg},${cb},${(a * base).toFixed(4)})`);
  }
  ctx.globalCompositeOperation = "source-over";
  ctx.fillStyle = grad as unknown as string;
  ctx.fillRect(endpoint.x - r, endpoint.y - r, tip.diameter, tip.diameter);
  return true;
}

export function paintTransientBrushTipToContext(
  ctx: CanvasRenderingContext2D | OffscreenCanvasRenderingContext2D,
  tip: BrushTip,
  endpoint: BrushPoint,
  lastDab: BrushPoint | null,
  alphaScale: number,
  color: string,
  isEraser: boolean,
): boolean {
  if (!isEraser && isGradientDabEnabled()) {
    return paintGradientDab(ctx, tip, endpoint, lastDab, alphaScale, color);
  }
  if (brushPointsEqual(lastDab, endpoint)) return false;

  const canvasWidth = ctx.canvas.width;
  const canvasHeight = ctx.canvas.height;
  const cx = tip.diameter / 2;
  const cy = tip.diameter / 2;
  const minX = Math.max(0, Math.floor(endpoint.x - cx));
  const maxX = Math.min(canvasWidth - 1, Math.ceil(endpoint.x + cx) - 1);
  const minY = Math.max(0, Math.floor(endpoint.y - cy));
  const maxY = Math.min(canvasHeight - 1, Math.ceil(endpoint.y + cy) - 1);
  if (maxX < minX || maxY < minY) return false;

  const width = maxX - minX + 1;
  const height = maxY - minY + 1;
  const mask = new Uint8ClampedArray(width * height);
  stampBrushTip(
    mask,
    width,
    height,
    tip,
    endpoint.x - minX,
    endpoint.y - minY,
    alphaScale,
  );

  const imageData = ctx.getImageData(minX, minY, width, height);
  compositeMaskToImageData(imageData, mask, color, isEraser);
  ctx.putImageData(imageData, minX, minY);
  return true;
}
