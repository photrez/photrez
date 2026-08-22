// @ts-nocheck
// R2 follow-up: measure the PROPOSED JS micro-fix (row-wise TypedArray.set copy)
// vs the original per-channel copy, plus a two-phase scan variant.
// Gate: full trim op should land <50ms @4K to keep the NO-GO-port verdict.
// Run: bun run apps/desktop/scripts/bench-r2-microfix.ts
function median(a) { const s = [...a].sort((x, y) => x - y); return s[Math.floor(s.length / 2)]; }
function benchIters(iters, fn) {
  fn(); fn();
  const ts = [];
  for (let i = 0; i < iters; i++) { const t0 = performance.now(); fn(); ts.push(performance.now() - t0); }
  return median(ts);
}
function makeImage(w, h, opaqueRatio) {
  const data = new Uint8ClampedArray(w * h * 4);
  for (let i = 0; i < w * h; i++) data[i * 4 + 3] = (i % 1000) / 1000 < opaqueRatio ? 255 : 0;
  return { data, width: w, height: h };
}
const W = 4000, H = 3000;
const img = makeImage(W, H, 0.6);

// ORIGINAL production shape: bbox scan + per-channel inner-loop copy
function trimOriginal(imageData) {
  const { width, height, data: px } = imageData;
  let top = height, bottom = 0, left = width, right = 0;
  for (let y = 0; y < height; y++)
    for (let x = 0; x < width; x++)
      if (px[(y * width + x) * 4 + 3] > 0) {
        if (y < top) top = y;
        if (y > bottom) bottom = y;
        if (x < left) left = x;
        if (x > right) right = x;
      }
  if (top > bottom) return imageData;
  const tw = right - left + 1, th = bottom - top + 1;
  if (tw === width && th === height) return imageData;
  const out = new Uint8ClampedArray(tw * th * 4);
  for (let y = 0; y < th; y++)
    for (let x = 0; x < tw; x++) {
      const s = ((top + y) * width + (left + x)) * 4, d = (y * tw + x) * 4;
      for (let c = 0; c < 4; c++) out[d + c] = px[s + c];
    }
  return { data: out, width: tw, height: th };
}

// MICRO-FIX v2 (EXACT): identical exhaustive scan, only the copy becomes
// row-wise TypedArray.set (memcpy path). Semantics preserved.
function trimRowSet(imageData) {
  const { width, height, data: px } = imageData;
  let top = height, bottom = 0, left = width, right = 0;
  for (let y = 0; y < height; y++)
    for (let x = 0; x < width; x++)
      if (px[(y * width + x) * 4 + 3] > 0) {
        if (y < top) top = y;
        if (y > bottom) bottom = y;
        if (x < left) left = x;
        if (x > right) right = x;
      }
  if (top > bottom) return imageData;
  const tw = right - left + 1, th = bottom - top + 1;
  if (tw === width && th === height) return imageData;
  const out = new Uint8ClampedArray(tw * th * 4);
  for (let y = 0; y < th; y++) {
    const s = ((top + y) * width + left) * 4;
    out.set(px.subarray(s, s + tw * 4), y * tw * 4);
  }
  return { data: out, width: tw, height: th };
}

// PROJECTION VARIANT (industry-standard autocrop shape): find top via
// top-down row scan (early exit), bottom via bottom-up, then refine
// left/right within [top,bottom]. Exact semantics preserved.
function trimProjection(imageData) {
  const { width, height, data: px } = imageData;
  const rowHasOpaque = (y) => {
    const row = y * width;
    for (let x = 0; x < width; x++) if (px[(row + x) * 4 + 3] > 0) return true;
    return false;
  };
  let top = -1;
  for (let y = 0; y < height; y++) if (rowHasOpaque(y)) { top = y; break; }
  if (top === -1) return imageData;
  let bottom = height - 1;
  while (bottom > top && !rowHasOpaque(bottom)) bottom--;
  let left = width, right = -1;
  for (let y = top; y <= bottom; y++) {
    const row = y * width;
    let rowL = width, rowR = -1;
    for (let x = 0; x < width; x++) {
      if (px[(row + x) * 4 + 3] > 0) { if (x < rowL) rowL = x; rowR = x; }
    }
    if (rowL < left) left = rowL;
    if (rowR > right) right = rowR;
  }
  const tw = right - left + 1, th = bottom - top + 1;
  if (tw === width && th === height && left === 0 && top === 0) return imageData;
  const out = new Uint8ClampedArray(tw * th * 4);
  for (let y = 0; y < th; y++) {
    const s = ((top + y) * width + left) * 4;
    out.set(px.subarray(s, s + tw * 4), y * tw * 4);
  }
  return { data: out, width: tw, height: th };
}

const r1 = benchIters(5, () => trimOriginal(img));
const r2 = benchIters(5, () => trimRowSet(img));
const r3 = benchIters(5, () => trimProjection(img));
const a = trimOriginal(img), b = trimRowSet(img), c = trimProjection(img);
let byteParity = true;
for (let i = 0; i < a.data.length; i++) {
  if (a.width !== b.width || a.height !== b.height || a.data[i] !== b.data[i]) { byteParity = false; break; }
}
const projParity = c.width === a.width && c.height === a.height &&
  c.data.every((v, i) => v === a.data[i]);
console.log(`trimOriginal  (scan+per-channel copy) : ${r1.toFixed(2)}ms`);
console.log(`trimRowSet    (exact scan + row set ) : ${r2.toFixed(2)}ms`);
console.log(`trimProjection(top/bottom early-exit): ${r3.toFixed(2)}ms`);
console.log(`rowset-vs-original=${(r1 / r2).toFixed(2)}x  proj-vs-original=${(r1 / r3).toFixed(2)}x`);
console.log(`byte-parity rowset=${byteParity} proj=${projParity} (${b.width}x${b.height})`);
const best = Math.min(r2, r3);
console.log(best < 50 ? "VERDICT: best exact CPU variant UNDER 50ms gate -> NO-GO port confirmed" : "VERDICT: still over gate");
