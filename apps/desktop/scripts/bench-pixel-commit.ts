// Baseline wall time for the commit READ side of the brush hot path:
// production `PaintTileSurface.readRect` is exactly one `ctx.getImageData` over
// a software canvas (paintTileSurface.ts:204), so this bench times that call
// against a REAL Cairo-backed canvas (the `canvas` devDependency, v3.2.3) -
// no mocked context anywhere.
//
// Scope: full-layer region N x N at N in {512, 2048, 4096}, 5 timed runs per
// size, no warmup (the cold first run is visible via min/max), median + p95
// recorded. NOT included: IPC/serde response serialization and the Rust
// write_region half (measured separately by
// `crates/core/tests/pixel_store_commit_bench.rs`).
//
// Run (from apps/desktop or repo root, either works):
//   bun scripts/bench-pixel-commit.ts        (or: node scripts/bench-pixel-commit.ts)
// Record machine class + engine in the baseline table next to the numbers.
//
// Falsifiability: PHOTREZ_BENCH_STUB_CANVAS=1 swaps getImageData for a
// constant-time stub. In that mode BOTH validity pins below must fail
// (pixel fidelity sees no real read, timer variance sees identical samples),
// which is what proves an unstubbed run is measuring a real engine.

import { createCanvas } from "canvas";
import os from "node:os";

const SIZES = [512, 2048, 4096] as const;
const RUNS = 5;
const STUB = process.env.PHOTREZ_BENCH_STUB_CANVAS === "1";
// Solid #123456 pre-paint: a real read returns 18,52,86 at pixel 0.
const FILL_R = 0x12;
const FILL_G = 0x34;
const FILL_B = 0x56;

interface ReadRectSample {
  median: number;
  p95: number;
  min: number;
  max: number;
  distinct: number;
}

function stats(samples: number[]): ReadRectSample {
  const sorted = [...samples].sort((a, b) => a - b);
  const at = (p: number): number =>
    sorted[Math.min(sorted.length - 1, Math.ceil(p * sorted.length) - 1)];
  return {
    median: at(0.5),
    p95: at(0.95),
    min: sorted[0],
    max: sorted[sorted.length - 1],
    distinct: new Set(samples).size,
  };
}

const failures: string[] = [];
const rows: string[] = [];

for (const n of SIZES) {
  const canvas = createCanvas(n, n);
  const ctx = canvas.getContext("2d");
  ctx.fillStyle = `#${FILL_R.toString(16)}${FILL_G.toString(16)}${FILL_B.toString(16)}`;
  ctx.fillRect(0, 0, n, n);

  const samples: number[] = [];
  let seen: Uint8ClampedArray | null = null;
  for (let run = 0; run < RUNS; run++) {
    if (STUB) {
      // Constant fake sample; no pixel read at all. Both pins must fail here.
      samples.push(0.01);
      continue;
    }
    const t0 = performance.now();
    const img = ctx.getImageData(0, 0, n, n);
    samples.push(performance.now() - t0);
    seen = img.data;
  }

  // Pin 1 - pixel fidelity: the timed read must return the painted pixels.
  // Stub mode never reads, so `seen` stays null and this fails by design.
  if (!seen || seen[0] !== FILL_R || seen[1] !== FILL_G || seen[2] !== FILL_B) {
    failures.push(
      `n=${n}: pixel0=(${seen ? `${seen[0]},${seen[1]},${seen[2]}` : "no read"}) expected ` +
        `(${FILL_R},${FILL_G},${FILL_B}) - getImageData is not a real engine read`,
    );
  }

  const s = stats(samples);
  // Pin 2 - timer variance: real runs jitter; identical samples mean a stub.
  if (s.distinct < 2) {
    failures.push(
      `n=${n}: ${s.distinct} distinct timing sample(s) out of ${RUNS} - timer looks stubbed`,
    );
  }

  rows.push(
    `READ_RECT n=${n} runs=${RUNS} median_ms=${s.median.toFixed(3)} ` +
      `p95_ms=${s.p95.toFixed(3)} min_ms=${s.min.toFixed(3)} max_ms=${s.max.toFixed(3)} ` +
      `distinct=${s.distinct}`,
  );
}

const cpu = os.cpus()[0]?.model ?? "unknown-cpu";
console.log(
  `READ_RECT_BASELINE engine=node-canvas-cairo-3.2.3 stub=${STUB} ` +
    `os=${os.platform()}-${os.arch()} cpu=${cpu} node=${process.versions.node}`,
);
console.log(rows.join("\n"));

if (failures.length > 0) {
  console.error(`BENCH VALIDITY FAILED:\n${failures.join("\n")}`);
  process.exit(1);
}
