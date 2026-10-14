// SPDX-License-Identifier: AGPL-3.0-or-later
//
// A refused seed must not be RECORDED as the document's seed.
//
// WHY THIS FILE EXISTS. `WorkspaceManager.addDocument` fires the open-path layer
// seed and attaches a `.catch(() => {})` to it, because the seed is a
// fire-and-forget ordering barrier and the caller has nothing to await. But
// `createNativeSeed` also RECORDS that promise per document id, and every routed
// command awaits the recorded one (`awaitNativeSeed`). A seed refused at
// addDocument time therefore left the document open with a permanently rejected
// barrier: every later routed command on it rejected, with no way to recover
// except closing the document.
//
// The project-open path already clears the recorded entry when it aborts on a
// refused seed (`loadProjectFile`); this file pins the OTHER creation site - the
// bare `addDocument`, which is what an opened image and every caller that opens
// outside the seed gate still go through. (file.new and the other new-document
// routes go through `addSeededDocument`, which awaits the tracked commit first.)
//
// TWO SHAPES, ONE BARRIER. The layer seed is recorded raw, so its rejection is
// poison. The canonical-document shadow seed is recorded as a guarded copy
// (`p.then(() => {}).catch(() => {})`), so its failure is an ordering fact, not
// an error channel - the second case here proves that difference rather than
// assuming it.
//
// REAL PATH, REAL BARRIER. `addDocument` is the production call, and the command
// is issued through the real facade, so the assertion is the barrier every
// routed command actually awaits.

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

import { WorkspaceManager } from "@/engine/workspace";
import { nativeProtocol } from "@/lib/protocol/nativeClient";
import { __resetNativeAuthorityForTests, awaitNativeSeed } from "@/lib/protocol/bridge";
import { __resetFacadeRegistryForTests } from "@/lib/protocol/facadeRegistry";
import { getFacade } from "@/lib/protocol/selectionMirror";
import { invoke } from "@tauri-apps/api/core";
import { answerInvoke, resetStreams } from "@/engine/__tests__/rustStreamEmulator";
import { flushPixelInvokeCensus } from "@/lib/protocol/pixelInvokeCensus";

async function settle() {
  await new Promise<void>((r) => setTimeout(r, 0));
  await flushPixelInvokeCensus();
}

const DOC = "open-seed-barrier-doc";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
vi.mock("@/tauri-apps/api/event", () => ({ listen: vi.fn(() => Promise.resolve(() => {})) }));
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
vi.mock("@/lib/protocol/bridge_emu", () => ({
  __resetEmulatedForTests: vi.fn(),
  resetWasmDoc: vi.fn(),
  getHistoryQuery: vi.fn(async () => ({ entries: [], cursor: 0 })),
}));

function acceptSeeds(): void {
  vi.mocked(nativeProtocol.protocol_seed_native).mockResolvedValue(JSON.stringify({ version: 0 }));
  vi.mocked(nativeProtocol.protocol_seed_canonical_native).mockResolvedValue(JSON.stringify({ ok: true }));
}

/** The document every assertion below runs on, opened the way `file.new` opens it. */
async function openDocument(workspace: WorkspaceManager): Promise<void> {
  workspace.addDocument(WorkspaceManager.createBlankDocument(DOC, "Barrier", 64, 64));
  await settle();
}

describe("an open-time seed refusal does not become the document's seed", () => {
  beforeEach(() => {
    localStorage.clear();
    (globalThis as unknown as Record<string, () => void>).__clearFacadeOwnedForTests?.();
    __resetFacadeRegistryForTests();
    __resetNativeAuthorityForTests();
    resetStreams();
    vi.mocked(invoke).mockReset();
    vi.mocked(invoke).mockImplementation((cmd: string, args: unknown) => answerInvoke(cmd, args));
    vi.spyOn(console, "warn").mockImplementation(() => {});
    acceptSeeds();
  });

  afterEach(async () => {
    await settle();
    vi.restoreAllMocks();
    localStorage.clear();
  });

  it("leaves a later routed command runnable after the open seed was refused", async () => {
    const workspace = new WorkspaceManager();
    vi.mocked(nativeProtocol.protocol_seed_native).mockRejectedValueOnce(
      new Error("E_SEED: canonical seed refused"),
    );

    await openDocument(workspace);
    expect(workspace.getDocumentCount(), "premise: the document is open").toBe(1);

    acceptSeeds();
    // The recorded entry is what every routed command awaits, so read it through
    // the same barrier a command reads, after the refusal has settled.
    await expect(awaitNativeSeed(DOC), "the seed barrier is not poisoned").resolves.toBeUndefined();
    await expect(
      getFacade(DOC).addLayer("Later"),
      "a routed command on the still-open document runs",
    ).resolves.toBeTruthy();
  });

  it("a refused canonical shadow push leaves the barrier runnable", async () => {
    const workspace = new WorkspaceManager();
    vi.mocked(nativeProtocol.protocol_seed_canonical_native).mockRejectedValue(
      new Error("E_SEED: canonical shadow refused"),
    );

    await openDocument(workspace);

    // The shadow seed is recorded as a guarded copy on purpose: its failure must
    // not become a barrier rejection. This case pins that difference.
    await expect(awaitNativeSeed(DOC), "the barrier is not poisoned by the shadow").resolves.toBeUndefined();
    await expect(getFacade(DOC).addLayer("Later")).resolves.toBeTruthy();
  });
});
