// SPDX-License-Identifier: AGPL-3.0-or-later
//
// A project whose facade seed is refused does NOT open half-owned.
//
// WHY THIS FILE EXISTS. `loadProjectFile` handed the Rust facade the opened
// document's layers and then ADOPTED ownership for them. A rejection of that seed
// used to be logged and stepped over, so the open committed anyway and the
// document entered the workspace holding NO facade-ownership claims. Undo/redo
// dispatches on `hasFacadeOwnedLayers()`: with nothing owned the press falls off
// the native walker onto the host stack, and the host's own `stepRustCursor`
// becomes the document's only cursor executor - an executor outside the native
// engine, while every other document shape moves its cursor inside that engine.
// In the native-entry sub-shape the walker still drove while the
// TypeScript twin was never drained, which is cursor/TS divergence rather than a
// dead undo. Either way a refused seed produced a document whose history path
// nothing else in the app can reach.
//
// The open now FAILS on a refused seed, the way the reader and the layer decode
// already fail it: the caller reports, and nothing reaches the workspace. That is
// only correct if the failure leaves the workspace EXACTLY as it found it, which
// is the second half of this file - the open-path release runs before the seed
// (a reopen's `restore` guard reads the superseded session's own claims) and the
// facade re-bind runs inside the seed, so both mutate state this open only owns
// while it commits.
//
// THE PATH UNDER TEST IS THE REAL ONE, same as
// `openProjectFacadeOwnership.wiring.test.ts`: `loadProjectFile` runs with only
// the two IPC boundaries stubbed, the document is the real writer's golden bytes
// with one field rewritten, and the seed is refused at the native seed boundary
// (`protocol_seed_native`, the stub that otherwise always resolves).
//
// MOCK FIDELITY. The refusal is a rejected promise with a message, which is what
// the native client surfaces when a Rust command returns `Err`. A resolving stub
// with an error-shaped payload would prove nothing - it is exactly the wrong
// contract that hid this defect in jsdom.

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

import { hasFacadeOwnedLayers, isFacadeOwnedLayer } from "@/engine/document";
import { WorkspaceManager, type DocumentSession } from "@/engine/workspace";
import { nativeProtocol } from "@/lib/protocol/nativeClient";
import { __resetNativeAuthorityForTests } from "@/lib/protocol/bridge";
import { __resetFacadeRegistryForTests } from "@/lib/protocol/facadeRegistry";
import { getFacade } from "@/lib/protocol/selectionMirror";
import { isTauriRuntime } from "@/lib/desktop/tauriWindow";
import { invoke } from "@tauri-apps/api/core";
import { answerInvoke, resetStreams, streamFor } from "@/engine/__tests__/rustStreamEmulator";
import { flushPixelInvokeCensus } from "@/lib/protocol/pixelInvokeCensus";

async function settle() {
  await new Promise<void>((r) => setTimeout(r, 0));
  await flushPixelInvokeCensus();
}
import { loadWriterGolden } from "./ptzWriterContract";

type OpenImageParams = import("../editorOpenImage").OpenImageParams;

const golden = loadWriterGolden();
const GOLDEN_CASE = "realDumpedModel";
const GOLDEN_INPUT = JSON.parse(golden[GOLDEN_CASE].input) as {
  id: string;
  layers: Array<{ id: string; width: number; height: number }>;
};
const DOC = GOLDEN_INPUT.id;
const LAYER_IDS = GOLDEN_INPUT.layers.map((l) => l.id);

const hoisted = vi.hoisted(() => ({
  mockLoadProject: vi.fn<() => Promise<{ document_json: string; layers: Record<string, string> }>>(),
  mockShowToast: vi.fn(),
  mockAddRecentFile: vi.fn(),
}));

vi.mock("@/tauri/native", () => ({
  showOpenImageDialog: vi.fn(),
  loadProject: hoisted.mockLoadProject,
  readFileBytes: vi.fn(),
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@/lib/desktop/tauriWindow", () => ({ isTauriRuntime: vi.fn(() => true) }));
vi.mock("@/tauri-apps/api/event", () => ({ listen: vi.fn(() => Promise.resolve(() => {})) }));
vi.mock("@tauri-apps/api/app", () => ({ getVersion: vi.fn(async () => "0.0.0") }));
vi.mock("@/lib/recentFiles", () => ({ addRecentFile: hoisted.mockAddRecentFile }));
vi.mock("../Toast", () => ({ showToast: hoisted.mockShowToast }));
vi.mock("@/lib/protocol/nativeClient", () => ({
  nativeProtocol: {
    protocol_seed_native: vi.fn(async () => JSON.stringify({ version: 0 })),
    protocol_seed_canonical_native: vi.fn(async () => JSON.stringify({ ok: true })),
    protocol_canonical_native: vi.fn(async () => JSON.stringify({ ok: true })),
    protocol_snapshot_native: vi.fn(async () => JSON.stringify({ version: 0, layers: [] })),
    protocol_layer_ids_native: vi.fn(async () => JSON.stringify({ ids: [] })),
    protocol_version_native: vi.fn(async () => JSON.stringify({ version: 0 })),
    protocol_register_adapter_native: vi.fn(async () => JSON.stringify({ ok: true })),
    protocol_history_query_native: vi.fn(async () => JSON.stringify({ cursor: 0, entries: [] })),
    protocol_history_cursor_commit_native: vi.fn(async () => JSON.stringify({ ok: true })),
    protocol_apply_command_native: vi.fn(async () =>
      JSON.stringify({ documentVersion: 0, delta: { changes: [], baseVersion: 0, version: 0 } }),
    ),
  },
}));

// The derivation reads the history query through this module.
vi.mock("@/lib/protocol/bridge_emu", () => ({
  __resetEmulatedForTests: vi.fn(),
  resetWasmDoc: vi.fn(),
  getHistoryQuery: vi.fn(async () => ({ entries: [], cursor: 0 })),
}));

interface DecodedBitmap extends ImageBitmap {
  __pixels: Uint8ClampedArray;
  __source: string;
}

const decodedBitmaps = new Map<string, DecodedBitmap>();

function pixelsFor(layerId: string): Uint8ClampedArray {
  const out = new Uint8ClampedArray(4);
  for (let i = 0; i < layerId.length; i++) out[i % 4] = layerId.charCodeAt(i);
  return out;
}

function stubCreateImageBitmap() {
  return (async (blob: Blob): Promise<DecodedBitmap> => {
    const entry = decodedBitmaps.get(await blob.text());
    if (!entry) throw new Error(`emulator: no PNG registered`);
    return entry;
  }) as unknown as typeof globalThis.createImageBitmap;
}

function ptzFixture(): Record<string, string> {
  const out: Record<string, string> = {};
  for (const l of GOLDEN_INPUT.layers) {
    const source = `png:${l.id}`;
    decodedBitmaps.set(source, {
      width: l.width,
      height: l.height,
      close: vi.fn(),
      __pixels: pixelsFor(l.id),
      __source: source,
    } as unknown as DecodedBitmap);
    out[l.id] = btoa(source);
  }
  return out;
}

/** The project's real bytes, unchanged: `loadProjectFile` reads them verbatim. */
function projectBytes(): string {
  return golden[GOLDEN_CASE].input;
}

async function openProject(
  workspace: WorkspaceManager,
  onLoading: (message: string | null) => void = vi.fn(),
): Promise<void> {
  hoisted.mockLoadProject.mockResolvedValue({
    document_json: projectBytes(),
    layers: ptzFixture(),
  });
  const { loadProjectFile } = await import("../editorOpenImage");
  await loadProjectFile(
    `/projects/${DOC}.ptz`,
    {
      workspace,
      renderer: { uploadImage: vi.fn() } as unknown as OpenImageParams["renderer"],
      scheduler: { requestRender: vi.fn() } as unknown as OpenImageParams["scheduler"],
      onError: vi.fn(),
      onLoading,
    } as unknown as OpenImageParams,
    `${DOC}.ptz`,
  );
  await settle();
}

/**
 * The recent-file / command-line route: `openSingleFile`, which is what the
 * title-bar menu and the startup chain call. Same refused seed, same real
 * `loadProjectFile` underneath.
 */
async function openSingleProject(
  workspace: WorkspaceManager,
  onLoading: (message: string | null) => void,
): Promise<void> {
  hoisted.mockLoadProject.mockResolvedValue({
    document_json: projectBytes(),
    layers: ptzFixture(),
  });
  const { openSingleFile } = await import("../editorOpenImage");
  await openSingleFile(`/projects/${DOC}.ptz`, {
    workspace,
    renderer: { uploadImage: vi.fn() } as unknown as OpenImageParams["renderer"],
    scheduler: { requestRender: vi.fn() } as unknown as OpenImageParams["scheduler"],
    onError: vi.fn(),
    onLoading,
  } as unknown as OpenImageParams);
  await settle();
}

function sessionOf(workspace: WorkspaceManager, docId: string = DOC): DocumentSession {
  const sessions = (workspace as unknown as { sessions: Map<string, DocumentSession> }).sessions;
  const s = sessions.get(docId);
  if (!s) throw new Error(`no session for ${docId}`);
  return s;
}

/** One layer per id, in the shape `applyFacadeSnapshot` reads. */
function projection(ids: readonly string[]) {
  return {
    version: 1,
    layers: ids.map((id, i) => ({
      id,
      name: id,
      visible: true,
      opacity: 1,
      x: 0,
      y: 0,
      scaleX: 1,
      scaleY: 1,
      rotation: 0,
      resourceId: i + 1,
    })),
  };
}

/** Close calls per decoded bitmap, in decode-registration order. */
function closesPerBitmap(): number[] {
  return [...decodedBitmaps.values()].map((b) => vi.mocked(b.close).mock.calls.length);
}

/**
 * Refuse the seed at the native boundaries it awaits, the way a Rust `Err`
 * arrives. `seedFacadeFromEngine` awaits the layer seed and then reads the
 * document back, so both are refused: the layer seed is idempotent per document
 * id, which is why the read is what makes a refused reopen reachable at all.
 */
function refuseSeed(message = "E_SEED: canonical seed refused"): void {
  vi.mocked(nativeProtocol.protocol_seed_native).mockRejectedValue(new Error(message));
  vi.mocked(nativeProtocol.protocol_snapshot_native).mockRejectedValue(new Error(message));
}

function acceptSeed(): void {
  vi.mocked(nativeProtocol.protocol_seed_native).mockResolvedValue(JSON.stringify({ version: 0 }));
  vi.mocked(nativeProtocol.protocol_snapshot_native).mockResolvedValue(
    JSON.stringify({ version: 0, layers: [] }),
  );
}

describe("a refused facade seed fails the project open", () => {
  let originalCreateImageBitmap: typeof globalThis.createImageBitmap;

  beforeEach(() => {
    localStorage.clear();
    (globalThis as unknown as Record<string, () => void>).__clearFacadeOwnedForTests?.();
    __resetFacadeRegistryForTests();
    __resetNativeAuthorityForTests();
    vi.mocked(isTauriRuntime).mockReturnValue(true);
    resetStreams();
    decodedBitmaps.clear();
    originalCreateImageBitmap = globalThis.createImageBitmap;
    globalThis.createImageBitmap = stubCreateImageBitmap();
    vi.mocked(invoke).mockReset();
    vi.mocked(invoke).mockImplementation((cmd: string, args: unknown) => answerInvoke(cmd, args));
    vi.spyOn(console, "warn").mockImplementation(() => {});
    vi.spyOn(console, "info").mockImplementation(() => {});
    acceptSeed();
  });

  afterEach(async () => {
    await settle();
    globalThis.createImageBitmap = originalCreateImageBitmap;
    vi.restoreAllMocks();
    localStorage.clear();
  });

  it("opens nothing and claims nothing when the seed is refused", async () => {
    const workspace = new WorkspaceManager();
    refuseSeed();

    await expect(openProject(workspace)).rejects.toThrow(/seed/i);

    expect(workspace.getSessions(), "no half-owned document reaches the workspace").toEqual([]);
    expect(workspace.getActiveDocumentId(), "no tab is added").toBeNull();
    for (const id of LAYER_IDS) {
      expect(isFacadeOwnedLayer(id), `layer ${id} is claimed by nobody`).toBe(false);
    }
    expect(hasFacadeOwnedLayers(), "the undo dispatch gate stays off").toBe(false);
  });

  it("a retry after a refused seed can seed again and open natively", async () => {
    const workspace = new WorkspaceManager();
    refuseSeed();
    await expect(openProject(workspace)).rejects.toThrow(/seed/i);

    // `createNativeSeed` records the rejected promise per document id and returns
    // the recorded one on every later call, so without dropping it the retry would
    // re-await the same rejection for the rest of the session.
    acceptSeed();
    await expect(openProject(workspace)).resolves.toBeUndefined();

    expect(workspace.getSessions().map((s) => s.engine.getId())).toEqual([DOC]);
    for (const id of LAYER_IDS) {
      expect(isFacadeOwnedLayer(id), `layer ${id} is owned by the retried open`).toBe(true);
    }
  });

  it("leaves a still-open session's claims owned when its reopen is refused", async () => {
    const workspace = new WorkspaceManager();
    await openProject(workspace);
    const live = sessionOf(workspace);
    for (const id of LAYER_IDS) {
      expect(isFacadeOwnedLayer(id), `premise: the live session owns ${id}`).toBe(true);
    }
    // The open-path release drops THIS document's claims before `restore`, because
    // a reopen's mixed-history guard reads them. If the open then fails, the live
    // session has been disarmed and nothing re-armed it: its undo would fall off
    // the native walker with no executor left.
    refuseSeed();

    await expect(openProject(workspace)).rejects.toThrow(/seed/i);

    expect(sessionOf(workspace), "the live session is still the one in the workspace").toBe(live);
    for (const id of LAYER_IDS) {
      expect(isFacadeOwnedLayer(id), `the live session still owns ${id}`).toBe(true);
    }
    expect(hasFacadeOwnedLayers(), "its undo still dispatches through the native walker").toBe(true);
  });

  it("leaves the facade bound to the live engine when its reopen is refused", async () => {
    const workspace = new WorkspaceManager();
    await openProject(workspace);
    const live = sessionOf(workspace);
    const facade = getFacade(DOC);
    const bindEngine = vi.spyOn(facade, "bindEngine");
    refuseSeed();

    await expect(openProject(workspace)).rejects.toThrow(/seed/i);

    // The refused seed ran `facade.bindEngine(engine)` on the facade it resolved
    // for this document id - which is the LIVE session's facade, because the key
    // is the document id. Left there, the next routed addLayer projects the
    // snapshot into a discarded engine and the live one never sees it.
    const bound = bindEngine.mock.calls.at(-1)?.[0];
    expect(bound, "the facade is left bound to the engine that is still live").toBe(live.engine);
  });

  it("closes every decoded bitmap when a refused open aborts", async () => {
    const workspace = new WorkspaceManager();
    refuseSeed();

    await expect(openProject(workspace)).rejects.toThrow(/seed/i);

    // The decode-failure path already closes what it decoded; the seed refusal
    // reaches the same abort and must do the same, or a refused open leaks a
    // full raster set. Bitmaps are per-open objects, so `decodedBitmaps` holds
    // exactly this open's set.
    expect(decodedBitmaps.size, "premise: every layer decoded").toBe(LAYER_IDS.length);
    expect(closesPerBitmap(), "the aborted open closes its decoded rasters").toEqual([1, 1]);
  });

  it("closes the refused reopen's rasters and leaves the live session's open", async () => {
    const workspace = new WorkspaceManager();
    await openProject(workspace);
    const liveRasters = [...decodedBitmaps.values()];

    refuseSeed();
    await expect(openProject(workspace)).rejects.toThrow(/seed/i);

    expect(closesPerBitmap(), "the refused reopen closes the rasters it decoded").toEqual([1, 1]);
    for (const raster of liveRasters) {
      expect(vi.mocked(raster.close), "the live session's rasters are still open").not.toHaveBeenCalled();
    }
  });

  it("a projection through the superseded engine cannot release the live claims", async () => {
    const workspace = new WorkspaceManager();
    await openProject(workspace);
    const superseded = sessionOf(workspace).engine;

    await openProject(workspace);
    const live = sessionOf(workspace).engine;
    expect(live, "the reopen installed a new engine").not.toBe(superseded);
    expect(live.getId(), "both engines answer to the one document id").toBe(superseded.getId());

    // The superseded engine is still reachable by anything that captured it (an
    // in-flight funnel holds the engine it was driving). Its projection reconciles
    // facade ownership under `getId()`, which is the LIVE document's id, so a
    // dropped layer here would strip the live session's claim over that id.
    superseded.applyFacadeSnapshot(projection([LAYER_IDS[0]]) as never);

    for (const id of LAYER_IDS) {
      expect(isFacadeOwnedLayer(id), `the live session still owns ${id}`).toBe(true);
    }
    expect(hasFacadeOwnedLayers(), "its undo still dispatches through the native walker").toBe(true);
  });

  it("clears the loading state when a refused open fails", async () => {
    const workspace = new WorkspaceManager();
    const onLoading = vi.fn();
    refuseSeed();

    await expect(openSingleProject(workspace, onLoading)).rejects.toThrow(/seed/i);

    // The route that shows the spinner is the one that has to take it down.
    // `openImage` clears it in its own `finally`; the recent-file menu and the
    // startup chain call `openSingleFile` / `loadProjectFile` directly, report
    // the failure as a toast, and never clear - so a refused open left the
    // overlay up over an editor with nothing in it.
    expect(onLoading.mock.calls.length, "premise: the open raised a loading message").toBeGreaterThan(0);
    expect(onLoading, "the failed open left a loading message on screen").toHaveBeenLastCalledWith(null);
  });

  it("an accepted seed still opens the project", async () => {
    const workspace = new WorkspaceManager();

    await expect(openProject(workspace)).resolves.toBeUndefined();

    expect(workspace.getSessions().map((s) => s.engine.getId())).toEqual([DOC]);
    expect(streamFor(DOC).cursor, "the open itself records no history").toBe(0);
    expect(hasFacadeOwnedLayers(), "the gate is on").toBe(true);
  });
});