// Native-authority history view derivation.
//
// CommandHistory, the History panel's jump route, and the handoff refusal
// gate all need the same reading of `protocol_history_query_native`: which
// entries sit below the cursor, where the document-open baseline ends, and
// what the derived counts are. This module is the single place those
// derivations live, so the panel rows, the undo/redo counts, and the jump
// target cannot drift onto different conventions.
import type { HistoryEntryView, HistoryQueryResult } from "@/lib/protocol/types";
import { createSignal } from "solid-js";

export type HistoryViewItem = { label: string; isRedo: boolean };

/**
 * Who currently owns the derived history view. This signal is test-pinned
 * instrumentation for the authority transition: no production consumer reads
 * it, deliberately - the only transition a running session can show is the
 * first read going authoritative, so a UI consumer could only ever report
 * "working as intended". `nativeHistoryViewFloor.test.ts:250-265` pins the
 * flip, and the console lines are what operators see. Initial value reflects
 * that the view starts from the TS stacks until an authoritative native read
 * lands.
 */
const [historyViewAuthority, setHistoryViewAuthority] = createSignal<"native" | "ts-fallback">(
  "ts-fallback",
);
export { historyViewAuthority, setHistoryViewAuthority };

/**
 * Is this a valid-but-serviceable stream read? The query is the authority
 * only when it actually knows the document: a session whose commits never
 * reached the stream leaves it empty, and then the TS stacks still own the
 * view. On the canonical engine every document carries at least the
 * document-open baseline entry (cursor 1 at open), so this test cannot
 * misfire there.
 */
export function isNativeQueryAuthoritative(
  query: Pick<HistoryQueryResult, "entries" | "cursor"> | null,
): boolean {
  if (!query) return false;
  return query.entries.length > 0 || query.cursor > 0;
}

/**
 * Count of leading entries that are the document-open baseline.
 *
 * The factory commits one SetBackgroundFlag apply at open time, before any
 * user edit, and it lands as stream position 0. It is not a user edit, so
 * undoing it would walk the cursor in front of history. The predicate reads
 * the stream itself rather than a host-side flag: a boolean recorded at open
 * stays sticky after the bounded stream evicts the entry, and after eviction
 * the oldest surviving USER entry would become un-undoable. Reading the
 * stream content also counts the duplicate-baseline case (a second factory
 * commit at open), which the boolean over-counts.
 *
 * The leading-run shape matters: a SetBackgroundFlag the user performs later
 * in the session is a real edit and must stay undoable.
 */
export function deriveOpenBaselineFloor(entries: HistoryEntryView[]): number {
  let floor = 0;
  for (const entry of entries) {
    const isOpenBaseline =
      entry.label === "Set Background Flag" &&
      (entry.origin === "native" || entry.origin.startsWith("external"));
    if (!isOpenBaseline) break;
    floor += 1;
  }
  return floor;
}

export function getNativeUndoCount(
  query: Pick<HistoryQueryResult, "entries" | "cursor">,
): number {
  // Malformed wire shape (cursor past the end) must not report undoable work
  // that does not exist; buildNativeHistoryItems already clamps the same way.
  const cursor = Math.min(Math.max(0, query.cursor), query.entries.length);
  return Math.max(0, cursor - deriveOpenBaselineFloor(query.entries as HistoryEntryView[]));
}

export function getNativeRedoCount(
  query: Pick<HistoryQueryResult, "entries" | "cursor">,
): number {
  const cursor = Math.min(Math.max(0, query.cursor), query.entries.length);
  return Math.max(0, query.entries.length - cursor);
}

/**
 * The History panel rows, in one convention: row index i corresponds to
 * cursor i + floor. Row 0 is the document-open state (cursor == floor),
 * every later row is one entry forward, and the rows past the current
 * cursor are the redo steps in replay order (the next redo adjacent to the
 * current row). Without the floor skip, the baseline entry renders as a
 * past row, the active-row highlight sits one row high, and clicks land one
 * entry off from the row the user sees.
 */
export function buildNativeHistoryItems(
  query: Pick<HistoryQueryResult, "entries" | "cursor">,
): HistoryViewItem[] {
  const entries = query.entries as HistoryEntryView[];
  const floor = deriveOpenBaselineFloor(entries);
  // Clamp both ends: a malformed cursor (past the end, or negative) must map
  // onto real stream positions, not index into entries that do not exist.
  // The redo walk also never starts below the baseline run: row 0 already
  // stands for the open state, so a cursor under the floor must not replay
  // the baseline entry as a redo row.
  const cursor = Math.min(Math.max(0, query.cursor), entries.length);
  const items: HistoryViewItem[] = [{ label: "Open", isRedo: false }];
  for (let i = floor; i < cursor; i++) {
    items.push({ label: entries[i].label || "Unknown Operation", isRedo: false });
  }
  for (let i = Math.max(cursor, floor); i < entries.length; i++) {
    items.push({ label: entries[i].label || "Unknown Operation", isRedo: true });
  }
  return items;
}

/**
 * Settle one commit's recorders before the caller re-reads the native stream.
 *
 * The facade commit shim registers its mirror promise synchronously right
 * after the commit wrapper's `apply` returns, so by the time this async
 * chain runs the pending transition is visible to flushExternalTransitions.
 * Cost: one `protocol_history_query_native` IPC round trip per commit - the
 * price of a derived view that cannot go stale over live work.
 *
 * `record` is the commit's bridge fire (null when it recorded nothing); a
 * rejected fire has already marked its entry and must not block the read.
 */
export async function settleCommitRecorders(
  record: Promise<unknown> | null,
  docId: string,
): Promise<void> {
  try {
    await record;
  } catch {
    // Fire failures already mark the entry; never block the resync on them.
  }
  if (docId) {
    try {
      const { flushExternalTransitions } = await import("@/lib/protocol/bridge");
      await flushExternalTransitions(docId);
    } catch {
      // A missing bridge must not leave the derived view stale forever.
    }
  }
}

/**
 * Re-read `protocol_history_query_native` and hand the result to `apply`,
 * which publishes it (cache set + derived-view repaint).
 *
 * After a history-relevant press the NATIVE stream is the authority on what
 * exists; the TS stacks only keep the snapshot payloads the fallback arms
 * replay from. Never rejects: a query the host cannot complete (no doc,
 * transport hiccup) leaves the previous cache in place, so one bad read
 * cannot turn a healthy press into a dead undo button. `apply` throwing is
 * swallowed for the same reason - a dead repaint callback must not fail the
 * press either.
 */
export async function resyncDerivedView(
  docId: string | undefined,
  apply: (query: HistoryQueryResult) => void,
): Promise<void> {
  if (!docId) return;
  try {
    const { getHistoryQuery } = await import("@/lib/protocol/bridge_emu");
    apply(await getHistoryQuery(docId));
  } catch {
    // Keep the last known view; the caller's cache stays untouched.
  }
}
