// SPDX-License-Identifier: AGPL-3.0-or-later
/**
 * A session reads the census before it has changed a single pixel: the CDP
 * drain calls window.__photrezPixelCensus() / window.__photrezPixelFlush() on a
 * freshly opened app where no pixel command has run yet. Importing this module
 * has to install those readers by itself - waiting for the first pixelInvoke
 * makes an untouched session read as "no census installed".
 */
import { describe, it, expect, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

type CensusWindow = {
  __photrezPixelCensus?: () => { entries: { command: string }[]; pending: number };
  __photrezPixelFlush?: () => Promise<{ entries: { command: string }[]; pending: number }>;
};
const w = () => window as unknown as CensusWindow;

describe("pixelInvokeCensus module-load registration", () => {
  it("installs the census readers when the module loads, with no pixel invoke first", async () => {
    delete w().__photrezPixelCensus;
    delete w().__photrezPixelFlush;

    vi.resetModules();
    await import("../pixelInvokeCensus");

    expect(typeof w().__photrezPixelCensus).toBe("function");
    expect(typeof w().__photrezPixelFlush).toBe("function");

    const snapshot = await w().__photrezPixelFlush!();
    expect(snapshot.entries).toEqual([]);
    expect(snapshot.pending).toBe(0);
  });
});
