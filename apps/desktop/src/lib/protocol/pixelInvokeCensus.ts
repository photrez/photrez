// SPDX-License-Identifier: AGPL-3.0-or-later
/**
 * Invoke census for the six state-changing pixel commands.
 *
 * Contract:
 *  - one monotonic counter orders every recorded invoke, so a test can prove that
 *    no pixel invoke happened after a flag-flip marker;
 *  - an entry carries `{ order, command, phase }`, where phase moves
 *    "pending" -> "resolved" | "rejected" once the invoke settles;
 *  - the underlying value or rejection is passed through untouched, so routing a
 *    call site through this wrapper cannot change what the caller observes;
 *  - registering installs `window.__photrezPixelCensus()` (sync snapshot) and the
 *    CDP drain alias `window.__photrezPixelFlush()`. Reading through a missing
 *    global throws instead of returning an empty snapshot that would read as
 *    "zero pixel invokes";
 *  - `flushPixelInvokeCensus()` awaits every in-flight invoke first. Callers such
 *    as `history.ts` fire and forget, so a raw read there can race.
 */

import { invoke } from "@tauri-apps/api/core";

/**
 * Exactly the commands the census records. Anything else - including the
 * read-only probes `rust_pixels_get_epoch` and `rust_pixels_snapshot_layer`,
 * and the document-open command - passes through without an entry, so probe
 * traffic can never be counted as a state change.
 */
const CENSUS_COMMANDS = new Set([
  "rust_pixels_write_region",
  "rust_pixels_record_external",
  "rust_pixels_undo",
  "rust_pixels_redo",
  "apply_tile_patch",
  "rust_pixels_record_snapshot",
]);

export type PixelCensusPhase = "pending" | "resolved" | "rejected";

export interface PixelCensusEntry {
  order: number;
  command: string;
  phase: PixelCensusPhase;
}

export interface PixelCensusSnapshot {
  entries: PixelCensusEntry[];
  pending: number;
}

interface CensusGlobals {
  __photrezPixelCensus?: () => PixelCensusSnapshot;
  __photrezPixelFlush?: () => Promise<PixelCensusSnapshot>;
}

const entries: PixelCensusEntry[] = [];
/** Promises that resolve only AFTER their entry reached a terminal phase. */
const inFlight = new Set<Promise<void>>();
let nextOrder = 0;

function censusWindow(): CensusGlobals | null {
  return typeof window === "undefined" ? null : (window as unknown as CensusGlobals);
}

export function registerPixelInvokeCensus(): void {
  const target = censusWindow();
  if (!target) return;
  target.__photrezPixelCensus = getPixelCensusSnapshot;
  target.__photrezPixelFlush = flushPixelInvokeCensus;
}

export function getPixelCensusSnapshot(): PixelCensusSnapshot {
  return {
    entries: entries.map((entry) => ({ ...entry })),
    pending: pendingCount(),
  };
}

export function pendingCount(): number {
  let pending = 0;
  for (const entry of entries) if (entry.phase === "pending") pending += 1;
  return pending;
}

export async function flushPixelInvokeCensus(): Promise<PixelCensusSnapshot> {
  // Loop: a caller may fire another invoke while this drain is awaiting, and
  // that new entry must reach a terminal phase before the snapshot is read.
  while (inFlight.size > 0) await Promise.all([...inFlight]);
  return getPixelCensusSnapshot();
}

export function pixelInvoke(command: string, args: Record<string, unknown>): Promise<unknown> {
  registerPixelInvokeCensus();
  if (!CENSUS_COMMANDS.has(command)) return invoke(command, args);

  const entry: PixelCensusEntry = { order: ++nextOrder, command, phase: "pending" };
  entries.push(entry);
  const started = invoke(command, args);
  // The terminal phase must be written when the invoke settles; without it the
  // entry stays "pending" forever and a drained read cannot be told apart from a
  // fire-and-forget invoke that has not even been awaited yet.
  const settled = started.then(
    () => {
      entry.phase = "resolved";
    },
    () => {
      entry.phase = "rejected";
    },
  );
  inFlight.add(settled);
  void settled.then(() => { inFlight.delete(settled); });
  return started;
}
