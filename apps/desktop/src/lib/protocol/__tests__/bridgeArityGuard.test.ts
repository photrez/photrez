// Document-scoped engine stale-pkg arity guard proof.
//
// PROBLEM: the per-document protocol requires a 2-arg protocol_apply_command(
// json, docId). An older shared-engine pkg exports a 1-arg wrapper; JS silently
// drops the extra docId, so every document would collapse into a single
// "default" engine while tests stay green (the exact "green-but-wrong"
// anti-pattern). setProtocolWasm's arity guard throws E_STALE_WASM at arm time
// so a stale pkg is caught HERE rather than in every later assertion.
//
// DISCRIMINATOR: this exercises the guard directly at the setProtocolWasm
// boundary. A 1-arg fixture MUST throw E_STALE_WASM; a 2-arg fixture must arm
// fine. Delete the arity check in bridge.ts setProtocolWasm and the 1-arg test
// FAILS (it stops throwing), while the 2-arg test stays green - exactly the
// failure mode the guard exists to prevent.
//
// We call setProtocolWasm directly (not through getWasmExportModule) because the
// production loader's catch-all downgrades an arm-time throw to console.warn +
// null (see bridge.ts comment on the guard's honest scope): the guard's throw is
// observable at THIS boundary.

import { describe, it, expect, afterEach } from "vitest";
import { getContractVersion, isFacadeArmed, setProtocolWasm } from "../bridge";
import { CONTRACT_VERSION } from "../types";

afterEach(() => {
  setProtocolWasm(null as unknown as Parameters<typeof setProtocolWasm>[0]);
});

function makeStaleWasm() {
  return {
    protocol_contract_version: () => CONTRACT_VERSION,
    // 1-arg wrapper - function.length === 1, so the arity guard must reject it.
    protocol_apply_command: (json: string) => json,
    protocol_snapshot_json: (_docId: string) => JSON.stringify({ version: 0, layers: [] }),
  };
}

function makeCurrentWasm() {
  return {
    protocol_contract_version: () => CONTRACT_VERSION,
    // 2-arg wrapper - function.length === 2, the document-scoped engine contract.
    protocol_apply_command: (json: string, _docId: string) => json,
    protocol_snapshot_json: (_docId: string) => JSON.stringify({ version: 0, layers: [] }),
  };
}

describe("bridge.setProtocolWasm stale-pkg arity guard", () => {
  it("throws E_STALE_WASM for a 1-arg protocol_apply_command (stale shared-engine pkg)", () => {
    expect(() => setProtocolWasm(makeStaleWasm() as never)).toThrow("E_STALE_WASM");
    // The throw must leave the bridge UNARMED so a stale pkg cannot half-wire.
    expect(isFacadeArmed()).toBe(false);
  });

  it("arms fine for a 2-arg protocol_apply_command (document-scoped engine)", () => {
    expect(() => setProtocolWasm(makeCurrentWasm() as never)).not.toThrow();
    expect(isFacadeArmed()).toBe(true);
    expect(getContractVersion()).toBe(CONTRACT_VERSION);
  });
});
