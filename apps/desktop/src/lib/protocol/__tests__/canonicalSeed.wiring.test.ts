// Wiring tests for the native canonical-document seed. When native authority is
// ON, the document-open path must seed the full canonical shadow into the native
// engine AFTER the layer seed (open + layer seed must already have run). When
// native authority is OFF (the production default), no native seed is issued at
// all - production stays byte-identical.
//
// Mock fidelity: Tauri v2 `invoke()` rejects a Rust `Err(String)` with the bare
// string, but these seeds succeed, so the mock resolves. Unknown commands throw
// so a reroute to a non-existent command surfaces loudly.

import { describe, it, expect, vi, beforeEach, afterEach, type Mock } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { WorkspaceManager } from "@/engine/workspace";
import { __resetNativeAuthorityForTests } from "../bridge";
import { getFacade, seedFacadeFromEngine, recordExternalTransitionFor } from "../facadeRegistry";

// Self-contained localStorage so this test runs in the node project (no jsdom).
if (typeof (globalThis as { localStorage?: unknown }).localStorage === "undefined") {
  const __ls = new Map<string, string>();
  (globalThis as { localStorage: Storage }).localStorage = {
    getItem: (k: string) => (__ls.has(k) ? (__ls.get(k) as string) : null),
    setItem: (k: string, v: string) => {
      __ls.set(k, String(v));
    },
    removeItem: (k: string) => {
      __ls.delete(k);
    },
    clear: () => __ls.clear(),
    key: (i: number) => Array.from(__ls.keys())[i] ?? null,
    get length() {
      return __ls.size;
    },
  } as Storage;
}

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const invokeMock = invoke as unknown as Mock<(cmd: string, args?: Record<string, unknown>) => Promise<unknown>>;

// Minimal faithful native surface for the two seed entry points + doc open.
function routeNative(): void {
  const open = new Set<string>();
  invokeMock.mockImplementation(async (cmd: string, args: Record<string, unknown> = {}) => {
    const docId = (args.docId as string) ?? "default";
    switch (cmd) {
      case "rust_pixels_open_document":
        open.add(docId);
        return undefined;
      case "protocol_seed_native":
        if (!open.has(docId)) throw `document not open: ${docId}`;
        return JSON.stringify({ version: 0, layers: [] });
      case "protocol_seed_canonical_native":
        if (!open.has(docId)) throw `document not open: ${docId}`;
        return null; // real Rust returns the "null" ack; result is ignored here
      default:
        throw `E_UNKNOWN_COMMAND: ${cmd}`;
    }
  });
}

const flush = () => new Promise<void>((r) => setTimeout(r, 0));

describe("native canonical seed wiring", () => {
  beforeEach(() => {
    localStorage.clear();
    invokeMock.mockReset();
    __resetNativeAuthorityForTests();
    routeNative();
  });
  afterEach(() => {
    localStorage.clear();
    __resetNativeAuthorityForTests();
  });

  it("seeds the canonical shadow after the layer seed when native authority is on", async () => {
    localStorage.setItem("photrez.facadeAuthority", "native");
    const wm = new WorkspaceManager();
    wm.addDocument(WorkspaceManager.createBlankDocument("docCanon", "Canon", 800, 600));
    await flush(); // open + layer seed + canonical seed are fire-and-forget

    const cmds = invokeMock.mock.calls.map((c) => c[0]);
    expect(cmds).toContain("protocol_seed_native");
    expect(cmds).toContain("protocol_seed_canonical_native");

    // Canonical seed must come AFTER the layer seed (doc already open + seeded).
    const idxLayer = cmds.indexOf("protocol_seed_native");
    const idxCanon = cmds.indexOf("protocol_seed_canonical_native");
    expect(idxCanon).toBeGreaterThan(idxLayer);

    const canonCall = invokeMock.mock.calls.find((c) => c[0] === "protocol_seed_canonical_native")!;
    expect((canonCall[1] as { docId: string }).docId).toBe("docCanon");

    const payload = JSON.parse((canonCall[1] as { payloadJson: string }).payloadJson);
    expect(payload.id).toBe("docCanon");
    expect(payload.name).toBe("Canon");
    expect(payload.width).toBe(800);
    expect(payload.layers.length).toBeGreaterThan(0);
    // Layer ids match the model (background layer is raster).
    expect(payload.layers[0].type).toBe("raster");
    // Excluded content: session/save state and transient pixel refs.
    expect(payload).not.toHaveProperty("activeLayerId");
    expect(payload).not.toHaveProperty("viewport");
    expect(payload).not.toHaveProperty("dirty");
    expect(payload).not.toHaveProperty("format");
    expect(payload.layers[0]).not.toHaveProperty("resourceId");
    expect(payload.layers[0]).not.toHaveProperty("imageBitmap");
    expect(payload.layers[0]).not.toHaveProperty("baseImageBitmap");
    expect(payload.layers[0]).not.toHaveProperty("bitmapEpoch");
  });

  it("never seeds the canonical shadow when native authority is off (production default)", async () => {
    localStorage.clear(); // facadeAuthority unset -> isNativeAuthority() false
    const wm = new WorkspaceManager();
    wm.addDocument(WorkspaceManager.createBlankDocument("docOff", "Off", 800, 600));
    await flush();

    const cmds = invokeMock.mock.calls.map((c) => c[0]);
    expect(cmds).not.toContain("protocol_seed_canonical_native");
    expect(cmds).not.toContain("protocol_seed_native");
  });

  it("surfaces a rejected canonical seed via the workspace console.warn (removing the catch breaks this)", async () => {
    localStorage.setItem("photrez.facadeAuthority", "native");
    const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});

    // Real Tauri v2 semantics: invoke() rejects a Rust Err(String) with the BARE
    // string (mirrors bridgeGetVersion.test.ts:58). Only the canonical seed command
    // fails; open + layer seed succeed so the canonical seed is actually issued and
    // its rejection is what we observe. The rejection must NOT become an unhandled
    // rejection: the workspace .catch swallows it and logs. Deleting that .catch
    // (in workspace.ts) makes console.warn never fire and this test fail.
    invokeMock.mockImplementation(async (cmd: string, args: Record<string, unknown> = {}) => {
      const docId = (args.docId as string) ?? "default";
      switch (cmd) {
        case "rust_pixels_open_document":
          return undefined;
        case "protocol_seed_native":
          return JSON.stringify({ version: 0, layers: [] });
        case "protocol_seed_canonical_native":
          throw "E_CANONICAL_PARSE: forced seed failure";
        default:
          throw `E_UNKNOWN_COMMAND: ${cmd}`;
      }
    });

    const wm = new WorkspaceManager();
    wm.addDocument(WorkspaceManager.createBlankDocument("docCanon", "Canon", 800, 600));
    await flush();
    await flush();

    expect(warnSpy).toHaveBeenCalled();
    const warned = (warnSpy.mock.calls[0]?.[0] as string) ?? "";
    expect(warned).toContain("[canonical-seed] shadow seed failed");
    // The rejected reason must be the canonical command's own rejection, not a
    // fall-through to the unknown-command default (which would mean the mock case
    // did not match the invoked command).
    const reason = (warnSpy.mock.calls[0]?.[1] as string) ?? "";
    expect(reason).toContain("E_CANONICAL_PARSE: forced seed failure");
    warnSpy.mockRestore();
  });
});

// ── Re-push after TS-side mutations the native reconciliation cannot observe ──
// Two events leave the native canonical shadow stale under native authority:
//   (a) facade addLayer mints the layer in Rust but cannot populate its
//       canonical-only fields, so the TS model must re-push the full canonical;
//   (b) mirrored external transitions (recordExternalTransitionFor) change the
//       TS model outside the facade command path.
// Both re-push the full canonical document only when native authority is ON; the
// default (wasm) path stays byte-identical. Mock fidelity: Tauri v2 `invoke()`
// rejects a Rust Err(String) with the bare string; the re-push swallows it via a
// console.warn .catch (never an unhandled rejection).

// Routes every native command the re-push paths exercise. The minted addLayer
// layer uses a stable id so the test can assert it lands in the pushed payload.
function routeNativeFull(): void {
  const open = new Set<string>();
  invokeMock.mockImplementation(async (cmd: string, args: Record<string, unknown> = {}) => {
    const docId = (args.docId as string) ?? "default";
    switch (cmd) {
      case "rust_pixels_open_document":
        open.add(docId);
        return undefined;
      case "protocol_seed_native":
        if (!open.has(docId)) throw `document not open: ${docId}`;
        return JSON.stringify({ version: 0, layers: [] });
      case "protocol_seed_canonical_native":
        if (!open.has(docId)) throw `document not open: ${docId}`;
        return null;
      case "protocol_snapshot_native":
        return JSON.stringify({ version: 0, layers: [] });
      case "protocol_version_native":
        return 0;
      case "protocol_register_adapter_native":
        return null;
      case "protocol_apply_command_native": {
        const env = JSON.parse((args.envelopeJson as string) ?? "{}") as {
          command: { type: string; name?: string };
          expectedVersion?: number;
        };
        if (env.command.type === "addLayer") {
          const id = "layer-minted-1";
          const layer = { id, name: env.command.name ?? "Layer", visible: true, opacity: 1, resourceId: 0, x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, dirtyRect: { x: 0, y: 0, width: 1, height: 1 } };
          const base = typeof env.expectedVersion === "number" ? env.expectedVersion : 0;
          return JSON.stringify({ documentVersion: base + 1, delta: { baseVersion: base, version: base + 1, changes: [{ kind: "upsert", layer } as never] }, status: "ok" });
        }
        if (env.command.type === "recordExternalTransition") {
          return JSON.stringify({ documentVersion: 1, delta: { baseVersion: 0, version: 1, changes: [] }, status: "external-recorded", externalSeq: 1 });
        }
        throw `E_UNKNOWN_COMMAND: ${env.command.type}`;
      }
      default:
        throw `E_UNKNOWN_COMMAND: ${cmd}`;
    }
  });
}

function seedOpen(docId: string, name = "RepushDoc", w = 800, h = 600) {
  localStorage.setItem("photrez.facadeAuthority", "native");
  const wm = new WorkspaceManager();
  const session = WorkspaceManager.createBlankDocument(docId, name, w, h);
  wm.addDocument(session);
  return session;
}

describe("native canonical re-push after TS-side mutations", () => {
  beforeEach(() => {
    localStorage.clear();
    invokeMock.mockReset();
    __resetNativeAuthorityForTests();
    localStorage.setItem("photrez.facadeAuthority", "native");
    routeNativeFull();
  });
  afterEach(() => {
    localStorage.clear();
    __resetNativeAuthorityForTests();
  });

  it("re-pushes the canonical shadow (with the minted layer) after a successful facade addLayer under native authority", async () => {
    const session = seedOpen("docAdd");
    await flush(); // open + layer seed + baseline canonical seed (fire-and-forget)
    invokeMock.mockClear(); // isolate the addLayer re-push from the open-path seed

    const facade = getFacade("docAdd");
    await seedFacadeFromEngine(session.engine as never, facade);
    await facade.addLayer("Layer 1");
    await flush();

    const cmds = invokeMock.mock.calls.map((c) => c[0]);
    expect(cmds).toContain("protocol_apply_command_native");
    const idxApply = cmds.indexOf("protocol_apply_command_native");
    const idxRepush = cmds.indexOf("protocol_seed_canonical_native");
    expect(idxRepush).toBeGreaterThan(idxApply); // re-push fires AFTER the apply command

    const newLayerId = facade.snapshot.layers[facade.snapshot.layers.length - 1].id;
    const repushCall = invokeMock.mock.calls[idxRepush]!;
    const payload = JSON.parse((repushCall[1] as { payloadJson: string }).payloadJson);
    expect(payload.layers.find((l: { id: string }) => l.id === newLayerId)).toBeTruthy();
  });

  it("re-pushes the canonical shadow after a mirrored external transition under native authority", async () => {
    const session = seedOpen("docExt");
    await flush();
    invokeMock.mockClear();

    await recordExternalTransitionFor(
      "docExt",
      { label: "Legacy Edit", affectedLayerIds: session.engine.getLayers().map((l) => l.id), snapshot: {} },
      session.engine as never,
    );
    await flush();

    const cmds = invokeMock.mock.calls.map((c) => c[0]);
    expect(cmds).toContain("protocol_apply_command_native"); // the record command
    const idxApply = cmds.indexOf("protocol_apply_command_native");
    const idxRepush = cmds.indexOf("protocol_seed_canonical_native");
    expect(idxRepush).toBeGreaterThan(idxApply);
  });

  it("does not re-push after addLayer or external transitions when native authority is off (production default)", async () => {
    localStorage.clear(); // facadeAuthority unset => isNativeAuthority() false
    const session = (() => {
      const wm = new WorkspaceManager();
      const s = WorkspaceManager.createBlankDocument("docOff", "Off", 800, 600);
      wm.addDocument(s);
      return s;
    })();
    await flush();
    invokeMock.mockClear();

    const facade = getFacade("docOff");
    await seedFacadeFromEngine(session.engine as never, facade);
    await facade.addLayer("Layer 1");
    await recordExternalTransitionFor("docOff", { label: "Legacy Edit", affectedLayerIds: [], snapshot: {} });
    await flush();

    const cmds = invokeMock.mock.calls.map((c) => c[0]);
    expect(cmds).not.toContain("protocol_seed_canonical_native");
  });

  it("does not re-push when the addLayer command is rejected (model did not change)", async () => {
    const session = seedOpen("docRej");
    await flush();
    invokeMock.mockClear();

    // Only the apply command rejects; the surrounding seed/snapshot calls succeed.
    const realImpl = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation(async (cmd: string, args: Record<string, unknown> = {}) => {
      if (cmd === "protocol_apply_command_native") throw "E_APPLY_FAILED: forced apply failure";
      return realImpl(cmd, args);
    });

    const facade = getFacade("docRej");
    await seedFacadeFromEngine(session.engine as never, facade);
    await expect(facade.addLayer("Layer 1")).rejects.toThrow();
    await flush();

    const cmds = invokeMock.mock.calls.map((c) => c[0]);
    expect(cmds).not.toContain("protocol_seed_canonical_native");
  });

  it("logs a rejected re-push via console.warn and never surfaces an unhandled rejection", async () => {
    const warnSpy = vi.spyOn(console, "warn").mockImplementation(() => {});
    const session = seedOpen("docWarn");
    await flush();
    invokeMock.mockClear();

    // Every native command succeeds except the re-push's canonical seed.
    const realImpl = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation(async (cmd: string, args: Record<string, unknown> = {}) => {
      if (cmd === "protocol_seed_canonical_native") throw "E_CANON_PARSE: forced repush failure";
      return realImpl(cmd, args);
    });

    const facade = getFacade("docWarn");
    await seedFacadeFromEngine(session.engine as never, facade);
    await facade.addLayer("Layer 1");
    await flush();
    await flush();

    expect(warnSpy).toHaveBeenCalled();
    const warned = (warnSpy.mock.calls[0]?.[0] as string) ?? "";
    expect(warned).toContain("[canonical-repush] shadow re-push failed");
    warnSpy.mockRestore();
  });
});
