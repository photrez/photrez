// Pure helpers for the paired readRect + write_region bench
// (`benchPixelPaired.ts` drives the two halves; this file holds only the
// pairing, guard, and summary logic so it can be unit-tested without loading
// the native canvas module).
//
// Contract: a paired record exists only when BOTH halves of one run landed.
// The recorded median/p95 are percentiles of the per-run `combined_ms`
// distribution; deriving them from separately computed component percentiles
// is a method error and is asserted against by the unit test.

export interface PairedRecord {
  size: number;
  run: number;
  read_rect_ms: number;
  write_region_ms: number;
  combined_ms: number;
}

export interface SizeSummary {
  size: number;
  n: number;
  median_ms: number;
  p95_ms: number;
  budget_ms: number;
  verdict: "WITHIN" | "ABOVE";
}

export const SIZES = [512, 2048, 4096] as const;
export const BUDGET_MS: Record<number, number> = { 512: 27, 2048: 672, 4096: 2535 };

export function pairRuns(size: number, readMs: number[], writeMs: number[]): PairedRecord[] {
  if (writeMs.length === 0) {
    throw new Error(`write half missing: 0 write_region_ms samples for size=${size}`);
  }
  if (readMs.length === 0) {
    throw new Error(`read half missing: 0 read_rect_ms samples for size=${size}`);
  }
  if (readMs.length !== writeMs.length) {
    throw new Error(
      `half missing: read=${readMs.length} write=${writeMs.length} paired samples for size=${size}`,
    );
  }
  return readMs.map((readRectMs, run) => ({
    size,
    run,
    read_rect_ms: readRectMs,
    write_region_ms: writeMs[run],
    combined_ms: readRectMs + writeMs[run],
  }));
}

export function percentile(sorted: number[], p: number): number {
  if (sorted.length === 0) {
    throw new Error("percentile of an empty distribution");
  }
  const idx = Math.max(0, Math.ceil(p * sorted.length) - 1);
  return sorted[Math.min(sorted.length - 1, idx)];
}

export function validatePaired(
  records: PairedRecord[],
  minPerSize: number,
  expectedSizes: readonly number[],
): void {
  if (records.length === 0) {
    throw new Error("no paired records emitted");
  }
  for (const record of records) {
    const fields: [string, number][] = [
      ["read_rect_ms", record.read_rect_ms],
      ["write_region_ms", record.write_region_ms],
      ["combined_ms", record.combined_ms],
    ];
    for (const [key, value] of fields) {
      if (!Number.isFinite(value)) {
        throw new Error(`size=${record.size} run=${record.run}: ${key} is non-finite (${value})`);
      }
      if (value <= 0) {
        throw new Error(`size=${record.size} run=${record.run}: ${key} is non-positive (${value})`);
      }
    }
  }

  for (const size of expectedSizes) {
    const atSize = records.filter((r) => r.size === size);
    if (atSize.length === 0) {
      throw new Error(`missing size=${size}: no paired records`);
    }
    if (atSize.length < minPerSize) {
      throw new Error(
        `size=${size}: ${atSize.length} paired records, need >=${minPerSize}`,
      );
    }
    const keys: (keyof PairedRecord)[] = ["read_rect_ms", "write_region_ms", "combined_ms"];
    for (const key of keys) {
      const distinct = new Set(atSize.map((r) => r[key])).size;
      if (distinct < 2) {
        throw new Error(
          `size=${size}: all ${key} identical across ${atSize.length} runs - one half looks stubbed`,
        );
      }
    }
  }
}

export function summarize(records: PairedRecord[]): SizeSummary[] {
  const sizes = [...new Set(records.map((r) => r.size))];
  return sizes.map((size) => {
    const combined = records
      .filter((r) => r.size === size)
      .map((r) => r.combined_ms)
      .sort((a, b) => a - b);
    const budget = BUDGET_MS[size];
    const p95 = percentile(combined, 0.95);
    return {
      size,
      n: combined.length,
      median_ms: percentile(combined, 0.5),
      p95_ms: p95,
      budget_ms: budget,
      verdict: verdictFor(p95, budget),
    };
  });
}

export function verdictFor(p95: number, budget: number): "WITHIN" | "ABOVE" {
  return p95 <= budget ? "WITHIN" : "ABOVE";
}
