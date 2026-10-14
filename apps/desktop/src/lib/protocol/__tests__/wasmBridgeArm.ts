/**
 * Test-only shared factory: arms the bridge surface on the REAL wasm
 * DocumentCore module - the engine the retired `photrez.facadeAuthority=wasm`
 * diversion used to select. Suites that pinned that key install this in their
 * `vi.mock("@/lib/protocol/bridge", ...)` factory instead of the retired flag.
 *
 * Fidelity notes (the mock-fidelity rule - every route below is what the retired
 * wasm arm actually did, verified against `18a5763b`'s bridge/bridge_emu):
 *  - the envelope is exactly what the retired bridge wasm arm sent
 *    (toRustEnvelope serialization, JSON in/out, envelope-error normalization),
 *    copied from the surviving ARM helpers in operationParity.matrix.test.ts;
 *  - `registerPayloadAdapter` REGISTERS ON THE WASM MODULE
 *    (`protocol_register_payload_adapter`), exactly as the retired arm did - a
 *    no-op here would make Rust answer E_UNKNOWN_ADAPTER (history.rs:291-294)
 *    on the first external transition;
 *  - `historyCursorCommit` drives the module's own
 *    `protocol_history_cursor_commit` when the export exists (the retired arm's
 *    first branch);
 *  - `getVersion`/`getLayerIds` parse the module's snapshot (the retired arm had
 *    no probe export on the wasm side, same stance as the version probe);
 *  - the native seed surface is a NO-OP - faithful, because every seed helper
 *    was gated on native authority in the retired bridge; the wasm module
 *    creates its document state on the first command.
 *
 * Arm the module once per file in beforeAll (`await armWasmModule()`); the
 * routes fail LOUD when it was never armed, so no case can silently fall back.
 */
import { getWasmExportModule } from "@/components/editor/wasmExport";

type BridgeModule = typeof import("@/lib/protocol/bridge");

interface WasmArm {
  // Per-test document reset: suites keep their `wasm.protocol_reset(docId)`
  // afterEach cleanup against the real module, so the arm type must expose it.
  protocol_reset: (docId: string) => void;
  protocol_apply_command: (json: string, docId: string) => string;
  protocol_snapshot_json: (docId: string) => string;
  protocol_history_query_json: (docId: string) => string;
  protocol_register_payload_adapter?: (id: string, docId: string) => void;
  protocol_history_cursor_commit?: (json: string, docId: string) => string;
}

let wasmModule: WasmArm | null = null;

export async function armWasmModule(): Promise<WasmArm> {
  wasmModule = (await getWasmExportModule()) as WasmArm;
  return wasmModule;
}

function requireWasm(): WasmArm {
  if (!wasmModule) {
    throw new Error("wasmBridgeArm: module not armed - call armWasmModule() in beforeAll");
  }
  return wasmModule;
}

export function makeWasmArmedBridge(actual: BridgeModule, log?: { types: string[] }): BridgeModule {
  return {
    ...actual,
    applyCommand: (async (envelope: Parameters<BridgeModule["applyCommand"]>[0]) => {
      log?.types.push(envelope.command.type);
      try {
        const out = requireWasm().protocol_apply_command(
          JSON.stringify(actual.toRustEnvelope(envelope)),
          envelope.docId ?? "default",
        );
        return JSON.parse(out) as Awaited<ReturnType<BridgeModule["applyCommand"]>>;
      } catch (e) {
        throw actual.normalizeProtocolError(e);
      }
    }) as BridgeModule["applyCommand"],
    getSnapshot: (async (docId?: string) =>
      JSON.parse(
        requireWasm().protocol_snapshot_json(docId ?? "default"),
      )) as unknown as BridgeModule["getSnapshot"],
    getHistoryQuery: (async (docId?: string) =>
      JSON.parse(
        requireWasm().protocol_history_query_json(docId ?? "default"),
      )) as unknown as BridgeModule["getHistoryQuery"],
    historyCursorCommit: (async (
      seq: number,
      direction: "undo" | "redo",
      docId?: string,
    ) => {
      const w = requireWasm();
      if (w.protocol_history_cursor_commit) {
        return JSON.parse(
          w.protocol_history_cursor_commit(JSON.stringify({ seq, direction }), docId ?? "default"),
        );
      }
      throw new Error(
        "wasmBridgeArm: the armed module has no protocol_history_cursor_commit; the retired wasm " +
          "arm fell back to the emulator arm, so this suite needs the emulator factory instead",
      );
    }) as unknown as BridgeModule["historyCursorCommit"],
    getVersion: (async (docId?: string) => {
      const snap = JSON.parse(
        requireWasm().protocol_snapshot_json(docId ?? "default"),
      ) as { version: number };
      return snap.version;
    }) as unknown as BridgeModule["getVersion"],
    getLayerIds: (async (docId?: string) => {
      const snap = JSON.parse(
        requireWasm().protocol_snapshot_json(docId ?? "default"),
      ) as { layers?: Array<{ id: string }> };
      return (snap.layers ?? []).map((l) => l.id);
    }) as unknown as BridgeModule["getLayerIds"],
    // Registers on the wasm module, exactly as the retired arm did (a no-op here
    // would surface as Rust's E_UNKNOWN_ADAPTER on the first external record).
    registerPayloadAdapter: ((adapterId: string, docId?: string) => {
      const w = requireWasm();
      if (!w.protocol_register_payload_adapter) {
        throw new Error(
          "wasmBridgeArm: the armed module has no protocol_register_payload_adapter",
        );
      }
      w.protocol_register_payload_adapter(adapterId, docId ?? "default");
    }) as unknown as BridgeModule["registerPayloadAdapter"],
    // The native seed surface no-ops under the wasm arm (the retired world gated
    // every one of these on native authority); the wasm module creates its
    // document state on the first command.
    createNativeSeed: (async () => {}) as unknown as BridgeModule["createNativeSeed"],
    seedNativeCanonical: (async () => {}) as unknown as BridgeModule["seedNativeCanonical"],
    ensureNativeEngineSeeded: (async () => {}) as unknown as BridgeModule["ensureNativeEngineSeeded"],
    awaitNativeSeed: (async () => {}) as unknown as BridgeModule["awaitNativeSeed"],
  };
}
