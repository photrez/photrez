import { describe, expect, it } from "vitest";
import {
  BUDGET_MS,
  pairRuns,
  percentile,
  summarize,
  validatePaired,
  verdictFor,
  type PairedRecord,
} from "../../../scripts/benchPixelPairedLib";

// Paired bench guards: one read half and one write half must land in the same
// record, and the recorded p95 must be a percentile of the per-run combined
// distribution - never the sum of two separately computed component percentiles.

function vary(count: number, base: number, step: number): number[] {
  return Array.from({ length: count }, (_, i) => base + i * step + (i % 3) * 0.07);
}

describe("benchPixelPaired pairing", () => {
  it("pairs a read half with the write half of the same run index", () => {
    const records = pairRuns(512, [1.5, 2.5], [10, 20]);
    expect(records).toEqual([
      { size: 512, run: 0, read_rect_ms: 1.5, write_region_ms: 10, combined_ms: 11.5 },
      { size: 512, run: 1, read_rect_ms: 2.5, write_region_ms: 20, combined_ms: 22.5 },
    ]);
  });

  it("throws when the write half is missing", () => {
    expect(() => pairRuns(512, [1, 2, 3], [])).toThrow(/write half missing/i);
  });

  it("throws when the read half is missing", () => {
    expect(() => pairRuns(512, [], [1, 2, 3])).toThrow(/read half missing/i);
  });

  it("throws when the two halves have different lengths", () => {
    expect(() => pairRuns(512, [1, 2, 3], [1, 2])).toThrow(/half missing/i);
  });
});

describe("benchPixelPaired guards", () => {
  const good = (): PairedRecord[] =>
    pairRuns(512, vary(5, 1, 0.31), vary(5, 3, 0.77));

  it("accepts five distinct paired records for one size", () => {
    expect(() => validatePaired(good(), 5, [512])).not.toThrow();
  });

  it("throws when every timing is identical (stubbed timer)", () => {
    const stubbed = pairRuns(512, [0.01, 0.01, 0.01, 0.01, 0.01], [0.5, 0.5, 0.5, 0.5, 0.5]);
    expect(() => validatePaired(stubbed, 5, [512])).toThrow(/identical/i);
  });

  it("throws when only the write half is stubbed to a constant", () => {
    const halfStubbed = pairRuns(512, vary(5, 1, 0.4), [0.5, 0.5, 0.5, 0.5, 0.5]);
    expect(() => validatePaired(halfStubbed, 5, [512])).toThrow(/write_region_ms identical/i);
  });

  it("throws when a size has fewer paired records than required", () => {
    expect(() => validatePaired(good().slice(0, 3), 5, [512])).toThrow(/paired records/i);
  });

  it("throws when an expected size is absent entirely", () => {
    expect(() => validatePaired(good(), 5, [512, 2048, 4096])).toThrow(/missing size/i);
  });

  it("throws on a non-finite timing", () => {
    const broken = good();
    broken[2] = { ...broken[2], combined_ms: Number.NaN };
    expect(() => validatePaired(broken, 5, [512])).toThrow(/non-finite/i);
  });

  it("throws on a zero or negative timing", () => {
    const broken = good();
    broken[0] = { ...broken[0], write_region_ms: 0 };
    expect(() => validatePaired(broken, 5, [512])).toThrow(/non-positive/i);
  });
});

describe("benchPixelPaired summary", () => {
  it("takes median and p95 from the combined distribution, not from summed component percentiles", () => {
    // 20 runs: one read outlier sits mid-distribution so the p95 of combined
    // (20 ms) cannot be reconstructed by adding the component p95s (1 + 18 = 19).
    const reads = [...Array(5).fill(1), 100, ...Array(14).fill(1)];
    const writes = Array.from({ length: 20 }, (_, i) => i);
    const records = pairRuns(512, reads as number[], writes);

    const [summary] = summarize(records);
    expect(summary.n).toBe(20);
    expect(summary.median_ms).toBe(11);
    expect(summary.p95_ms).toBe(20);
    expect(percentile([...reads].sort((a, b) => a - b), 0.95)).toBe(1);
    expect(
      percentile([...reads].sort((a, b) => a - b), 0.95) +
        percentile([...writes].sort((a, b) => a - b), 0.95),
    ).toBe(19);
    expect(summary.p95_ms).not.toBe(19);
  });

  it("keeps each size separate and carries its budget", () => {
    const records = [
      ...pairRuns(512, vary(5, 1, 0.3), vary(5, 2, 0.9)),
      ...pairRuns(2048, vary(5, 10, 3), vary(5, 20, 9)),
      ...pairRuns(4096, vary(5, 40, 6), vary(5, 60, 11)),
    ];
    const summaries = summarize(records);
    expect(summaries.map((s) => s.size)).toEqual([512, 2048, 4096]);
    expect(summaries.map((s) => s.budget_ms)).toEqual([BUDGET_MS[512], BUDGET_MS[2048], BUDGET_MS[4096]]);
    expect(summaries.every((s) => s.n === 5)).toBe(true);
  });

  it("verdict compares measured p95 against the budget edge exactly", () => {
    expect(verdictFor(27, 27)).toBe("WITHIN");
    expect(verdictFor(27.001, 27)).toBe("ABOVE");
    expect(verdictFor(5.56, 27)).toBe("WITHIN");
  });
});
