// SPDX-License-Identifier: AGPL-3.0-or-later
//
// A refused native seed fails EVERY document open that goes through a document
// factory - blank canvas, preset, new tab, dropped layer, plain image.
//
// WHY THIS FILE EXISTS. The project-open path already fails a refused seed
// (`openProjectSeedFailure.wiring.test.ts`): nothing reaches the workspace, no
// claims are taken, no tab opens, a retry re-seeds. The two document factories
// did not. `commitFacadeBackgroundFlag` swallowed the seed failure, the factory
// catch then fell back to the host-side `markLayerBackgroundNative`, and
// `addDocument`'s `createNativeSeed(...).catch(() => {})` recorded the
// rejection. The document opened anyway holding ZERO facade-ownership claims,
// and `hasFacadeOwnedLayers()` is exactly what the undo dispatch reads - so its
// undo fell off the native walker onto the host stack, whose `stepRustCursor` is
// the only executor such a document then has. That is the executor the next
// slice deletes, so a half-owned document is a state no route may produce.
//
// The fallback this replaced could not have kept its promise either: it writes
// through the engine's own wasm mirror, not the native registry the document's
// authority now lives in - an engine the same endgame retires.
//
// THE PATH UNDER TEST IS THE REAL ONE: `openImageFilesAsDocuments` and
// `openImage` run with only their IPC boundaries stubbed, the documents are
// built by the production factories, and the seed is refused at the boundary
// (`protocol_seed_native`, the stub that otherwise resolves).
//
// MOCK FIDELITY. Two rules, both learned the hard way in this repo:
//  1. The refusal is a REJECTED promise carrying the bare string a Rust
//     `Err(String)` produces through Tauri v2 `invoke` - never a resolved
//     `{ok:false}` envelope.
//  2. The accepting side models the REAL native arm: a seeded document holds
//     exactly the layer ids it was seeded with, and `SetBackgroundFlag` flags a
//     KNOWN id (position/rotation locks + version bump) while an UNKNOWN id
//     yields an empty delta. A stub that always answered "no layers" made the
//     accepting control fail for a reason the product does not have.

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

import { hasFacadeOwnedLayers, isFacadeOwnedLayer } from "@/engine/document";
import { WorkspaceManager, type DocumentSession } from "@/engine/workspace";
import { nativeProtocol } from "@/lib/protocol/nativeClient";
import { __resetNativeAuthorityForTests, nativeSeedPromiseByDoc } from "@/lib/protocol/bridge";
import { __resetFacadeRegistryForTests } from "@/lib/protocol/facadeRegistry";
import { peekFacade } from "@/lib/protocol/selectionMirror";
import { isTauriRuntime } from "@/lib/desktop/tauriWindow";
import { invoke } from "@tauri-apps/api/core";
import { answerInvoke, resetStreams } from "@/engine/__tests__/rustStreamEmulator";
import { settle } from "@/engine/__tests__/historyCursorHarness";

type OpenImageParams = import("../editorOpenImage").OpenImageParams;

// The mock factory below runs before this file's imports are evaluated, so the
// emulator it closes over has to be built in the hoisted block (dynamic import,
// because the harness itself is an import).
const { nativeProtocolEmulator } = await vi.hoisted(async () => {
  const { createNativeProtocolEmulator } = await import(
    "@/engine/__tests__/nativeProtocolEmulator"
  );
  return { nativeProtocolEmulator: createNativeProtocolEmulator() };
});

const hoisted = vi.hoisted(() => ({
  mockShowOpenImageDialog: vi.fn<() => Promise<string[] | null>>(),
  mockReadFileBytes: vi.fn<() => Promise<Uint8Array>>(),
  mockShowToast: vi.fn(),
  mockAddRecentFile: vi.fn(),
  errors: [] as string[],
}));

/** The emulated native engine: seeded layer ids + document version per doc. */
vi.mock("@/tauri/native", () => ({
  showOpenImageDialog: hoisted.mockShowOpenImageDialog,
  loadProject: vi.fn(),
  readFileBytes: hoisted.mockReadFileBytes,
}));
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn(), save: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@/lib/desktop/tauriWindow", () => ({ isTauriRuntime: vi.fn(() => true) }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn(() => Promise.resolve(() => {})) }));
vi.mock("@tauri-apps/api/app", () => ({ getVersion: vi.fn(async () => "0.0.0") }));
vi.mock("@/lib/recentFiles", () => ({ addRecentFile: hoisted.mockAddRecentFile }));
vi.mock("../Toast", () => ({ showToast: hoisted.mockShowToast }));
vi.mock("@/lib/protocol/nativeClient", () => ({
  nativeProtocol: nativeProtocolEmulator.nativeProtocol,
}));

// The derived history view reads the query through this module.
vi.mock("@/lib/protocol/bridge_emu", () => ({
  __resetEmulatedForTests: vi.fn(),
  resetWasmDoc: vi.fn(),
  getHistoryQuery: vi.fn(async () => ({ entries: [], cursor: 0 })),
}));

interface StubBitmap extends ImageBitmap {
  __pixels: Uint8ClampedArray;
}

function makeBitmap(width: number, height: number): StubBitmap {
  return {
    width,
    height,
    close: vi.fn(),
    __pixels: new Uint8ClampedArray(width * height * 4),
  } as unknown as StubBitmap;
}

/** Every bitmap this test handed out, in creation order. */
let issued: StubBitmap[] = [];

function closesPerBitmap(): number[] {
  return issued.map((b) => vi.mocked(b.close).mock.calls.length);
}

/** The document ids the production open routes minted (they are random UUIDs). */
function sessionIds(workspace: WorkspaceManager): string[] {
  return workspace.getSessions().map((s: DocumentSession) => s.engine.getId());
}

function makeParams(workspace: WorkspaceManager, onError?: (m: string) => void): OpenImageParams {
  return {
    workspace,
    renderer: { uploadImage: vi.fn() } as unknown as OpenImageParams["renderer"],
    scheduler: { requestRender: vi.fn() } as unknown as OpenImageParams["scheduler"],
    onError:
      onError ??
      ((m: string) => {
        hoisted.errors.push(m);
      }),
    onLoading: vi.fn(),
  } as unknown as OpenImageParams;
}

function acceptSeed(): void {
  nativeProtocolEmulator.acceptSeed();
}

function refuseSeed(): void {
  nativeProtocolEmulator.refuseSeed();
}

async function openDroppedImages(workspace: WorkspaceManager, names: string[]): Promise<void> {
  const { openImageFilesAsDocuments } = await import("../editorOpenImage");
  const files = names.map(
    (name) => new File([new Uint8Array([1, 2, 3])], name, { type: "image/png" }),
  );
  await openImageFilesAsDocuments(files, makeParams(workspace));
  await settle();
}

async function openViaDialog(workspace: WorkspaceManager, path: string): Promise<void> {
  hoisted.mockShowOpenImageDialog.mockResolvedValue([path]);
  const { openImage } = await import("../editorOpenImage");
  await openImage(makeParams(workspace));
  await settle();
}

describe("a refused native seed fails a factory-built document open", () => {
  let originalCreateImageBitmap: typeof globalThis.createImageBitmap;

  beforeEach(() => {
    localStorage.clear();
    (globalThis as unknown as Record<string, () => void>).__clearFacadeOwnedForTests?.();
    __resetFacadeRegistryForTests();
    __resetNativeAuthorityForTests();
    vi.mocked(isTauriRuntime).mockReturnValue(true);
    resetStreams();
    nativeProtocolEmulator.reset();
    acceptSeed();
    issued = [];
    hoisted.errors = [];
    originalCreateImageBitmap = globalThis.createImageBitmap;
    globalThis.createImageBitmap = vi.fn(async () => {
      const b = makeBitmap(8, 8);
      issued.push(b);
      return b as unknown as ImageBitmap;
    }) as unknown as typeof globalThis.createImageBitmap;
    hoisted.mockReadFileBytes.mockResolvedValue(new Uint8Array([137, 80, 78, 71]));
    vi.mocked(invoke).mockReset();
    vi.mocked(invoke).mockImplementation((cmd: string, args: unknown) => answerInvoke(cmd, args));
    vi.spyOn(console, "warn").mockImplementation(() => {});
    vi.spyOn(console, "info").mockImplementation(() => {});
    vi.spyOn(console, "error").mockImplementation(() => {});
  });

  afterEach(async () => {
    await settle();
    globalThis.createImageBitmap = originalCreateImageBitmap;
    vi.restoreAllMocks();
    localStorage.clear();
  });

  it("the dropped-image route opens nothing and claims nothing when the seed is refused", async () => {
    const workspace = new WorkspaceManager();
    refuseSeed();

    await openDroppedImages(workspace, ["refused.png"]);

    // DEFEAT: this is the assertion the swallow could not satisfy. It reads the
    // workspace, so dropping the refusal gate from the open route - or letting
    // the factory catch fall through to `markLayerBackgroundNative` - puts the
    // session back and this reds. Nothing about the seed stub is bypassed.
    expect(sessionIds(workspace), "no host-owned document reaches the workspace").toEqual([]);
    expect(workspace.getActiveDocumentId(), "no tab is added").toBeNull();
    expect(hasFacadeOwnedLayers(), "the undo dispatch gate stays off").toBe(false);
    expect(
      hoisted.errors.some((m) => /seed/i.test(m)),
      "the refusal is reported through the open route's own error channel",
    ).toBe(true);
  });

  it("the dialog image route opens nothing when the seed is refused", async () => {
    const workspace = new WorkspaceManager();
    refuseSeed();

    await openViaDialog(workspace, "/pictures/refused.png");

    expect(sessionIds(workspace), "no document reaches the workspace").toEqual([]);
    expect(workspace.getActiveDocumentId(), "no tab is added").toBeNull();
    expect(
      hoisted.errors.some((m) => /seed/i.test(m)),
      "the refusal is reported through the open route's own error channel",
    ).toBe(true);
  });

  it("a refused open leaves no seed-barrier entry, so a retry can seed again", async () => {
    const workspace = new WorkspaceManager();
    refuseSeed();
    await openDroppedImages(workspace, ["retry.png"]);

    // `createNativeSeed` records its promise per document id and hands the recorded
    // one to every later caller, so a surviving refused entry would make this
    // read true - and the retry below would re-await the same rejection. The
    // entry is dropped by `createNativeSeed`'s own identity-guarded eviction on
    // rejection, not by the open route: removing the route's own seed drop
    // leaves this line GREEN, which is why there is no such call on that path.
    expect([...nativeSeedPromiseByDoc.keys()], "the refused open leaves no seed-barrier entry").toEqual(
      [],
    );

    acceptSeed();
    await openDroppedImages(workspace, ["retry.png"]);

    expect(sessionIds(workspace).length, "the retry opens a document").toBe(1);
  });

  it("an accepted seed still opens the dropped image", async () => {
    const workspace = new WorkspaceManager();

    await openDroppedImages(workspace, ["fine.png"]);

    // DEFEAT: the control that makes the refusal cases meaningful. A gate that
    // refused every seed - or a refusal stub that leaked into this case - reds
    // here, so the cases above cannot pass by simply never opening.
    expect(sessionIds(workspace).length, "an accepted seed opens the document").toBe(1);
    expect(workspace.getActiveDocumentId(), "and focuses it").toBe(sessionIds(workspace)[0]);
  });

  it("a refused open closes the raster it decoded", async () => {
    const workspace = new WorkspaceManager();
    refuseSeed();

    await openDroppedImages(workspace, ["raster.png"]);

    // DEFEAT: the decoded bitmap is attached to the engine the abort discards, so
    // nothing else closes it. Removing the close from the abort path leaves this
    // at [0] and reddens.
    expect(issued.length, "premise: the route decoded a raster").toBe(1);
    expect(closesPerBitmap(), "the refused open releases the raster it decoded").toEqual([1]);
  });

  it("a refused blank-document open fails before the session reaches the workspace", async () => {
    const workspace = new WorkspaceManager();
    refuseSeed();
    const session = WorkspaceManager.createBlankDocument("factory-blank", "Blank", 8, 8);
    const bgId = session.engine.getLayers()[0].id;

    // The gate every factory route must pass: it awaits the factory's own
    // background-flag commit (the open's native seed) and refuses the open when
    // that seed was refused.
    await expect(workspace.addSeededDocument(session)).rejects.toThrow(/seed/i);

    // DEFEAT (one per line): each reads state the abort must have cleaned, so
    // calling the ungated `addDocument` instead of `addSeededDocument` reddens
    // the first two; the seed entry is dropped by `createNativeSeed`'s own
    // identity-guarded eviction, which the last line pins.
    expect(sessionIds(workspace), "no session reaches the workspace").toEqual([]);
    expect(workspace.getActiveDocumentId(), "no tab is added").toBeNull();
    expect(isFacadeOwnedLayer(bgId), "the layer is claimed by nobody").toBe(false);
    expect(hasFacadeOwnedLayers(), "the undo dispatch gate stays off").toBe(false);
    expect([...nativeSeedPromiseByDoc.keys()], "the refused open leaves no seed-barrier entry").toEqual(
      [],
    );
  });

  it("a same-id retry after a refused open seeds and opens natively", async () => {
    const workspace = new WorkspaceManager();
    const DOC_ID = "factory-same-id-retry";
    refuseSeed();
    await expect(
      workspace.addSeededDocument(WorkspaceManager.createBlankDocument(DOC_ID, "Blank", 8, 8)),
    ).rejects.toThrow(/seed/i);

    // The refused attempt left its per-document state behind: the facade entry
    // survives the abort seeded with the DISCARDED engine's layers and bound to
    // it. `commitFacadeBackgroundFlag` skips its seed step whenever the facade
    // already holds layers, so the retry never re-seeds the document it is
    // opening and its first apply lands on a native engine that was never
    // seeded. DEFEAT: dropping the survivor from the abort path reds the open
    // below; leaving the retry alone keeps this red at `document not open`.
    acceptSeed();
    const retry = WorkspaceManager.createBlankDocument(DOC_ID, "Blank", 8, 8);

    await expect(workspace.addSeededDocument(retry)).resolves.toBeUndefined();

    expect(sessionIds(workspace)).toEqual([DOC_ID]);
    expect(hasFacadeOwnedLayers(), "the retried document owns its layers").toBe(true);
  });

  it("a refused open never evicts a live session's facade under the same id", async () => {
    const workspace = new WorkspaceManager();
    const DOC_ID = "factory-same-id-live";
    const session = WorkspaceManager.createBlankDocument(DOC_ID, "Blank", 8, 8);
    await expect(workspace.addSeededDocument(session)).resolves.toBeUndefined();
    const liveFacade = peekFacade(DOC_ID);
    expect(liveFacade, "premise: the live document has a facade").toBeDefined();

    // A second, same-id attempt whose commit is refused aborts - and the abort
    // path drops the per-document state it created. A live session shares that
    // state BY DOCUMENT ID, so an unconditional drop would evict the live
    // document's own facade and its next routed command would start from a
    // fresh, unseeded one.
    refuseSeed();
    await expect(
      workspace.addSeededDocument(WorkspaceManager.createBlankDocument(DOC_ID, "Blank", 8, 8)),
    ).rejects.toThrow(/seed/i);

    expect(peekFacade(DOC_ID), "the live document keeps the facade it is using").toBe(liveFacade);
  });

  it("an accepted seed opens a blank document through the same gate", async () => {
    const workspace = new WorkspaceManager();
    const session = WorkspaceManager.createBlankDocument("factory-blank-ok", "Blank", 8, 8);

    await expect(workspace.addSeededDocument(session)).resolves.toBeUndefined();

    // DEFEAT: the control for the gate. A gate that refused every seed - or one
    // that resolved before the commit settled - reds here.
    expect(sessionIds(workspace)).toEqual(["factory-blank-ok"]);
    expect(hasFacadeOwnedLayers(), "an opened factory document owns its layers").toBe(true);
  });
});