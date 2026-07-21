/**
 * Pure pixel math for Paint Bucket (flood fill) and Gradient tools.
 *
 * This file is the WASM migration target: when the WASM module is ready,
 * swap the export to the WASM implementation. The call-site stays unchanged
 * because all functions share the same (ImageData, ...) => ImageData signature.
 *
 * Note: all functions mutate imgData in-place for performance.
 * The return value is the same reference — useful for chaining.
 */

export interface FillMask {
  x: number;
  y: number;
  w: number;
  h: number;
  shape?: "rect" | "ellipse";
  inverted?: boolean;
}

export type GradientType = "linear" | "radial";

export interface ColorStop {
  offset: number; // 0.0 – 1.0
  r: number; g: number; b: number; a: number; // RGBA 0–255
}

/** Simple distance test, same math as SelectionOperations.isInsideEllipse */
function isInsideEllipse(
  px: number, py: number,
  cx: number, cy: number, cw: number, ch: number,
): boolean {
  const hw = cw / 2;
  const hh = ch / 2;
  if (hw <= 0 || hh <= 0) return false;
  const nx = (px - (cx + hw)) / hw;
  const ny = (py - (cy + hh)) / hh;
  return (nx * nx + ny * ny) <= 1.0;
}

/**
 * Flood-fill contiguous matching pixels.
 * Queue-based scanline (safe for large areas — no recursion).
 */
export function floodFill(
  imgData: ImageData,
  sx: number,
  sy: number,
  fillR: number,
  fillG: number,
  fillB: number,
  fillA: number,
  tolerance: number,
  mask?: FillMask | null,
  contiguous: boolean = true,
): ImageData {
  const { data, width, height } = imgData;

  if (sx < 0 || sx >= width || sy < 0 || sy >= height) return imgData;
  if (!Number.isFinite(tolerance)) return imgData;

  const startIdx = (sy * width + sx) * 4;
  const sourceR = data[startIdx];
  const sourceG = data[startIdx + 1];
  const sourceB = data[startIdx + 2];
  const sourceA = data[startIdx + 3];

  // Already the fill colour → nothing to do
  if (sourceR === fillR && sourceG === fillG && sourceB === fillB && sourceA === fillA) return imgData;

  const toleranceSq = tolerance * tolerance;

  const matchesSource = (idx: number): boolean => {
    const dR = data[idx] - sourceR;
    const dG = data[idx + 1] - sourceG;
    const dB = data[idx + 2] - sourceB;
    const dA = data[idx + 3] - sourceA;
    return (dR * dR + dG * dG + dB * dB + dA * dA) <= toleranceSq;
  };

  const isInsideMask = (px: number, py: number): boolean => {
    if (!mask) return true;
    if (px < mask.x || px >= mask.x + mask.w || py < mask.y || py >= mask.y + mask.h) {
      return mask.inverted ?? false;
    }
    if (mask.shape === "ellipse" && !mask.inverted) {
      return isInsideEllipse(px, py, mask.x, mask.y, mask.w, mask.h);
    }
    if (mask.shape === "ellipse" && mask.inverted) {
      return !isInsideEllipse(px, py, mask.x, mask.y, mask.w, mask.h);
    }
    return !(mask.inverted ?? false);
  };

  // Global replacement mode (contiguous === false): replace all matching pixels across the image
  if (!contiguous) {
    for (let py = 0; py < height; py++) {
      for (let px = 0; px < width; px++) {
        if (!isInsideMask(px, py)) continue;
        const idx = (py * width + px) * 4;
        if (matchesSource(idx)) {
          data[idx] = fillR;
          data[idx + 1] = fillG;
          data[idx + 2] = fillB;
          data[idx + 3] = fillA;
        }
      }
    }
    return imgData;
  }

  // Note: queue-based, not recursion — safe for millions of pixels
  const queue: Array<[number, number]> = [[sx, sy]];
  const visited = new Uint8Array(width * height);
  visited[sy * width + sx] = 1;

  let head = 0;
  while (head < queue.length) {
    const [px, py] = queue[head++];

    if (!isInsideMask(px, py)) continue;

    const idx = (py * width + px) * 4;
    if (!matchesSource(idx)) continue;

    data[idx] = fillR;
    data[idx + 1] = fillG;
    data[idx + 2] = fillB;
    data[idx + 3] = fillA;

    // 4-directional neighbors
    if (px > 0 && !visited[py * width + (px - 1)]) {
      visited[py * width + (px - 1)] = 1;
      queue.push([px - 1, py]);
    }
    if (px < width - 1 && !visited[py * width + (px + 1)]) {
      visited[py * width + (px + 1)] = 1;
      queue.push([px + 1, py]);
    }
    if (py > 0 && !visited[(py - 1) * width + px]) {
      visited[(py - 1) * width + px] = 1;
      queue.push([px, py - 1]);
    }
    if (py < height - 1 && !visited[(py + 1) * width + px]) {
      visited[(py + 1) * width + px] = 1;
      queue.push([px, py + 1]);
    }
  }

  return imgData;
}

/**
 * Fill with a gradient defined by two points and color stops.
 */
export function gradientFill(
  imgData: ImageData,
  type: GradientType,
  ax: number, ay: number,
  bx: number, by: number,
  stops: ColorStop[],
  mask?: FillMask | null,
): ImageData {
  const { data, width, height } = imgData;

  const sortedStops = stops.slice().sort((a, b) => a.offset - b.offset);
  if (sortedStops.length < 2) return imgData;

  const dx = bx - ax;
  const dy = by - ay;
  const lenSq = dx * dx + dy * dy;

  const isInsideMask = (px: number, py: number): boolean => {
    if (!mask) return true;
    if (px < mask.x || px >= mask.x + mask.w || py < mask.y || py >= mask.y + mask.h) {
      return mask.inverted ?? false;
    }
    if (mask.shape === "ellipse" && !mask.inverted) {
      return isInsideEllipse(px, py, mask.x, mask.y, mask.w, mask.h);
    }
    if (mask.shape === "ellipse" && mask.inverted) {
      return !isInsideEllipse(px, py, mask.x, mask.y, mask.w, mask.h);
    }
    return !(mask.inverted ?? false);
  };

  const lerpStops = (t: number): [number, number, number, number] => {
    if (t <= sortedStops[0].offset) {
      const s = sortedStops[0];
      return [s.r, s.g, s.b, s.a];
    }
    if (t >= sortedStops[sortedStops.length - 1].offset) {
      const s = sortedStops[sortedStops.length - 1];
      return [s.r, s.g, s.b, s.a];
    }
    for (let i = 0; i < sortedStops.length - 1; i++) {
      const a = sortedStops[i];
      const b = sortedStops[i + 1];
      if (t >= a.offset && t <= b.offset) {
        const range = b.offset - a.offset;
        const frac = range > 0 ? (t - a.offset) / range : 0;
        return [
          Math.round(a.r + (b.r - a.r) * frac),
          Math.round(a.g + (b.g - a.g) * frac),
          Math.round(a.b + (b.b - a.b) * frac),
          Math.round(a.a + (b.a - a.a) * frac),
        ];
      }
    }
    return [0, 0, 0, 255];
  };

  for (let py = 0; py < height; py++) {
    for (let px = 0; px < width; px++) {
      if (!isInsideMask(px, py)) continue;

      const t = type === "linear"
        ? linearGradientCoord(px, py, ax, ay, dx, dy, lenSq)
        : radialGradientCoord(px, py, ax, ay, dx, dy, lenSq);

      const [cr, cg, cb, ca] = lerpStops(Math.max(0, Math.min(1, t)));
      const idx = (py * width + px) * 4;
      data[idx] = cr;
      data[idx + 1] = cg;
      data[idx + 2] = cb;
      data[idx + 3] = ca;
    }
  }

  return imgData;
}

function linearGradientCoord(
  px: number, py: number,
  ax: number, ay: number,
  dx: number, dy: number,
  lenSq: number,
): number {
  if (lenSq === 0) return 0;
  return ((px - ax) * dx + (py - ay) * dy) / lenSq;
}

function radialGradientCoord(
  px: number, py: number,
  ax: number, ay: number,
  dx: number, dy: number,
  lenSq: number,
): number {
  if (lenSq === 0) return 0;
  const pdx = px - ax;
  const pdy = py - ay;
  const dist = Math.sqrt(pdx * pdx + pdy * pdy);
  const radius = Math.sqrt(lenSq);
  return radius > 0 ? dist / radius : 0;
}
