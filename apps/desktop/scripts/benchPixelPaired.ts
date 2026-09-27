// Paired idle bench: one run = one timed `PaintTileSurface.readRect`-equivalent
// (`ctx.getImageData` on a real Cairo-backed canvas) immediately followed by one
// timed Rust `PixelStoreRegistry::write_region`, so both halves belong to the
// same run index instead of being percentiles of two separately-run
// distributions. Emits one JSONL record per run:
//   {"size":512,"run":0,"read_rect_ms":..,"write_region_ms":..,"combined_ms":..}
//
// Run (from repo root, on an idle machine):
//   bun apps/desktop/scripts/benchPixelPaired.ts --out <path.jsonl>
// The Rust half runs in a binary built from
// crates/core/tests/pixel_store_paired_sample.rs; this script resolves it once
// via `cargo test --release ... --no-run --message-format=json`, then spawns it
// per run (process start is outside the timed region).
//
// Excluded from the timer (disclosed, same as the component benches): IPC
// transport, serde response serialization, and the payload clone.
//
// Falsifiability: `PHOTREZ_BENCH_STUB_CANVAS=1 PHOTREZ_BENCH_PAIR_STUB=1`
// makes both halves constant, so validatePaired must abort with a non-zero
// exit; if that run succeeds the guards are vacuous.

import { createCanvas } from "canvas";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import {
  SIZES,
  pairRuns,
  summarize,
  validatePaired,
  type PairedRecord,
} from "./benchPixelPairedLib";

const SCRIPT_DIR = path.dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = path.resolve(SCRIPT_DIR, "..", "..", "..");
const SAMPLE_MARKER = "PAIR_SAMPLE ";

const RUNS = Number(process.env.PHOTREZ_BENCH_PAIR_RUNS ?? "15");

function outPathFromArgv(argv: string[]): string | null {
  const idx = argv.indexOf("--out");
  if (idx === -1) return null;
  const value = argv[idx + 1];
  if (!value) throw new Error("--out requires a file path");
  return value;
}

function resolveHelperExecutable(): string {
  const cargo = process.env.CARGO ?? "cargo";
  const build = spawnSync(
    cargo,
    [
      "test",
      "--release",
      "-p",
      "photrez-core",
      "--test",
      "pixel_store_paired_sample",
      "--no-run",
      "--message-format=json",
    ],
    { cwd: REPO_ROOT, encoding: "utf8", maxBuffer: 256 * 1024 * 1024 },
  );
  if (build.status !== 0) {
    const tail = (build.stderr ?? "").split(/\r?\n/).slice(-8).join("\n");
    throw new Error(`cargo test --no-run failed (status ${build.status})\n${tail}`);
  }
  for (const line of (build.stdout ?? "").split(/\r?\n/)) {
    if (!line.trim()) continue;
    let message: unknown;
    try {
      message = JSON.parse(line);
    } catch {
      continue;
    }
    const artifact = message as {
      reason?: string;
      target?: { name?: string };
      executable?: string | null;
    };
    if (
      artifact.reason === "compiler-artifact" &&
      artifact.target?.name === "pixel_store_paired_sample" &&
      artifact.executable
    ) {
      return artifact.executable;
    }
  }
  throw new Error("cargo reported no executable for pixel_store_paired_sample");
}

function runWriteSample(executable: string, size: number, run: number): number {
  // --nocapture is required: libtest swallows a passing test's stdout, which is
  // where the PAIR_SAMPLE record is printed.
  const result = spawnSync(executable, ["--nocapture"], {
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
    env: {
      ...process.env,
      PHOTREZ_BENCH_PAIR_N: String(size),
      PHOTREZ_BENCH_PAIR_RUN: String(run),
    },
  });
  if (result.status !== 0) {
    const tail = (result.stderr ?? "").split(/\r?\n/).slice(-8).join("\n");
    throw new Error(
      `write half failed for size=${size} run=${run} (status ${result.status})\n${tail}`,
    );
  }
  const line = (result.stdout ?? "")
    .split(/\r?\n/)
    .find((candidate) => candidate.startsWith(SAMPLE_MARKER));
  if (!line) {
    throw new Error(`write half missing: no ${SAMPLE_MARKER} line for size=${size} run=${run}`);
  }
  const sample = JSON.parse(line.slice(SAMPLE_MARKER.length)) as {
    size: number;
    run: number;
    write_region_ms: number;
  };
  if (sample.size !== size || sample.run !== run) {
    throw new Error(
      `pair mismatch: asked size=${size} run=${run}, helper answered size=${sample.size} run=${sample.run}`,
    );
  }
  return sample.write_region_ms;
}

function main(): void {
  if (!Number.isFinite(RUNS) || RUNS < 1) {
    throw new Error(`PHOTREZ_BENCH_PAIR_RUNS must be a positive integer, got ${RUNS}`);
  }
  const stubRead = process.env.PHOTREZ_BENCH_STUB_CANVAS === "1";
  const stubWrite = process.env.PHOTREZ_BENCH_PAIR_STUB === "1";
  const executable = resolveHelperExecutable();

  const records: PairedRecord[] = [];
  for (const size of SIZES) {
    const canvas = createCanvas(size, size);
    const ctx = canvas.getContext("2d");
    ctx.fillStyle = "#123456";
    ctx.fillRect(0, 0, size, size);

    const reads: number[] = [];
    const writes: number[] = [];
    for (let run = 0; run < RUNS; run++) {
      let readMs: number;
      let seen: Uint8ClampedArray | null = null;
      if (stubRead) {
        readMs = 0.01;
      } else {
        const started = performance.now();
        const image = ctx.getImageData(0, 0, size, size);
        readMs = performance.now() - started;
        seen = image.data;
      }
      if (seen && (seen[0] !== 0x12 || seen[1] !== 0x34 || seen[2] !== 0x56)) {
        throw new Error(
          `pixel fidelity failed at size=${size} run=${run}: got ${seen[0]},${seen[1]},${seen[2]} expected 18,52,86`,
        );
      }
      reads.push(readMs);
      writes.push(runWriteSample(executable, size, run));
    }
    records.push(...pairRuns(size, reads, writes));
  }

  validatePaired(records, RUNS, SIZES);

  const cpu = os.cpus()[0]?.model ?? "unknown-cpu";
  const header = [
    "PAIRED_BENCH",
    `engine=node-canvas-cairo-3.2.3`,
    `os=${os.platform()}-${os.arch()}`,
    `cpu=${cpu}`,
    `node=${process.versions.node}`,
    `runs_per_size=${RUNS}`,
    `stub_read=${stubRead}`,
    `stub_write=${stubWrite}`,
    `excluded=ipc,serde,payload_clone`,
  ].join(" ");

  const summaries = summarize(records);
  const lines = [
    header,
    ...records.map((record) => JSON.stringify(record)),
    ...summaries.map(
      (summary) =>
        `PAIRED_SUMMARY size=${summary.size} runs=${summary.n} ` +
        `median_ms=${summary.median_ms.toFixed(3)} p95_ms=${summary.p95_ms.toFixed(3)} ` +
        `budget_ms=${summary.budget_ms} verdict=${summary.verdict}`,
    ),
  ];
  const payload = `${lines.join("\n")}\n`;

  const outPath = outPathFromArgv(process.argv.slice(2));
  if (outPath) {
    fs.mkdirSync(path.dirname(outPath), { recursive: true });
    fs.writeFileSync(outPath, payload, "utf8");
  }
  process.stdout.write(payload);
}

try {
  main();
} catch (error) {
  const message = error instanceof Error ? error.message : String(error);
  process.stderr.write(`PAIRED BENCH VALIDITY FAILED:\n${message}\n`);
  process.exit(1);
}
