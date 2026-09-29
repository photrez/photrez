// SPDX-License-Identifier: AGPL-3.0-or-later
/**
 * A CDP census drain can be evaluated before the frontend module graph
 * executes pixelInvokeCensus.ts, so src-tauri/src/main.rs injects
 * censusPreload.js at document start. The injected text is shared with Rust
 * via include_str!(), so executing it here exercises the same bytes the app
 * ships: it must install both census globals, publish the document-start
 * marker the bridge polls, THROW on a read before the real registrar has run
 * (never answer with an empty snapshot that reads as "zero pixel invokes"),
 * and never shadow the real readers once the registrar has run.
 */
import { describe, expect, it, vi } from "vitest";
import source from "../censusPreload.js?raw";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

type Snapshot = {
  entries: { order: number; command: string; phase: string }[];
  pending: number;
};
type CensusWindow = {
  __photrezPixelCensus?: () => Snapshot;
  __photrezPixelFlush?: () => Promise<Snapshot>;
  __photrezPixelCensusPreload?: boolean;
};

const w = () => window as unknown as CensusWindow;
const inject = () => new Function(source)();

describe("censusPreload.js (injected before page scripts)", () => {
  it("installs not-ready census readers that throw, plus the document-start marker", () => {
    delete w().__photrezPixelCensus;
    delete w().__photrezPixelFlush;
    delete w().__photrezPixelCensusPreload;

    inject();

    expect(typeof w().__photrezPixelCensus).toBe("function");
    expect(typeof w().__photrezPixelFlush).toBe("function");
    // Before the real registrar runs a read must fail loudly: an empty snapshot
    // here would drain as "zero pixel invokes" (pixelInvokeCensus.ts:12-15).
    expect(() => w().__photrezPixelCensus!()).toThrow(/PhotrezCensusNotReady/);
    expect(() => w().__photrezPixelFlush!()).toThrow(/PhotrezCensusNotReady/);
    expect(w().__photrezPixelCensusPreload).toBe(true);
  });

  it("keeps the real registrar's readers when injected again", async () => {
    delete w().__photrezPixelCensus;
    delete w().__photrezPixelFlush;
    inject();

    const { registerPixelInvokeCensus } = await import("../pixelInvokeCensus");
    registerPixelInvokeCensus();
    const realFlush = w().__photrezPixelFlush;

    inject();

    expect(w().__photrezPixelFlush).toBe(realFlush);
    expect(await w().__photrezPixelFlush!()).toEqual({ entries: [], pending: 0 });
  });
});
