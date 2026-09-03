// Real-wasm proof that the "ts-external" payload adapter is registered for a
// DOCUMENT-SCOPED engine before a legacy TS transition is recorded into the
// canonical Rust stream.
//
// PROBLEM: the Rust ProtocolEngine::record_external rejects an UNREGISTERED
// adapter with E_UNKNOWN_ADAPTER (crates/core/src/protocol.rs). With a
// document-scoped engine each document owns its adapters, so the adapter must be
// registered on the TARGET document's engine -- registering once on the shared
// "default" engine does not cover a non-default document.
//
// REGISTRATION POINT (documented contract): there is NO pre-registration at
// wasm-arm time. wasmExport.ts -> getWasmExportModule() calls setProtocolWasm(mod)
// and then does not register any adapter. The canonical registration point is
// facadeRegistry.recordExternalTransitionFor(), which registers
// "ts-external" on the target document's engine inline, immediately before the
// record call (facadeRegistry.ts, registerPayloadAdapter("ts-external", docId)).
//
// DISCRIMINATOR: this test relies ONLY on that inline per-document registration.
// It calls recordExternalTransitionFor() without any prior registration. Delete
// the registerPayloadAdapter("ts-external", docId) line inside
// recordExternalTransitionFor and this test FAILS -- the real engine rejects the
// unregistered adapter with E_UNKNOWN_ADAPTER, facadeRegistry catches it, and
// recordExternalTransitionFor returns { ok: false }. The wasmTestShim alias
// loads the REAL .wasm bytes, so this is the true production boundary (serde
// JSON in/out), not a fake.

import { beforeAll, describe, expect, it } from "vitest";
import { getWasmExportModule } from "@/components/editor/wasmExport";
import { recordExternalTransitionFor } from "@/lib/protocol/facadeRegistry";

beforeAll(async () => {
  // First getWasmExportModule() call in this file triggers the production
  // wiring: getWasmExportModule -> setProtocolWasm(mod). It does NOT pre-register
  // the ts-external adapter (see header).
  const m = await getWasmExportModule();
  expect(m).not.toBeNull();
  expect(typeof m.protocol_apply_command).toBe("function");
});

describe("real-wasm ts-external adapter is registered inline per document", () => {
  it("recordExternalTransitionFor succeeds on the REAL wasm engine for a NON-default document (no E_UNKNOWN_ADAPTER)", () => {
    // "docA4" is a non-default document engine. Without the inline per-document
    // registration this returns ok:false (E_UNKNOWN_ADAPTER caught in
    // facadeRegistry). With it the real engine records the legacy TS transition
    // and returns ok:true. We deliberately do NOT assert res.seq: the current
    // shipped wasm binary does not populate externalSeq, a separate concern.
    const res = recordExternalTransitionFor("docA4", {
      label: "Legacy Edit",
      affectedLayerIds: ["bg"],
      snapshot: { layers: [] },
    });
    expect(res.ok).toBe(true);
  });

  it("recordExternalTransitionFor succeeds for a SECOND document (per-document registration, not default-only)", () => {
    // Proves the registration is scoped to each document's own engine, not a
    // one-time "default" registration that would leak from wasmExport.
    const res = recordExternalTransitionFor("docA4b", {
      label: "Legacy Edit 2",
      affectedLayerIds: [],
      snapshot: { layers: [] },
    });
    expect(res.ok).toBe(true);
  });
});
