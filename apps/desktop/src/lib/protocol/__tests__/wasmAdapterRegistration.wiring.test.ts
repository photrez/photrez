// Structural-sharing wiring proof: on the REAL wasm path, the production
// bridge-arming call registers the "ts-external" payload adapter so legacy TS
// transitions can be recorded into the canonical Rust stream.
//
// PROBLEM: the Rust ProtocolEngine::record_external rejects an UNREGISTERED
// adapter with E_UNKNOWN_ADAPTER (crates/core/src/protocol.rs). The production
// wiring in wasmExport.ts -> getWasmExportModule() previously called
// setProtocolWasm(mod) WITHOUT registering the adapter, so the first real-wasm
// recordExternalTransitionFor(...) recorded a legacy TS transition and the real
// engine returned E_UNKNOWN_ADAPTER (caught -> ok:false).
//
// DISCRIMINATOR: this test relies ONLY on the production wiring
// (getWasmExportModule -> setProtocolWasm + registerPayloadAdapter) to register
// the adapter. It does NOT call bridge.registerPayloadAdapter itself, and it
// does NOT call protocol_reset() before the assertion (a reset would recreate
// the engine and drop the registration). Delete the
// registerPayloadAdapter("ts-external") call in wasmExport.ts and this test
// FAILS -- recordExternalTransitionFor returns ok:false because the real engine
// rejects the unregistered adapter. The wasmTestShim alias loads the REAL .wasm
// bytes, so this is the true production boundary (serde JSON in/out), not a fake.

import { beforeAll, describe, expect, it } from "vitest";
import { getWasmExportModule } from "@/components/editor/wasmExport";
import { recordExternalTransitionFor } from "@/lib/protocol/facadeRegistry";

beforeAll(async () => {
  // First getWasmExportModule() call in this file triggers the production
  // wiring: setProtocolWasm(mod) + registerPayloadAdapter("ts-external").
  const m = await getWasmExportModule();
  expect(m).not.toBeNull();
  expect(typeof m.protocol_apply_command).toBe("function");
});

describe("real-wasm ts-external adapter is registered by the production wiring", () => {
  it("recordExternalTransitionFor succeeds on the REAL wasm engine (no E_UNKNOWN_ADAPTER)", () => {
    // Without the adapter registration this returns ok:false (E_UNKNOWN_ADAPTER
    // caught in facadeRegistry). With registration the real engine records the
    // legacy TS transition and returns ok:true + a seq.
    const res = recordExternalTransitionFor("docA4", {
      label: "Legacy Edit",
      affectedLayerIds: ["bg"],
      snapshot: { layers: [] },
    });
    // ok:true is the discriminator -- without the adapter registration the real
    // engine throws E_UNKNOWN_ADAPTER, facadeRegistry catches it, and returns
    // { ok: false }. We intentionally do NOT assert res.seq: the current shipped
    // wasm binary does not populate externalSeq, which is a separate concern.
    expect(res.ok).toBe(true);
  });
});
