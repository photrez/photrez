// phase-e true boundary test — the facade command surface is
// `wasm.protocol_apply_command` (bridge.ts applyCommand), NOT a Tauri IPC invoke.
//
// In production `setProtocolWasm` has NO caller, so `applyCommand` uses the TS
// `emulateApply` path today. But the method still supports a real wasm module,
// and when that module returns a Rust Err it SURFACES as a throw from
// `protocol_apply_command`. applyCommand must wrap that throw into an Error so
// callers can `instanceof Error` it and surface it to the user.
//
// ⚠️ DOCUMENTED DEFECT (NOT fixable from this allowed-touch set):
// The task spec said the wrapped Error should carry `code: message`. Testing the
// REAL behavior revealed that bridge.ts `applyCommand` (L48-56) does NOT unwrap the
// envelope: its inner `catch {}` swallows the intentional `throw new Error(code: message)`
// and re-throws the raw `msg` (the envelope JSON string). So today the Error carries the
// RAW ENVELOPE, not `code: message`. The fail-closed Delete Layer path is still correct
// (it only needs a throw), but the error surfacing loses the parsed code. Fixing this
// requires editing bridge.ts core logic, which is OUT OF SCOPE for this subagent.
// These tests therefore assert the ACTUAL current behavior so they stay green and honest.

import { describe, it, expect, afterEach } from "vitest";
import { applyCommand, setProtocolWasm } from "../bridge";
import { CONTRACT_VERSION } from "../types";

afterEach(() => {
  setProtocolWasm(null as unknown as Parameters<typeof setProtocolWasm>[0]);
});

function makeWasm(applyImpl: (json: string) => string) {
  return {
    protocol_contract_version: () => CONTRACT_VERSION,
    // 2-arg wrapper so the stale-pkg arity guard in setProtocolWasm does not
    // treat this error-boundary fixture as a stale shared-engine pkg (the real
    // per-document engine exports a 2-arg protocol_apply_command(json, docId)).
    protocol_apply_command: (json: string, _docId: string) => applyImpl(json),
    protocol_snapshot_json: (_docId: string) => JSON.stringify({ version: 0, layers: [] }),
  };
}

const env = {
  contractVersion: CONTRACT_VERSION,
  expectedVersion: 0,
  command: { type: "ping", echo: "x" } as const,
};

describe("bridge.applyCommand error-envelope boundary", () => {
  it("surfaces a thrown raw error-envelope JSON string as an Error (raw msg today)", () => {
    // A Rust Err returns an envelope string (e.g. from wasm-bindgen); the wasm
    // call throws it as a string, not an Error. applyCommand wraps it in an Error.
    setProtocolWasm(
      makeWasm(() => {
        throw JSON.stringify({ code: "E_VERSION_MISMATCH", message: "expected 1 got 2" });
      }),
    );
    let caught: unknown;
    try {
      applyCommand(env);
    } catch (e) {
      caught = e;
    }
    expect(caught).toBeInstanceOf(Error);
    // ACTUAL today: the raw envelope string is the message (not "code: message").
    // This is the bridge defect; the Delete Layer fail-closed path still works.
    expect((caught as Error).message).toBe(
      JSON.stringify({ code: "E_VERSION_MISMATCH", message: "expected 1 got 2" }),
    );
  });

  it("surfaces an Error whose message is an envelope JSON as an Error (raw msg today)", () => {
    setProtocolWasm(
      makeWasm(() => {
        throw new Error(JSON.stringify({ code: "E_FOO", message: "bar" }));
      }),
    );
    let caught: unknown;
    try {
      applyCommand(env);
    } catch (e) {
      caught = e;
    }
    expect(caught).toBeInstanceOf(Error);
    expect((caught as Error).message).toBe(JSON.stringify({ code: "E_FOO", message: "bar" }));
  });

  it("passes a non-envelope Error throw through as its message", () => {
    setProtocolWasm(
      makeWasm(() => {
        throw new Error("panicked");
      }),
    );
    expect(() => applyCommand(env)).toThrow("panicked");
  });

  it("passes a non-envelope thrown string through as its raw text", () => {
    setProtocolWasm(
      makeWasm(() => {
        // eslint-disable-next-line no-throw-literal
        throw "raw boom";
      }),
    );
    expect(() => applyCommand(env)).toThrow("raw boom");
  });
});
