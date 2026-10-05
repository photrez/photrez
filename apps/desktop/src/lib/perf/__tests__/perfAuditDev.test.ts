// SPDX-License-Identifier: AGPL-3.0-or-later
//
// Shape test for the dev-only perf audit harness. Drives runPerfAudit with tiny
// dims and stub runners, so no engine, GPU, or IPC is touched here.
//
// The byte-accounting block is the one place a REAL runner is driven through the
// real entry point, because the property under test is wiring: that `runPerfAudit`
// actually reaches the read-only byte probe with the document id it opened, and
// that a Rust rejection becomes a visible row instead of an `undefined` cell.
// Its transport is a mock, but the mock REPLICATES the real one - Tauri v2 invoke
// REJECTS with the bare string carried by a Rust `Err(String)`, and RESOLVES with
// the payload - so a resolved `{ ok: false }` here would prove nothing.

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
import { invoke } from "@tauri-apps/api/core";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;

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

/**
 * Byte accounting. The probe (`rust_pixels_store_bytes`) is read-only and returns
 * no pixels, so the only mock the harness needs is the transport - and the mock
 * has to be faithful about FAILURE (bare-string rejection), because a harness that
 * renders `undefined` on a rejected read is worse than one that reports nothing.
 */
const BYTE_REPORT_4PX = {
  layer_count: 1,
  row_major_bytes: 64,
  tile_graph: {
    total_bytes: 320,
    shared_bytes: 192,
    private_bytes: 128,
    tile_count: 5,
    state_count: 3,
    tile_reference_count: 7,
  },
  total_bytes: 384,
  owed_anchor_bytes: 0,
  owed_anchor_layer_count: 0,
};

/** Faithful transport: resolves payloads, REJECTS with the bare Rust `Err(String)`. */
function routeInvoke(
  onProbe: () => unknown,
  calls: { cmd: string; args: Record<string, unknown> }[],
): void {
  invokeMock.mockImplementation(async (cmd: string, args: Record<string, unknown> = {}) => {
    calls.push({ cmd, args });
    if (cmd === "rust_pixels_store_bytes") return onProbe();
    if (cmd === "apply_tile_patch") return { tiles: [], epoch: 1, version: 1 };
    if (cmd === "paint_parity_autorun_enabled") return true;
    return null;
  });
}

/** The real byte-accounting runner, driven through the real entry point. */
function byteHarness() {
  const runners: Record<string, RowRunner> = {};
  for (const op of PERF_AUDIT_OPS) runners[op] = stubCells();
  const real = defaultRunners["byte-accounting"];
  if (!real) throw new Error("byte-accounting runner is missing");
  runners["byte-accounting"] = real;
  return {
    runners,
    buildScratch: async (w: number, h: number) =>
      ({ closed: false, width: w, height: h }) as unknown as PerfScratch,
    closeScratch: async (s: PerfScratch) => {
      s.closed = true;
    },
  };
}

function byteOptions(h: ReturnType<typeof byteHarness>, byteSizes: number[], byteCommits: number) {
  return {
    dims: { w: 16, h: 12 },
    runners: h.runners,
    buildScratch: h.buildScratch,
    closeScratch: h.closeScratch,
    byteSizes,
    byteCommits,
  } as Parameters<typeof runPerfAudit>[0];
}

describe("perf audit byte accounting", () => {
  it("reads the probe for the document it opened, once per size and stage", async () => {
    const calls: { cmd: string; args: Record<string, unknown> }[] = [];
    routeInvoke(() => BYTE_REPORT_4PX, calls);
    const h = byteHarness();
    const log = vi.spyOn(console, "log").mockImplementation(() => {});
    const rows = await runPerfAudit(byteOptions(h, [4, 4], 1));
    const probeCalls = calls.filter((c) => c.cmd === "rust_pixels_store_bytes");
    // Two sizes, and per size four stages (seeded / after-commits /
    // after-full-layer / control-full-layer-only): the sharing ratio only means
    // something across a run that commits, and only the control reads zero, so a
    // run without either would be a measurement nobody can interpret.
    expect(probeCalls.length).toBe(8);
    // The doc id must be the one THIS run opened, not the scratch engine's id. All
    // the measured documents are driven first, then the controls, so the two
    // families of ids stay separable in the sequence.
    expect(probeCalls.map((c) => c.args.docId)).toEqual([
      "perf-audit-bytes-4",
      "perf-audit-bytes-4",
      "perf-audit-bytes-4",
      "perf-audit-bytes-4",
      "perf-audit-bytes-4",
      "perf-audit-bytes-4",
      "perf-audit-bytes-control-4",
      "perf-audit-bytes-control-4",
    ]);
    const row = rows.find((r) => r.op === "byte-accounting");
    expect(row?.bytes).toHaveLength(8);
    expect(row?.bytes?.[0]).toMatchObject({
      doc_id: "perf-audit-bytes-4",
      stage: "seeded",
      commits: 0,
      layer_count: 1,
      row_major_bytes: 64,
      tile_total_bytes: 320,
      shared_bytes: 192,
      private_bytes: 128,
      tile_count: 5,
      state_count: 3,
      tile_reference_count: 7,
      owed_anchor_bytes: 0,
      owed_anchor_layer_count: 0,
    });
    expect(typeof row?.bytes?.[0]?.probe_ms).toBe("number");
    expect(String(log.mock.calls[0][0])).toContain("byte-accounting");
  });

  /**
   * The seeded read is the one that can be misread: its tile graph is genuinely
   * empty, but empty because the canon has not been built yet. The row has to
   * carry the figure AND say so in the printed block, or a half-built store
   * prints like a cheap one.
   */
  it("carries the owed tile graph on the seeded read and names it in the table", async () => {
    const calls: { cmd: string; args: Record<string, unknown> }[] = [];
    const owed = {
      ...BYTE_REPORT_4PX,
      row_major_bytes: 1_048_576,
      tile_graph: {
        total_bytes: 0,
        shared_bytes: 0,
        private_bytes: 0,
        tile_count: 0,
        state_count: 0,
        tile_reference_count: 0,
      },
      total_bytes: 1_048_576,
      owed_anchor_bytes: 1_048_576,
      owed_anchor_layer_count: 1,
    };
    let first = true;
    routeInvoke(() => {
      // The FIRST read of a document is the seeded one; the rest are committed.
      if (first) {
        first = false;
        return owed;
      }
      return BYTE_REPORT_4PX;
    }, calls);
    const h = byteHarness();
    const log = vi.spyOn(console, "log").mockImplementation(() => {});
    const rows = await runPerfAudit(byteOptions(h, [4], 1));
    const samples = rows.find((r) => r.op === "byte-accounting")?.bytes ?? [];
    const seeded = samples[0];
    expect(seeded?.stage).toBe("seeded");
    expect(seeded?.owed_anchor_layer_count).toBe(1);
    expect(seeded?.owed_anchor_bytes).toBe(1_048_576);
    expect(seeded?.tile_total_bytes).toBe(0);
    const table = String(log.mock.calls[0][0]);
    expect(table).toContain("NOT YET BUILT");
    expect(table).toContain("owes_tile_graph=1048576");
    // A later, committed read must NOT keep claiming to owe one.
    expect(table).toContain("owes_tile_graph=0");
  });

  it("commits through the canonical writer before each read, sub-tile then full-layer", async () => {
    const calls: { cmd: string; args: Record<string, unknown> }[] = [];
    routeInvoke(() => BYTE_REPORT_4PX, calls);
    const h = byteHarness();
    vi.spyOn(console, "log").mockImplementation(() => {});
    await runPerfAudit(byteOptions(h, [8], 3));
    const patches = calls.filter((c) => c.cmd === "apply_tile_patch");
    // Three sub-tile commits plus one full-layer commit on the measured document,
    // plus the control's single full-layer commit.
    expect(patches.length).toBe(5);
    const measured = patches.filter((c) => c.args.docId === "perf-audit-bytes-8");
    expect(measured).toHaveLength(4);
    for (const p of measured) expect(p.args.docId).toBe("perf-audit-bytes-8");
    const shapes = measured.map((p) => {
      const after = p.args.after as { w: number; h: number }[];
      return after.map((a) => `${a.w}x${a.h}`).join(",");
    });
    // A full-layer patch re-tiles every tile, so the new state owns all of them.
    // That is the DILUTING case, not a zero: the older states still reach the
    // tiles they never touched, so shared_bytes holds while tile_total grows.
    // Only a document retaining nothing (the control) reads zero. Both reach the
    // store through the same writer, so the two shapes must differ.
    expect(shapes[0]).toBe("4x4");
    expect(shapes[shapes.length - 1]).toBe("8x8");
    for (const s of shapes.slice(0, 3)) expect(s).toBe("4x4");
  });

  /**
   * The control is what makes stage 3 readable. Without a document that retains
   * nothing, a falling `share` after a full-layer commit is indistinguishable
   * from sharing having collapsed - which is the misread this row exists to
   * prevent, on the surface a human actually reads. So the control has to be a
   * real, separate document driven through the same writer, and the printed
   * block has to say what its zero means.
   */
  it("drives a separate control document so the zero case is on the table", async () => {
    const calls: { cmd: string; args: Record<string, unknown> }[] = [];
    routeInvoke(() => BYTE_REPORT_4PX, calls);
    const h = byteHarness();
    const log = vi.spyOn(console, "log").mockImplementation(() => {});
    const rows = await runPerfAudit(byteOptions(h, [8], 2));
    const samples = rows.find((r) => r.op === "byte-accounting")?.bytes ?? [];
    const control = samples.find((s) => s.stage === "control-full-layer-only");
    expect(control).toBeDefined();
    expect(control?.doc_id).toBe("perf-audit-bytes-control-8");
    expect(control?.commits).toBe(1);
    // Exactly one commit on it, and it is full-layer: nothing is retained, which
    // is the only way shared_bytes reaches zero.
    const controlPatches = calls.filter(
      (c) => c.cmd === "apply_tile_patch" && c.args.docId === "perf-audit-bytes-control-8",
    );
    expect(controlPatches).toHaveLength(1);
    const after = controlPatches[0].args.after as { w: number; h: number }[];
    expect(after[0]).toMatchObject({ w: 8, h: 8 });
    // The dilution has to be stated where the numbers are read, not only in a
    // source comment a reader has to go and find.
    const table = String(log.mock.calls[0][0]);
    expect(table).toContain("dilution, not collapse");
    expect(table).toContain("control row");
    expect(table).toContain("control-full-layer-only");
  });

  /**
   * `total ms` must mean what the column header says. The probe reads are a small
   * fraction of this row's real cost - seeding ships a whole document's RGBA per
   * size and the full-layer patches are another whole document each - so a
   * probe-only total under-reports by a large factor. Falsified by making the
   * NON-probe commands slow: a probe-only total would stay near zero.
   */
  it("reports total ms for the whole row, not the probe reads alone", async () => {
    const calls: { cmd: string; args: Record<string, unknown> }[] = [];
    const spin = (ms: number): void => {
      const until = performance.now() + ms;
      while (performance.now() < until) {
        // Deliberate: jsdom timers are not a cost the row can be billed for.
      }
    };
    invokeMock.mockImplementation(async (cmd: string, args: Record<string, unknown> = {}) => {
      calls.push({ cmd, args });
      if (cmd === "rust_pixels_store_bytes") return BYTE_REPORT_4PX;
      if (cmd === "apply_tile_patch") return { tiles: [], epoch: 1, version: 1 };
      if (cmd === "paint_parity_autorun_enabled") return true;
      spin(3);
      return null;
    });
    const h = byteHarness();
    vi.spyOn(console, "log").mockImplementation(() => {});
    const rows = await runPerfAudit(byteOptions(h, [8], 1));
    const row = rows.find((r) => r.op === "byte-accounting");
    const sampleCount = row?.bytes?.length ?? 0;
    expect(sampleCount).toBeGreaterThan(0);
    expect(typeof row?.totalMs).toBe("number");
    // Seeding and commits cost real time and belong in the row's total.
    expect(row?.totalMs as number).toBeGreaterThan(5);
    expect(row?.invokeMs as number).toBeLessThan(row?.totalMs as number);
    expect(row?.notes).toContain("total ms covers seeding + commits + reads");
  });

  /**
   * The depth is the point, not a sample size. The memory requirement is about a
   * history at the store's stream cap (`max_depth`, 50), so a harness that stops
   * at 8 reproduces a shallower document than the one being claimed about - and
   * the headline figure then exists only in the Rust measurement. This asserts
   * the row's own default reaches the cap.
   */
  it("defaults the sub-tile commit depth to the store's stream cap", async () => {
    const calls: { cmd: string; args: Record<string, unknown> }[] = [];
    routeInvoke(() => BYTE_REPORT_4PX, calls);
    const h = byteHarness();
    vi.spyOn(console, "log").mockImplementation(() => {});
    // No byteCommits passed: this must exercise the shipped default.
    const options = byteOptions(h, [4], undefined as unknown as number);
    delete (options as { byteCommits?: number }).byteCommits;
    await runPerfAudit(options);
    const measured = calls.filter(
      (c) => c.cmd === "apply_tile_patch" && c.args.docId === "perf-audit-bytes-4",
    );
    // 50 sub-tile commits + 1 full-layer commit on the measured document.
    expect(measured).toHaveLength(51);
    const row = calls.length;
    expect(row).toBeGreaterThan(0);
  });

  it("surfaces a rejected probe read as an ERROR row instead of rendering undefined", async () => {
    const calls: { cmd: string; args: Record<string, unknown> }[] = [];
    routeInvoke(() => {
      // Real Tauri shape: Rust `Err(String)` arrives as a bare string rejection.
      throw "E_RUST: document not open: perf-audit-bytes-4";
    }, calls);
    const h = byteHarness();
    const log = vi.spyOn(console, "log").mockImplementation(() => {});
    const rows = await runPerfAudit(byteOptions(h, [4], 1));
    const row = rows.find((r) => r.op === "byte-accounting");
    expect(row?.totalMs).toBe("ERROR");
    expect(row?.notes).toContain("document not open");
    expect(row?.bytes).toBeUndefined();
    // A rejected read must never reach the printed table as a number.
    expect(String(log.mock.calls[0][0])).not.toContain("undefined");
  });

  it("surfaces a well-typed but crossed payload rather than printing it", async () => {
    const calls: { cmd: string; args: Record<string, unknown> }[] = [];
    // Every field is a finite number, so only the arithmetic identity catches it:
    // total != row_major + tile_graph.total_bytes means the numbers were swapped.
    routeInvoke(() => ({ ...BYTE_REPORT_4PX, total_bytes: 999 }), calls);
    const h = byteHarness();
    vi.spyOn(console, "log").mockImplementation(() => {});
    const rows = await runPerfAudit(byteOptions(h, [4], 0));
    const row = rows.find((r) => r.op === "byte-accounting");
    expect(row?.totalMs).toBe("ERROR");
    expect(row?.notes).toContain("row_major");
    expect(row?.bytes).toBeUndefined();
  });
});
