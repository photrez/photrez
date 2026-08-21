// Bench: Rust DocumentEngine vs TS DocumentEngine — full graph-op suite
// Run: bun run apps/desktop/src/engine/__tests__/document-rust-ssot.bench.ts
// Ops: add(100) select(100) delete(50) duplicate(50) reorder(100) selection(200) — 100-layer doc

import { DocumentEngine as TsEngine } from "../document";

async function benchRust() {
  const { getWasmExportModule } = await import("../../components/editor/wasmExport");
  const m: any = await getWasmExportModule();
  if (!m?.DocumentEngine) {
    console.log("Rust DocumentEngine not available — skip");
    return null;
  }
  const t0 = performance.now();
  const eng = new m.DocumentEngine("doc", "Bench", 800, 600);
  for (let i = 0; i < 100; i++) eng.add_layer(`l${i}`, `Layer ${i}`, 100, 100);
  for (let i = 0; i < 100; i++) eng.set_active_layer(`l${i % 100}`);
  for (let i = 0; i < 50; i++) eng.duplicate_layer(`l${i}`);
  for (let i = 0; i < 50; i++) eng.delete_layer(`l${i}`);
  for (let i = 0; i < 100; i++) eng.reorder_layer(i % 20, (i + 5) % 20);
  for (let i = 0; i < 100; i++) eng.set_selection(i, i, 10, 10, 0, "rect", false);
  for (let i = 0; i < 100; i++) eng.clear_selection();
  const t1 = performance.now();
  return t1 - t0;
}

function benchTs() {
  const t0 = performance.now();
  const eng: any = new TsEngine("doc", "Bench", 800, 600);
  for (let i = 0; i < 100; i++) eng.addLayer(`Layer ${i}`, 100, 100);
  const ids = () => eng.getLayers().map((l: any) => l.id);
  for (let i = 0; i < 100; i++) eng.setActiveLayer(ids()[i % ids().length] ?? null);
  for (let i = 0; i < 50 && ids().length > 1; i++) eng.duplicateLayer(ids()[i % ids().length]);
  for (let i = 0; i < 50 && ids().length > 1; i++) eng.deleteLayer(ids()[ids().length - 1]);
  for (let i = 0; i < 100 && ids().length > 2; i++) eng.reorderLayer(i % 20, (i + 5) % 20);
  for (let i = 0; i < 100; i++) eng.createSelection(i, i, 10, 10, 0, "rect");
  for (let i = 0; i < 100; i++) eng.clearSelection();
  const t1 = performance.now();
  return t1 - t0;
}

const iters = 20;
const rustTimes: number[] = [];
const tsTimes: number[] = [];

for (let i = 0; i < iters; i++) {
  const rt = await benchRust();
  if (rt !== null) rustTimes.push(rt);
  tsTimes.push(benchTs());
}

function stats(arr: number[]) {
  arr.sort((a, b) => a - b);
  return { med: arr[Math.floor(arr.length / 2)], mean: arr.reduce((a, b) => a + b, 0) / arr.length, min: arr[0] };
}

if (rustTimes.length) {
  const r = stats(rustTimes);
  const t = stats(tsTimes);
  console.log(`\nDocumentEngine graph ops x550 (add100+select100+dup50+del50+reorder100+sel200), median of ${iters}:`);
  console.log(`  Rust median=${r.med.toFixed(2)}ms mean=${r.mean.toFixed(2)}ms min=${r.min.toFixed(2)}ms`);
  console.log(`  TS   median=${t.med.toFixed(2)}ms mean=${t.mean.toFixed(2)}ms min=${t.min.toFixed(2)}ms`);
  const ratio = t.med / r.med;
  console.log(`  Rust vs TS: ${ratio.toFixed(2)}× ${r.med < t.med ? "(Rust faster)" : "(TS faster)"}`);
} else {
  const t = stats(tsTimes);
  console.log(`TS median=${t.med.toFixed(2)}ms (Rust not available)`);
}
