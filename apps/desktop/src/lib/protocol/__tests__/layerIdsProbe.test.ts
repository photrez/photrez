// Id-only membership probe for the guarded metadata funnels.
//
// The guarded funnels (commitFacadeOpacity/Rename/Visibility/Lock/Blend/
// Adjustment/Params) used to read the FULL guard snapshot for the layer-id
// membership check. getLayerIds drives protocol_layer_ids_native under native
// authority instead and falls back to the full snapshot when the probe is
// unavailable (older backend) or on the wasm/emulator paths (no probe export,
// same stance as the version probe). The settled-value guards are untouched.
//
// Mock fidelity: Tauri v2 invoke() REJECTS with the bare string on a Rust
// Err(String); an unregistered command on an older backend rejects the same
// way, so the fallback mock rejects with a bare string too.

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import {
  getLayerIds,
  setProtocolWasm,
  __resetNativeAuthorityForTests,
  __resetEmulatedForTests,
  emulateApply,
} from "../bridge";
import { CONTRACT_VERSION } from "../types";
import { commitFacadeOpacity, __resetFacadeRegistryForTests } from "../facadeRegistry";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const invokeMock = invoke as unknown as ReturnType<typeof vi.fn>;

const owned = new Set<string>();
vi.mock("@/engine/document", () => ({
  isFacadeOwnedLayer: (id: string) =>
    localStorage.getItem("photrez.facade") !== "0" && owned.has(id),
  hasFacadeOwnedLayers: () => owned.size > 0,
}));

// Self-contained localStorage so this test runs in the node project.
if (typeof (globalThis as { localStorage?: unknown }).localStorage === "undefined") {
  const store = new Map<string, string>();
  (globalThis as { localStorage: Storage }).localStorage = {
    getItem: (k: string) => (store.has(k) ? (store.get(k) as string) : null),
    setItem: (k: string, v: string) => {
      store.set(k, String(v));
    },
    removeItem: (k: string) => {
      store.delete(k);
    },
    clear: () => store.clear(),
    key: (i: number) => Array.from(store.keys())[i] ?? null,
    get length() {
      return store.size;
    },
  } as Storage;
}

const FULL_SNAPSHOT = {
  version: 4,
  layers: [
    {
      id: "L1",
      name: "Base",
      visible: true,
      opacity: 0.5,
      resourceId: 1,
      x: 0,
      y: 0,
      scaleX: 1,
      scaleY: 1,
      rotation: 0,
      layerType: "raster",
      blendMode: "normal",
      width: 800,
      height: 600,
    },
    {
      id: "L2",
      name: "Top",
      visible: false,
      opacity: 1,
      resourceId: 2,
      x: 10,
      y: 20,
      scaleX: 1,
      scaleY: 1,
      rotation: 0,
      layerType: "raster",
      blendMode: "normal",
      width: 800,
      height: 600,
    },
  ],
};

beforeEach(() => {
  localStorage.clear();
  localStorage.setItem("photrez.facade", "1");
  invokeMock.mockReset();
  owned.clear();
  __resetNativeAuthorityForTests();
  __resetFacadeRegistryForTests();
  __resetEmulatedForTests();
  setProtocolWasm(null as unknown as Parameters<typeof setProtocolWasm>[0]);
});

afterEach(() => {
  localStorage.clear();
  owned.clear();
  __resetNativeAuthorityForTests();
  __resetFacadeRegistryForTests();
  __resetEmulatedForTests();
  setProtocolWasm(null as unknown as Parameters<typeof setProtocolWasm>[0]);
  vi.restoreAllMocks();
});

describe("bridge.getLayerIds", () => {
  it("native authority: returns probe ids without reading the full snapshot", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "protocol_layer_ids_native") return JSON.stringify(["L1", "L2"]);
      throw `E_UNKNOWN_COMMAND: ${cmd}`;
    });

    const ids = await getLayerIds("docA");

    expect(ids).toEqual(["L1", "L2"]);
    expect(invokeMock).toHaveBeenCalledWith("protocol_layer_ids_native", { docId: "docA" });
    expect(invokeMock).not.toHaveBeenCalledWith("protocol_snapshot_native", expect.anything());
  });

  it("native authority: an older backend without the probe falls back to the full snapshot", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "protocol_layer_ids_native") throw "protocol_layer_ids_native not found";
      if (cmd === "protocol_snapshot_native") return JSON.stringify(FULL_SNAPSHOT);
      throw `E_UNKNOWN_COMMAND: ${cmd}`;
    });

    const ids = await getLayerIds("docB");

    expect(ids).toEqual(["L1", "L2"]);
    expect(invokeMock).toHaveBeenCalledWith("protocol_snapshot_native", { docId: "docB" });
  });

  it("native authority: the fallback preserves the missing-doc error (no masking)", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "protocol_layer_ids_native") throw "protocol_layer_ids_native not found";
      if (cmd === "protocol_snapshot_native") throw "document not open: docC";
      throw `E_UNKNOWN_COMMAND: ${cmd}`;
    });

    await expect(getLayerIds("docC")).rejects.toThrow("document not open: docC");
  });

  it("wasm path: parses ids from the wasm snapshot (no probe export)", async () => {
    localStorage.setItem("photrez.facadeAuthority", "wasm");
    setProtocolWasm({
      protocol_contract_version: () => CONTRACT_VERSION,
      protocol_apply_command: (_json: string, _docId: string) => JSON.stringify({}),
      protocol_snapshot_json: (_docId: string) => JSON.stringify(FULL_SNAPSHOT),
    });

    const ids = await getLayerIds("docD");

    expect(ids).toEqual(["L1", "L2"]);
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("emulator path: reads ids from the emulator snapshot", async () => {
    localStorage.setItem("photrez.facadeAuthority", "wasm");
    emulateApply({ contractVersion: CONTRACT_VERSION, command: { type: "addLayer", id: "e1", name: "E1", width: 10, height: 10, index: 0 } });
    emulateApply({ contractVersion: CONTRACT_VERSION, command: { type: "addLayer", id: "e2", name: "E2", width: 10, height: 10, index: 1 } });

    const ids = await getLayerIds("docE");

    expect(ids).toEqual(["e1", "e2"]);
    expect(invokeMock).not.toHaveBeenCalled();
  });
});

describe("guarded funnel probe adoption (commitFacadeOpacity)", () => {
  function stubEngine() {
    return {
      getId: () => "docP",
      applyFacadeSnapshot: vi.fn(),
    };
  }

  function stubFacade() {
    return {
      docId: "docP",
      snapshot: { version: 4, layers: [{ id: "L1", opacity: 0.5 }] },
      lastProjectionDimsAuthoritative: undefined,
      setOpacity: vi.fn(async () => ({ version: 4, layers: [{ id: "L1" }] })),
    };
  }

  it("membership check drives the probe, never the full snapshot", async () => {
    owned.add("L1");
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "protocol_layer_ids_native") return JSON.stringify(["L1"]);
      throw `E_UNKNOWN_COMMAND: ${cmd}`;
    });

    const r = await commitFacadeOpacity(stubEngine() as never, ["L1"], 0.5, stubFacade() as never);

    expect(r.status).toBe("applied");
    expect(invokeMock).toHaveBeenCalledWith("protocol_layer_ids_native", { docId: "docP" });
    expect(invokeMock).not.toHaveBeenCalledWith("protocol_snapshot_native", expect.anything());
  });

  it("probe ids are a fraction of the full snapshot bytes", async () => {
    owned.add("L1");
    const probeJson = JSON.stringify(["L1", "L2"]);
    const snapJson = JSON.stringify(FULL_SNAPSHOT);
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "protocol_layer_ids_native") return probeJson;
      if (cmd === "protocol_snapshot_native") return snapJson;
      throw `E_UNKNOWN_COMMAND: ${cmd}`;
    });

    await commitFacadeOpacity(stubEngine() as never, ["L1"], 0.5, stubFacade() as never);

    expect(probeJson.length).toBeLessThan(snapJson.length);
  });

  it("missing id still fails loud through the probe path", async () => {
    owned.add("L9");
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "protocol_layer_ids_native") return JSON.stringify(["L1"]);
      throw `E_UNKNOWN_COMMAND: ${cmd}`;
    });
    const facade = stubFacade();
    facade.snapshot = { version: 4, layers: [{ id: "L9", opacity: 0.5 }] };

    await expect(
      commitFacadeOpacity(stubEngine() as never, ["L9"], 0.5, facade as never),
    ).rejects.toThrow("does not hold this layer");
  });

  it("older backend: funnel succeeds through the snapshot fallback", async () => {
    owned.add("L1");
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "protocol_layer_ids_native") throw "protocol_layer_ids_native not found";
      if (cmd === "protocol_snapshot_native") return JSON.stringify(FULL_SNAPSHOT);
      throw `E_UNKNOWN_COMMAND: ${cmd}`;
    });

    const r = await commitFacadeOpacity(stubEngine() as never, ["L1"], 0.5, stubFacade() as never);

    expect(r.status).toBe("applied");
    expect(invokeMock).toHaveBeenCalledWith("protocol_snapshot_native", { docId: "docP" });
  });
});
