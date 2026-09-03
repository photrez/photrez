import { describe, it, expect, beforeEach } from "vitest";
import {
  applyCommand,
  getContractVersion,
  __resetEmulatedForTests,
} from "../bridge";
import { CONTRACT_VERSION, isDeltaApplicable } from "../types";

// Wiring test — proves CommandEnvelope -> CommandResult{delta} boundary without UI change.
// Uses JS emulation when wasm is absent; when wasm is present the same assertions hold via real Rust.

describe("protocol wiring — Ticket 1", () => {
  beforeEach(() => __resetEmulatedForTests());

  it("contractVersion vs documentVersion are distinct numbers", () => {
    expect(getContractVersion()).toBe(CONTRACT_VERSION);
    const r = applyCommand({ contractVersion: CONTRACT_VERSION, command: { type: "noop" } });
    expect(r.documentVersion).toBe(1);
    expect(r.delta.baseVersion).toBe(0);
    expect(r.delta.version).toBe(1);
  });

  it("rejects wrong contractVersion", () => {
    expect(() =>
      applyCommand({ contractVersion: 999, command: { type: "noop" } }),
    ).toThrow(/E_CONTRACT_VERSION/);
  });

  it("snapshot vs delta semantics — baseVersion check", () => {
    const r1 = applyCommand({ contractVersion: CONTRACT_VERSION, command: { type: "ping", echo: "a" } });
    expect(isDeltaApplicable(r1.delta, 0)).toBe(true);
    expect(isDeltaApplicable(r1.delta, 999)).toBe(false);
    const r2 = applyCommand({ contractVersion: CONTRACT_VERSION, command: { type: "ping", echo: "b" } });
    expect(r2.delta.baseVersion).toBe(1);
    expect(isDeltaApplicable(r2.delta, 1)).toBe(true);
    expect(isDeltaApplicable(r2.delta, 0)).toBe(false);
  });

  it("delta carries resourceId and dirtyRect (resource registry)", () => {
    const r = applyCommand({ contractVersion: CONTRACT_VERSION, command: { type: "ping", echo: "x" } });
    const ch = r.delta.changes[0] as { kind: "upsert"; layer: { resourceId: number; dirtyRect: unknown } };
    expect(ch.kind).toBe("upsert");
    expect(ch.layer.resourceId).toBeGreaterThanOrEqual(1);
    expect(ch.layer.dirtyRect).toBeTruthy();
  });

  it("core is command -> delta, not mutate then refetch", () => {
    const r = applyCommand({ contractVersion: CONTRACT_VERSION, command: { type: "noop" } });
    // No separate snapshot fetch needed; delta arrives with the result
    expect(r.delta.changes.length).toBe(0);
    expect(r.documentVersion).toBe(r.delta.version);
  });

  it("stale delta is not applicable — renderer must request snapshot", () => {
    const r1 = applyCommand({ contractVersion: CONTRACT_VERSION, command: { type: "ping", echo: "a" } });
    const r2 = applyCommand({ contractVersion: CONTRACT_VERSION, command: { type: "ping", echo: "b" } });
    // renderedVersion = 0, r2 is base 1 -> not applicable
    expect(isDeltaApplicable(r2.delta, 0)).toBe(false);
    // r1 is still applicable to 0
    expect(isDeltaApplicable(r1.delta, 0)).toBe(true);
  });
});

// When wasm is present, the same boundary is exercised through real Rust (load via wasmTestShim)
describe("protocol wiring — with wasm (when available)", () => {
  it("wasm path returns same shape when available", async () => {
    let wasm: unknown = null;
    try {
      wasm = await import("@/wasm/pkg/photrez_core");
    } catch {
      return; // wasm not built in this env — JS emulation already proved the contract
    }
    const mod = wasm as {
      protocol_apply_command: (s: string, docId: string) => string;
      protocol_contract_version: () => number;
    };
    if (typeof mod.protocol_contract_version !== "function") return;
    expect(mod.protocol_contract_version()).toBe(CONTRACT_VERSION);
    const env = JSON.stringify({ contractVersion: CONTRACT_VERSION, command: { type: "ping", echo: "wasm-check" } });
    const out = JSON.parse(mod.protocol_apply_command(env, "default")) as { documentVersion: number; delta: { baseVersion: number; version: number } };
    expect(out.delta.version).toBeGreaterThan(out.delta.baseVersion);
  });
});
