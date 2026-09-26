// SPDX-License-Identifier: AGPL-3.0-or-later
import { describe, it, expect, vi, beforeEach } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import {
  pixelInvoke,
  registerPixelInvokeCensus,
  pendingCount,
  flushPixelInvokeCensus,
} from "../pixelInvokeCensus";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

type CensusWindow = {
  __photrezPixelCensus?: () => { entries: { order: number; command: string; phase: string }[]; pending: number };
  __photrezPixelFlush?: () => Promise<{ entries: { order: number; command: string; phase: string }[]; pending: number }>;
};
const w = () => window as unknown as CensusWindow;

describe("pixelInvokeCensus", () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    registerPixelInvokeCensus();
  });

  it("installs the census globals BEFORE the underlying invoke runs", async () => {
    delete w().__photrezPixelCensus;
    delete w().__photrezPixelFlush;
    let globalsAtInvoke: unknown;
    vi.mocked(invoke).mockImplementation(async () => {
      globalsAtInvoke = typeof w().__photrezPixelCensus;
      return { version: 1 };
    });

    await pixelInvoke("rust_pixels_write_region", { docId: "d" });

    expect(globalsAtInvoke).toBe("function");
    expect(typeof w().__photrezPixelFlush).toBe("function");
  });

  it("records {order, command, phase} with a monotonic order and drains to a terminal phase", async () => {
    vi.mocked(invoke).mockResolvedValue({});
    const before = (await flushPixelInvokeCensus()).entries.length;

    void pixelInvoke("rust_pixels_write_region", { docId: "d" });
    void pixelInvoke("apply_tile_patch", { docId: "d" });

    const snap = await flushPixelInvokeCensus();
    expect(snap.entries.length - before).toBe(2);
    const fresh = snap.entries.slice(before);
    expect(fresh.map((e) => e.command)).toEqual(["rust_pixels_write_region", "apply_tile_patch"]);
    expect(fresh[1].order).toBeGreaterThan(fresh[0].order);
    expect(fresh.every((e) => e.phase === "resolved")).toBe(true);
    expect(snap.pending).toBe(0);
    expect(pendingCount()).toBe(0);
  });

  it("flush drains an in-flight fire-and-forget invoke without a timer", async () => {
    let release!: (value: unknown) => void;
    vi.mocked(invoke).mockReturnValue(new Promise<unknown>((resolve) => { release = resolve; }));

    void pixelInvoke("rust_pixels_record_external", { docId: "d" });
    expect(pendingCount()).toBe(1);

    release({ version: 2 });
    const snap = await flushPixelInvokeCensus();
    expect(snap.pending).toBe(0);
    expect(snap.entries[snap.entries.length - 1].phase).toBe("resolved");
    expect(pendingCount()).toBe(0);
  });

  it("passes a successful value through byte-for-byte", async () => {
    const sentinel = { tiles: [{ x: 1, y: 2, w: 3, h: 4, data: [9] }], epoch: 7, version: 7 };
    vi.mocked(invoke).mockResolvedValue(sentinel);

    expect(await pixelInvoke("rust_pixels_undo", { docId: "d" })).toBe(sentinel);
  });

  it("passes a bare-string rejection through byte-for-byte and records phase rejected", async () => {
    vi.mocked(invoke).mockRejectedValue("E_RUST: boom");

    await expect(pixelInvoke("rust_pixels_write_region", { docId: "d" })).rejects.toBe("E_RUST: boom");

    const snap = await flushPixelInvokeCensus();
    expect(snap.entries[snap.entries.length - 1].phase).toBe("rejected");
    expect(snap.pending).toBe(0);
    expect(pendingCount()).toBe(0);
  });

  it("never records a command outside the six-command allowlist (read-only probes stay out)", async () => {
    vi.mocked(invoke).mockResolvedValue(3);
    const before = (await flushPixelInvokeCensus()).entries.length;

    expect(await pixelInvoke("rust_pixels_get_epoch", { docId: "d" })).toBe(3);
    expect(await pixelInvoke("rust_pixels_snapshot_layer", { docId: "d" })).toBe(3);

    expect((await flushPixelInvokeCensus()).entries.length).toBe(before);
  });

  it("reads fail closed when the registrar globals are missing, and a pixel invoke reinstalls them", async () => {
    registerPixelInvokeCensus();
    expect(typeof w().__photrezPixelCensus).toBe("function");

    delete w().__photrezPixelCensus;
    delete w().__photrezPixelFlush;
    expect(() => w().__photrezPixelCensus!()).toThrow();
    expect(() => w().__photrezPixelFlush!()).toThrow();

    vi.mocked(invoke).mockResolvedValue(0);
    await pixelInvoke("rust_pixels_write_region", { docId: "d" });
    expect(typeof w().__photrezPixelCensus).toBe("function");
    expect(typeof w().__photrezPixelFlush).toBe("function");
  });

  it("bridge shared pixel entry records through the census with the registrar installed first", async () => {
    const { invokePixelCommand } = await import("../bridge");
    let globalsAtInvoke: unknown;
    vi.mocked(invoke).mockImplementation(async () => {
      globalsAtInvoke = typeof w().__photrezPixelCensus;
      return { version: 4 };
    });
    const before = (await flushPixelInvokeCensus()).entries.length;

    const res = await invokePixelCommand("rust_pixels_write_region", { docId: "d" });

    expect(res).toEqual({ version: 4 });
    expect(globalsAtInvoke).toBe("function");
    const snap = await flushPixelInvokeCensus();
    expect(snap.entries.length).toBe(before + 1);
    expect(snap.entries[snap.entries.length - 1].command).toBe("rust_pixels_write_region");
  });
});
