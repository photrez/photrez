// Native (Tauri-runtime) authority client for the per-document `ProtocolEngine`.
//
// The native protocol commands use a bare `"CODE: message"` string error
// envelope (`Result<T, String>`). On a Rust `Err(String)`, Tauri v2 `invoke()`
// REJECTS with exactly that string - it does NOT resolve with an `{ ok, error }`
// envelope. So these commands are driven through the RAW `invoke()` transport
// here, never the `invokeApi` wrapper (which would mis-handle the rejection as
// `[object Object]`). The rejection propagates to the (future) caller unchanged,
// so the `"CODE: message"` is always the surfaced error.
//
// Method names/args mirror `WasmProtocol` (bridge.ts) so the bridge can dispatch
// to either engine. The native-authority dispatch branch in bridge.applyCommand
// (and getSnapshot/getHistoryQuery/historyCursorCommit) calls this client to drive
// the per-document native ProtocolEngine.

import { invoke } from "@tauri-apps/api/core";

export interface NativeProtocol {
  protocol_apply_command_native: (envelopeJson: string, docId: string) => Promise<string>;
  protocol_history_query_native: (docId: string) => Promise<string>;
  protocol_history_cursor_commit_native: (
    docId: string,
    seq: number,
    direction: string,
  ) => Promise<string>;
  protocol_register_adapter_native: (docId: string, adapterId: string) => Promise<string>;
  protocol_seed_native: (payloadJson: string, docId: string) => Promise<string>;
  protocol_snapshot_native: (docId: string) => Promise<string>;
  protocol_version_native: (docId: string) => Promise<number>;
}

// Normalize empty/absent doc id to the reserved "default" key, matching the
// Rust `resolve_doc_key` and the wasm bridge default so a raw "" never enters
// the registry as its own disjoint doc.
function resolveDocKey(docId: string): string {
  return docId === "" ? "default" : docId;
}

export const nativeProtocol: NativeProtocol = {
  protocol_apply_command_native(envelopeJson, docId) {
    return invoke("protocol_apply_command_native", {
      envelopeJson,
      docId: resolveDocKey(docId),
    });
  },
  protocol_history_query_native(docId) {
    return invoke("protocol_history_query_native", { docId: resolveDocKey(docId) });
  },
  protocol_history_cursor_commit_native(docId, seq, direction) {
    return invoke("protocol_history_cursor_commit_native", {
      docId: resolveDocKey(docId),
      seq,
      direction,
    });
  },
  protocol_register_adapter_native(docId, adapterId) {
    return invoke("protocol_register_adapter_native", {
      docId: resolveDocKey(docId),
      adapterId,
    });
  },
  protocol_seed_native(payloadJson, docId) {
    return invoke("protocol_seed_native", { payloadJson, docId: resolveDocKey(docId) });
  },
  protocol_snapshot_native(docId) {
    return invoke("protocol_snapshot_native", { docId: resolveDocKey(docId) });
  },
  protocol_version_native(docId) {
    return invoke("protocol_version_native", { docId: resolveDocKey(docId) });
  },
};
