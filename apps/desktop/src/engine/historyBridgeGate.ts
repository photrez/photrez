// SPDX-License-Identifier: AGPL-3.0-or-later

/**
 * The TS->Rust history bridge's runtime gate - ONE predicate, read by every site
 * that records into or steps the Rust cursor.
 *
 * It lives apart from `CommandHistory` so that `snapshotTokenReattach` can ask
 * the same question without importing the history class, which keeps the
 * dependency one-way (gate -> consumers, never back).
 *
 * Gated: the bridge is OFF by default in production. It is enabled only when the
 * runtime DEV gate `localStorage["photrez.historyBridge"] === "1"` is set AND the
 * app is running in the Tauri runtime. In default production the TS
 * `CommandHistory` remains the sole undo/redo authority (no Rust cursor append,
 * so the two histories never diverge).
 */
import { isTauriRuntime } from "@/lib/desktop/tauriWindow";

const HISTORY_BRIDGE_GATE = "photrez.historyBridge";

/**
 * The TS->Rust history bridge is enabled only when the runtime DEV gate
 * `localStorage["photrez.historyBridge"] === "1"` is set AND the app runs in the
 * Tauri runtime. Default production OFF.
 *
 * Re-exported from `@/engine/history`, so importers of the predicate and
 * importers of the class that records under it cannot drift onto two gates.
 */
export function historyBridgeEnabled(): boolean {
  return (
    typeof localStorage !== "undefined" &&
    localStorage.getItem(HISTORY_BRIDGE_GATE) === "1" &&
    isTauriRuntime()
  );
}