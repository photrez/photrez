// @ts-nocheck
// Bench round 2: remaining CPU pixel loops (R2 selection ops + R3 brush mask).
// Mirrors production loops VERBATIM from:
//   - src/features/selection/SelectionOperations.ts  trimTransparent (~L179),
//     inverted-ellipse mask-out loop (~L137)
//   - src/components/editor/brushTipMask.ts          stampBrushTip (~L260),
//     compositeMaskToImageDataDirty (~L502)
// Run: bun run apps/desktop/scripts/bench-cpu-pixel-round2.ts
// Gates (from docs/plans/2026-08-21-rust-gpu-benchmark-matrix.md):
//   R2: matters only if >50ms at 4K (one-shot action)
//   R3: NO-GO if total per-stroke-frame CPU < ~2ms; port if >5ms

function median(arr) {
  const s = [...arr].sort((a, b) => a - b);
  return s[Math.floor(s.length / 2)];
}

function benchIters(name, iters, fn) {
  // warmup x2
  fn(); fn();
  const times = [];
  for (let i = 0; i < iters; i++) {
    const t0 = performance.now();
    fn();
    times.push(performance.now() - t0);
  }
  return { name, med: median(times), min: Math.min(...times) };
}

const fmt = (ms) => `${ms.toFixed(2)}ms`;

// ── R2a: trimTransparent — bbox scan + conditional copy, 4K layer ───────────
// Source mirror: SelectionOperations.trimTransparent L179-225.
function makeImageShim(w, h, opaqueRatio) {
  const data = new Uint8ClampedArray(w * h * 4);
  for (let i = 0; i < w * h; i++) {
    // opaqueRatio of pixels non-transparent, scattered like real artwork
    data[i * 4 + 3] = (i % 1000) / 1000 < opaqueRatio ? 255 : 0;
  }
  return { data, width: w, height: h };
}

function trimTransparent(imageData) {
  const { width, height, data: pixels } = imageData;
  let top = height, bottom = 0, left = width, right = 0;
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      if (pixels[(y * width + x) * 4 + 3] > 0) {
        if (y < top) top = y;
        if (y > bottom) bottom = y;
        if (x < left) left = x;
        if (x > right) right = x;
      }
    }
  }
  if (top > bottom) return imageData;
  const trimmedW = right - left + 1;
  const trimmedH = bottom - top + 1;
  if (trimmedW === width && trimmedH === height) return imageData;
  const trimmedData = new Uint8ClampedArray(trimmedW * trimmedH * 4);
  for (let y = 0; y < trimmedH; y++) {
    for (let x = 0; x < trimmedW; x++) {
      const srcIdx = ((top + y) * width + (left + x)) * 4;
      const dstIdx = (y * trimmedW + x) * 4;
      for (let c = 0; c < 4; c++) trimmedData[dstIdx + c] = pixels[srcIdx + c];
    }
  }
  return { data: trimmedData, width: trimmedW, height: trimmedH };
}

// ── R2b: inverted-selection ellipse mask-out, 4K ─────────────────────────────
// Source mirror: copySelection L137-143 + isInsideEllipse math.
function maskOutsideEllipse(data, W, H) {
  // ellipse covering central half of canvas (like a real marquee)
  const cx = W / 2, cy = H / 2, rx = W / 4, ry = H / 4;
  for (let py = 0; py < H; py++) {
    const ny = (py - cy) / ry;
    for (let px = 0; px < W; px++) {
      const nx = (px - cx) / rx;
      if (nx * nx + ny * ny <= 1) {
        data[(py * W + px) * 4 + 3] = 0;
      }
    }
  }
}

// ── R3a: stampBrushTip — bilinear tip resample + accumulate, N dabs ─────────
// Source mirror: brushTipMask.stampBrushTip L260-333.
function makeTip(diameter) {
  const dataSize = 64; // tips are precomputed at fixed resolution
  const data = new Float32Array(dataSize * dataSize);
  for (let y = 0; y < dataSize; y++)
    for (let x = 0; x < dataSize; x++) {
      const nx = (x - dataSize / 2) / (dataSize / 2);
      const ny = (y - dataSize / 2) / (dataSize / 2);
      const d = Math.sqrt(nx * nx + ny * ny);
      data[y * dataSize + x] = d <= 1 ? Math.pow(1 - d, 0.8) : 0;
    }
  return { diameter, dataSize, data };
}

function clamp01(v) { return v < 0 ? 0 : v > 1 ? 1 : v; }

function stampBrushTip(mask, maskWidth, maskHeight, tip, centerX, centerY, alphaScale) {
  const aScale = clamp01(alphaScale);
  const halfExtent = tip.diameter / 2;
  const centerIndex = halfExtent - 0.5;
  const dataSize = tip.dataSize;
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
}

// ── R3b: compositeMaskToImageDataDirty — straight-alpha over, dirty rect ────
// Source mirror: brushTipMask.compositeMaskToImageDataDirty L502-545.
function compositeDirty(imageData, originX, originY, mask, maskWidth, rect, color, isEraser) {
  const data = imageData.data;
  const imgW = imageData.width;
  const paint = color;
  const strokeAlpha = paint.a;
  for (let y = rect.y0; y < rect.y1; y++) {
    const rowInMask = y * maskWidth;
    const rowInImage = (y - originY) * imgW;
    for (let x = rect.x0; x < rect.x1; x++) {
      const maskAlpha = mask[rowInMask + x] / 255;
      if (maskAlpha <= 0) continue;
      const i = (rowInImage + (x - originX)) << 2;
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

// ═════════════════════════ RUN ═════════════════════════
console.log("== R2: Selection ops (one-shot gate: >50ms @ 4K matters) ==");
{
  const W = 4000, H = 3000;
  const img = makeImageShim(W, H, 0.6); // realistic: ~60% opaque scatter
  const r = benchIters("trimTransparent 4K bbox scan", 5, () => trimTransparent(img));
  console.log(`  ${r.name.padEnd(38)} med=${fmt(r.med)}`);

  const img2 = new Uint8ClampedArray(W * H * 4).fill(200);
  const r2 = benchIters("inverted-ellipse mask-out 4K", 5, () => maskOutsideEllipse(img2, W, H));
  console.log(`  ${r2.name.padEnd(38)} med=${fmt(r2.med)}`);
}

console.log("\n== R3: Brush per-dab cost (gate: stroke frame <2ms fluid, >5ms port) ==");
{
  const CANVAS_W = 1920, CANVAS_H = 1080;
  const strokes = [
    { label: "small d=32px",  tip: makeTip(32) },
    { label: "medium d=128px", tip: makeTip(128) },
    { label: "huge d=512px",  tip: makeTip(512) },
  ];
  const DABS = 500; // ~ one fast stroke at 60fps over ~8s, worst-case burst
  for (const { label, tip } of strokes) {
    const mask = new Uint8ClampedArray(CANVAS_W * CANVAS_H);
    const rs = benchIters(`stamp ${label} x${DABS}`, 3, () => {
      for (let d = 0; d < DABS; d++)
        stampBrushTip(mask, CANVAS_W, CANVAS_H, tip, 960 + (d % 64), 540 + (d % 64), 0.5);
    });
    // composite: worst case dab-sized dirty rects
    const sz = tip.diameter;
    const imgData = { data: new Uint8ClampedArray(sz * sz * 4), width: sz, height: sz };
    const rect = { x0: 0, y0: 0, x1: sz, y1: sz };
    const cmask = new Uint8ClampedArray(sz * sz).fill(180);
    const rc = benchIters(`composite ${label} x${DABS}`, 3, () => {
      for (let d = 0; d < DABS; d++)
        compositeDirty(imgData, 0, 0, cmask, sz, rect, { r: 200, g: 80, b: 40, a: 0.7 }, false);
    });
    const perDab = (rs.med + rc.med) / DABS;
    const dabBudgetPerFrame = 16.6 / perDab; // 60fps frame budget
    console.log(
      `  ${label.padEnd(16)} stamp=${fmt(rs.med)} composite=${fmt(rc.med)} | ${(perDab * 1000).toFixed(0)}µs/dab → ${dabBudgetPerFrame.toFixed(0)} dabs/frame @60fps`,
    );
  }
}
