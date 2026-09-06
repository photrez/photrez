import { describe, it, expect, beforeEach } from "vitest";
import { applyCommand, __resetEmulatedForTests } from "../bridge";
import { CONTRACT_VERSION } from "../types";

describe("expectedVersion guard", () => {
  beforeEach(() => __resetEmulatedForTests());

  it("expectedVersion matches -> accepted", async () => {
    const r1 = await applyCommand({ contractVersion: CONTRACT_VERSION, command: { type: "addLayer", name: "A" } });
    const v = r1.documentVersion;
    const r2 = await applyCommand({ contractVersion: CONTRACT_VERSION, expectedVersion: v, command: { type: "addLayer", name: "B" } });
    expect(r2.delta.baseVersion).toBe(v);
  });

  it("expectedVersion stale -> rejected, document unchanged", async () => {
    const r1 = await applyCommand({ contractVersion: CONTRACT_VERSION, command: { type: "addLayer", name: "A" } });
    const v = r1.documentVersion;
    await expect(applyCommand({ contractVersion: CONTRACT_VERSION, expectedVersion: 999, command: { type: "addLayer", name: "stale" } })).rejects.toThrow(/E_VERSION_MISMATCH/);
    const r2 = await applyCommand({ contractVersion: CONTRACT_VERSION, expectedVersion: v, command: { type: "addLayer", name: "B" } });
    expect(r2.delta.baseVersion).toBe(v);
  });

  it("two concurrent same expectedVersion -> exactly one accepted", async () => {
    const r1 = await applyCommand({ contractVersion: CONTRACT_VERSION, command: { type: "addLayer", name: "A" } });
    const v = r1.documentVersion;
    const ok = await applyCommand({ contractVersion: CONTRACT_VERSION, expectedVersion: v, command: { type: "addLayer", name: "B" } });
    expect(ok.delta.baseVersion).toBe(v);
    await expect(applyCommand({ contractVersion: CONTRACT_VERSION, expectedVersion: v, command: { type: "addLayer", name: "C" } })).rejects.toThrow(/E_VERSION_MISMATCH/);
  });

  it("retry after snapshot -> succeeds against new version", async () => {
    const r1 = await applyCommand({ contractVersion: CONTRACT_VERSION, command: { type: "addLayer", name: "A" } });
    const v0 = r1.documentVersion;
    await expect(applyCommand({ contractVersion: CONTRACT_VERSION, expectedVersion: 999, command: { type: "addLayer", name: "stale" } })).rejects.toThrow(/E_VERSION_MISMATCH/);
    const r2 = await applyCommand({ contractVersion: CONTRACT_VERSION, expectedVersion: v0, command: { type: "addLayer", name: "B" } });
    expect(r2.delta.baseVersion).toBe(v0);
  });
});
