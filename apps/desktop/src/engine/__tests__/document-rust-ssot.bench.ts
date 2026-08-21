// Bench: Rust DocumentEngine vs TS DocumentEngine for 100-layer add/select/undo
// Run: bun run apps/desktop/src/engine/__tests__/document-rust-ssot.bench.ts
// (uses relative imports so it works in bun/deno without Vite alias)

import { DocumentEngine as TsEngine } from "../document";

async function benchRust() {
  const { getWasmExportModule } = await import("../../components/editor/wasmExport");
  const m: any = await getWasmExportModule();
  if (!m?.DocumentEngine) {
    console.log("Rust DocumentEngine not available (headless) — skip");
    return null;
  }
  const t0 = performance.now();
  const eng = new m.DocumentEngine("doc", "Bench", 800, 600);
  for (let i = 0; i < 100; i++) eng.add_layer(`l${i}`, `Layer ${i}`, 100, 100);
  for (let i = 0; i < 100; i++) eng.set_active_layer(`l${i % 100}`);
  const t1 = performance.now();
  return t1 - t0;
}

function benchTs() {
  const t0 = performance.now();
  const eng = new TsEngine("doc", "Bench", 800, 600);
  for (let i = 0; i < 100; i++) eng.addLayer(`Layer ${i}`, 100, 100);
  for (let i = 0; i < 100; i++) eng.setActiveLayer(eng.getLayers()[i % eng.getLayers().length]?.id ?? null);
  const t1 = performance.now();
  return t1 - t0;
}

const iters = 10;
const rustTimes: number[] = [];
const tsTimes: number[] = [];

for (let i = 0; i < iters; i++) {
  const rt = await benchRust();
  if (rt !== null) rustTimes.push(rt);
  tsTimes.push(benchTs());
}

function stats(arr: number[]) {
  arr.sort((a, b) => a - b);
  const med = arr[Math.floor(arr.length / 2)];
  const mean = arr.reduce((a, b) => a + b, 0) / arr.length;
  return { med, mean, min: arr[0] };
}

if (rustTimes.length) {
  const r = stats(rustTimes);
  const t = stats(tsTimes);
  console.log(`\nDocumentEngine 100-layer add/select/undo (median over ${iters} runs):`);
  console.log(`  Rust median=${r.med.toFixed(2)}ms mean=${r.mean.toFixed(2)}ms`);
  console.log(`  TS   median=${t.med.toFixed(2)}ms mean=${t.mean.toFixed(2)}ms`);
  console.log(`  Rust vs TS: ${(t.med / r.med).toFixed(2)}× ${r.med < t.med ? "(Rust faster)" : "(TS faster)"}`);
  // Update research doc decision threshold: Rust within 0.5×–2× of TS is OK for SSOT
  const ratio = t.med / r.med;
  if (ratio >= 0.5 && ratio <= 2) console.log("Decision: keep Rust SSOT (within 0.5×–2×, SSOT benefit outweighs)");
  else if (ratio < 0.5) console.log("Decision: Rust significantly faster — keep Rust");
  else console.log("Decision: Rust >2× slower — keep TS for graph, document why");
} else {
  const t = stats(tsTimes);
  console.log(`TS median=${t.med.toFixed(2)}ms (Rust not available)`);
}
