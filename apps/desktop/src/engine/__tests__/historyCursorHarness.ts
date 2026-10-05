// SPDX-License-Identifier: AGPL-3.0-or-later

/**
 * The observation helpers the history-cursor parity cases share: waiting, draining,
 * counting and driving one production undo/redo press.
 *
 * Extracted from `historyCursorParity.wiring.test.ts` so the file holds cases and
 * this holds the mechanics. All of it observes the SAME emulated Rust stream and
 * the same `invoke` spy the cases assert on, so a helper that drifted from the
 * stream's timing rules would silently weaken every case that uses it - which is
 * why the ordering note lives in `rustStreamEmulator.ts` and `tick`/`settle` are
 * the only two yields, deliberately different.
 */
import { vi } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { flushPixelInvokeCensus } from "@/lib/protocol/pixelInvokeCensus";
import { streamFor } from "./rustStreamEmulator";

export const waitFor = async (pred: () => boolean, ms = 3000) => {
  const start = Date.now();
  while (!pred()) {
    if (Date.now() - start > ms) throw new Error("waitFor timeout");
    await new Promise((r) => setTimeout(r, 5));
  }
};

/** Wait until the emulated Rust stream for `docId` holds `count` entries. */
export const waitForRust = async (docId: string, count: number) => {
  await waitFor(() => streamFor(docId).entries.length >= count);
  await settle();
};

/**
 * Drain the fire-and-forget bridge invokes. `commit` reaches Rust through a
 * dynamic `import()`, so one macrotask is not enough under parallel workers;
 * cases that assert a specific Rust cursor use `waitForRust`.
 */
export async function settle() {
  await new Promise<void>((r) => setTimeout(r, 0));
  await flushPixelInvokeCensus();
}

/**
 * One macrotask, NO census drain. `flushPixelInvokeCensus` awaits every in-flight
 * pixel invoke, so a case that deliberately HOLDS a cursor step open would
 * deadlock on it; this is the only yield such a case may use.
 */
export const tick = () => new Promise<void>((r) => setTimeout(r, 0));

/** Count the invokes the emulator saw for `command`. */
export const timesInvoked = (command: string): number =>
  vi.mocked(invoke).mock.calls.filter((c) => c[0] === command).length;

/** Count the cursor-step invokes in EITHER direction. */
export const cursorStepInvokes = (): number =>
  vi.mocked(invoke).mock.calls.filter(
    (c) => c[0] === "rust_pixels_undo" || c[0] === "rust_pixels_redo",
  ).length;

/** Only the probe's own divergence warns, not the hook's unrelated setup noise. */
export const parityWarns = (): string[] =>
  vi
    .mocked(console.warn)
    .mock.calls.map((c) => String(c[0]))
    .filter((m) => m.includes("history-cursor-parity"));

/**
 * Fire the production undo/redo command and wait for the probe read it triggers.
 * `commands.undo`/`redo` are `() => execute("edit.undo")` - they return void and
 * `restoreHistorySnapshot` runs fire-and-forget, so completion is observed, not
 * awaited. The cursor read is the last thing each production path does, so its
 * arrival marks the step as done.
 */
export async function driveStep(run: () => void): Promise<void> {
  const before = timesInvoked("rust_pixels_history_tip");
  run();
  await waitFor(() => timesInvoked("rust_pixels_history_tip") > before);
  await settle();
}

/** The facade commit shim's gate: `photrez.facade !== "0"`, so ON unless set. */
export const FACADE_KEY = "photrez.facade";

/**
 * Opt a case OUT of the facade commit shim, so "no recorder armed" is actually true.
 *
 * The shim is a THIRD recorder for a host pop, independent of `photrez.rustPixels`
 * and `photrez.historyBridge`. At its default it records an `External` entry for
 * every non-pixel commit, which means a case that never installs it would still see
 * the host take the shim arm of `stepRustCursor` and issue a cursor step for an entry
 * nothing in the case recorded. Every "expected 0 steps" row would then be measuring
 * the shim's default rather than the flag and the bridge under test.
 *
 * Call this before the flag row is applied if the row helper sets the other keys.
 * The shipping-default arrangement - the shim really installed, really recording - is
 * pinned over the real shim in
 * apps/desktop/src/engine/__tests__/historyCursorDriftClosure.wiring.test.ts.
 */
export function optOutOfShimRecording(): void {
  localStorage.setItem(FACADE_KEY, "0");
}
