// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Shape test for the dev-only perf audit harness. Drives runPerfAudit with tiny
// dims and stub runners, so no engine, GPU, or IPC is touched here.

import { afterEach, describe, expect, it, vi } from "vitest";
import { existsSync, readFileSync } from "node:fs";
import {
  PERF_AUDIT_OPS,
  defaultCloseScratch,
  defaultRunners,
  formatPerfAudit,
  runPerfAudit,
} from "../perfAuditDev";
import type { PerfAuditRow, PerfScratch, RowRunner } from "../perfAuditDev";

function stubCells(): RowRunner {
  return async () => ({
    totalMs: 1.5,
    invokeMs: -1,
    rasterMs: 2.5,
    uploadMs: -1,
    snapHistMs: -1,
    notes: "stub",
  });
}

function stubHarness() {
  const runners: Record<string, RowRunner> = {};
  for (const op of PERF_AUDIT_OPS) runners[op] = stubCells();
  let scratch: PerfScratch | null = null;
  return {
    runners,
    buildScratch: async (w: number, h: number): Promise<PerfScratch> => {
      scratch = { closed: false, width: w, height: h } as unknown as PerfScratch;
      return scratch;
    },
    closeScratch: async (s: PerfScratch): Promise<void> => {
      s.closed = true;
    },
    wasClosed: () => scratch?.closed ?? false,
  };
}

function isCell(value: unknown): boolean {
  return value === "ERROR" || (typeof value === "number" && Number.isFinite(value));
}

afterEach(() => {
  vi.restoreAllMocks();
});

describe("perf audit battery", () => {
  it("covers every expected row exactly once", () => {
    expect(Object.keys(defaultRunners).sort()).toEqual([...PERF_AUDIT_OPS].sort());
  });

  it("prints ONE table with numeric-or-ERROR cells and closes the scratch doc", async () => {
    const harness = stubHarness();
    const log = vi.spyOn(console, "log").mockImplementation(() => {});
    const rows = await runPerfAudit({
      dims: { w: 16, h: 12 },
      runners: harness.runners,
      buildScratch: harness.buildScratch,
      closeScratch: harness.closeScratch,
    });
    expect(log).toHaveBeenCalledTimes(1);
    expect(rows.map((r) => r.op).sort()).toEqual([...PERF_AUDIT_OPS].sort());
    for (const row of rows) {
      expect(isCell(row.totalMs)).toBe(true);
      expect(isCell(row.invokeMs)).toBe(true);
      expect(isCell(row.rasterMs)).toBe(true);
      expect(isCell(row.uploadMs)).toBe(true);
      expect(isCell(row.snapHistMs)).toBe(true);
      expect(typeof row.notes).toBe("string");
    }
    expect(harness.wasClosed()).toBe(true);
    const table = String(log.mock.calls[0][0]);
    for (const op of PERF_AUDIT_OPS) expect(table).toContain(op);
    expect(table).toContain("scratch: closed");
  });

    it("keeps going on a failing row and stays ASCII-only", async () => {
    const harness = stubHarness();
    harness.runners["gpu-composite"] = async () => {
      throw new Error("boom \u2013 caf\u00e9 \u2026");
    };
    const log = vi.spyOn(console, "log").mockImplementation(() => {});
    const rows = await runPerfAudit({
      dims: { w: 16, h: 12 },
      runners: harness.runners,
      buildScratch: harness.buildScratch,
      closeScratch: harness.closeScratch,
    });
    const failed = rows.find((r) => r.op === "gpu-composite");
    expect(failed?.totalMs).toBe("ERROR");
    expect(rows.filter((r) => r.op !== "gpu-composite").every((r) => r.totalMs !== "ERROR")).toBe(true);
    expect(harness.wasClosed()).toBe(true);
    const table = formatPerfAudit(rows as PerfAuditRow[], true, { w: 16, h: 12 });
    for (const ch of table) expect(ch.charCodeAt(0)).toBeLessThanOrEqual(127);
  });

  it("describes the battery in plain words with no internal report path", () => {
    const candidates = [
      "src/lib/perf/perfAuditDev.ts",
      "apps/desktop/src/lib/perf/perfAuditDev.ts",
    ];
    const found = candidates.find((p) => existsSync(p));
    expect(found).toBeDefined();
    const src = readFileSync(found as string, "utf8");
    expect(src).not.toContain("latency-audit");
  });

  it("only exposes the window entry point on dev builds", async () => {
    const mod = (await import("../perfAuditDev")) as unknown as {
      installPerfAuditWindow: (
        target: { __photrezPerfAudit?: unknown },
        isDev: boolean,
      ) => void;
    };
    const off: { __photrezPerfAudit?: unknown } = {};
    mod.installPerfAuditWindow(off, false);
    expect("__photrezPerfAudit" in off).toBe(false);
    const on: { __photrezPerfAudit?: unknown } = {};
    mod.installPerfAuditWindow(on, true);
    expect(typeof on.__photrezPerfAudit).toBe("function");
  });

  it("evicts the scratch facade entry and releases the per-instance mirror on close", async () => {
    const { getFacade, peekFacade } = await import("@/lib/protocol/selectionMirror");
    const docId = "perf-audit-scratch-leak-check";
    getFacade(docId);
    expect(peekFacade(docId)).toBeDefined();
    let freed = false;
    const scratch = {
      engine: { getId: () => docId, rustEngine: { free: () => { freed = true; } } },
      layerId: "l1",
      width: 4,
      height: 4,
      closed: false,
      bitmaps: [],
    } as unknown as PerfScratch;
    await defaultCloseScratch(scratch);
    expect(scratch.closed).toBe(true);
    expect(peekFacade(docId)).toBeUndefined();
    expect(freed).toBe(true);
  });

  it("disposes the GL backend after each measured GL row", async () => {
    const harness = stubHarness();
    harness.runners["upload-full-patch"] = defaultRunners["upload-full-patch"];
    harness.runners["gpu-composite"] = defaultRunners["gpu-composite"];
    let disposed = 0;
    const fakeGl = async () => ({
      uploadFull: () => {},
      uploadPatch: () => {},
      dispose: () => { disposed++; },
    });
    const log = vi.spyOn(console, "log").mockImplementation(() => {});
    const rows = await runPerfAudit({
      dims: { w: 8, h: 8 },
      runners: harness.runners,
      buildScratch: async (w, h) =>
        ({
          engine: {
            getId: () => "perf-audit-gl-check",
            getLayer: () => ({ imageBitmap: {} }),
          },
          layerId: "l1",
          width: w,
          height: h,
          closed: false,
          bitmaps: [],
        }) as unknown as PerfScratch,
      closeScratch: harness.closeScratch,
      makeGl: fakeGl,
    } as Parameters<typeof runPerfAudit>[0]);
    expect(rows.find((r) => r.op === "upload-full-patch")?.totalMs).not.toBe("ERROR");
    expect(rows.find((r) => r.op === "gpu-composite")?.totalMs).not.toBe("ERROR");
    expect(disposed).toBe(2);
  });
});
