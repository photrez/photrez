// Stale-projection safety on the native-authority path.
//
// When a Rust command result carries a delta whose baseVersion does not match
// the facade's rendered version, the facade must reject the delta and re-read
// the full snapshot from the engine (EditorFacade.applyDelta gate ->
// refreshSnapshot -> bridge.getSnapshot). The stale delta's changes must never
// reach the rendered snapshot: applying them would graft state from a newer
// engine version onto an older rendered view.
//
// Also covers getSnapshot's handling of invalid external input on the native
// path: a "null" JSON parse result, an oversize payload, dims that arrived as
// null (JSON has no NaN; NaN serializes to null), and the Tauri v2 bare-string
// rejection envelope.
//
// Mock fidelity: Tauri v2 invoke() REJECTS with the bare "CODE: message"
// string on a Rust Err(String); it never resolves an {ok:false} envelope (see
// nativeClient.ts header). The invoke mock below rejects with bare strings.

import { afterEach, beforeEach, describe, expect, it, vi, type Mock } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { EditorFacade } from "../editorFacade";
import { getSnapshot, __resetNativeAuthorityForTests } from "../bridge";
import type { CommandResult, RenderLayer, RenderSnapshot } from "../types";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

const invokeMock = invoke as unknown as Mock<
  (cmd: string, args?: Record<string, unknown>) => Promise<unknown>
>;

function layer(id: string, name: string): RenderLayer {
  return {
    id,
    name,
    visible: true,
    opacity: 1,
    resourceId: 1,
    x: 0,
    y: 0,
    scaleX: 1,
    scaleY: 1,
    rotation: 0,
  };
}

// Reject exactly the way Tauri v2 invoke() rejects on a Rust Err(String):
// with the bare string, never an Error (same helper shape the native reroute
// harness uses).
function rejectWith(message: string): never {
  throw message;
}

function unknownCommand(cmd: string): never {
  rejectWith(`E_UNKNOWN_COMMAND: ${cmd}`);
}

beforeEach(() => {
  localStorage.setItem("photrez.facadeAuthority", "native");
  __resetNativeAuthorityForTests();
  invokeMock.mockReset();
});

afterEach(() => {
  localStorage.removeItem("photrez.facadeAuthority");
  __resetNativeAuthorityForTests();
  vi.restoreAllMocks();
});

describe("stale baseVersion delta falls back to a full-snapshot re-read", () => {
  it("re-reads the full snapshot and never renders the stale delta's changes", async () => {
    const fresh = layer("fresh", "Fresh");
    const stale = layer("stale", "Stale");
    const facade = new EditorFacade({ version: 5, layers: [layer("base", "Base")] }, "docStale");

    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "protocol_version_native") return 5;
      if (cmd === "protocol_apply_command_native") {
        // Delta built against engine version 99 while the facade rendered
        // version is 5: applying it would graft version-99 state onto a
        // version-5 view. The facade must reject it and re-read instead.
        const result: CommandResult = {
          documentVersion: 100,
          delta: { baseVersion: 99, version: 100, changes: [{ kind: "upsert", layer: stale }] },
        };
        return JSON.stringify(result);
      }
      if (cmd === "protocol_snapshot_native") {
        return JSON.stringify({ version: 7, layers: [fresh], width: 640, height: 480 });
      }
      return unknownCommand(cmd);
    });

    const result = await facade.addLayer("X");

    const applyIdx = invokeMock.mock.calls.findIndex(
      (c) => c[0] === "protocol_apply_command_native",
    );
    const snapIdx = invokeMock.mock.calls.findIndex((c) => c[0] === "protocol_snapshot_native");
    expect(applyIdx).toBeGreaterThanOrEqual(0);
    // The full-snapshot re-read happens AFTER the stale command result.
    expect(snapIdx).toBeGreaterThan(applyIdx);
    // The rendered projection is the re-read engine truth, not the stale delta.
    expect(facade.snapshot.version).toBe(7);
    expect(facade.snapshot.layers.map((l) => l.id)).toEqual(["fresh"]);
    expect(result).toBe(facade.snapshot);
  });
});

describe("getSnapshot invalid input on the native path", () => {
  it('a "null" parse result resolves without throwing', async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "protocol_snapshot_native") return "null";
      return unknownCommand(cmd);
    });

    await expect(getSnapshot("docNull")).resolves.toBeNull();
  });

  it("an oversize payload parses without throwing", async () => {
    const bigName = "x".repeat(1_500_000);
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "protocol_snapshot_native") {
        return JSON.stringify({ version: 6, layers: [layer("big", bigName)] });
      }
      return unknownCommand(cmd);
    });

    const snap = await getSnapshot("docBig");
    expect(snap.layers).toHaveLength(1);
    expect(snap.layers[0].name).toHaveLength(1_500_000);
  });

  it("dims that arrived as null (JSON serializes NaN to null) parse without throwing", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "protocol_snapshot_native") {
        return JSON.stringify({ version: 6, layers: [], width: null, height: null });
      }
      return unknownCommand(cmd);
    });

    const snap = await getSnapshot("docNan");
    expect(snap.width).toBeNull();
    expect(snap.height).toBeNull();
  });

  it("a bare-string Tauri rejection surfaces as an Error (never a raw string)", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "protocol_snapshot_native") rejectWith("document not open: docReject");
      return unknownCommand(cmd);
    });

    const caught = await getSnapshot("docReject").then(
      () => null,
      (e: unknown) => e,
    );
    expect(caught).toBeInstanceOf(Error);
    expect((caught as Error).message).toContain("document not open: docReject");
  });
});
