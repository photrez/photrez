// SPDX-License-Identifier: AGPL-3.0-or-later
//
// A non-pixel External undo must revert the operation's EFFECT, not just step the
// cursor. This drives a real flip - metadata-only, so it needs no pixel store -
// through the real production entry points and asserts the flip is GONE after one
// undo:
//
//   CommandHistory.commit  -> installFacadeCommitShim -> recordExternalTransitionFor
//     (the shim every legacy op's commit goes through, installed once at EditorShell
//     boot) records the External entry in the REAL Rust engine;
//   engine.transformLayer  -> the legacy flip mutator MoveOptionBar.handleFlip calls;
//   runFacadeExternalHandoff(editor, "undo")
//     -> the real production orchestration unit useEditorCommands.restoreHistorySnapshot
//        calls behind the Ctrl+Z handler.
//
// AUTHORITY: native, which is the shipped default, and the only mode in which the
// commit shim re-pushes the canonical shadow at all. So this test drives the real
// production path end to end with NO hand-placed seed: the document is opened by
// WorkspaceManager.addDocument and every push is the production one. That matters
// because the defect is precisely a push the production path does not make.
//
// The engine is the real Rust engine through the wasm module, reached via a Tauri
// transport shim (the same technique hostPlumbingParity.nativeVsWasm.test.ts uses):
// jsdom has no Tauri runtime, and the wasm build exports the same command surface.
// The one wasm/native difference that matters here - the rejection shape - is
// translated exactly as that file does.
import { describe, it, expect, vi, beforeAll, afterAll, beforeEach, afterEach, type Mock } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { CommandHistory } from "@/engine/history";
import { WorkspaceManager } from "@/engine/workspace";
import { getWasmExportModule } from "@/components/editor/wasmExport";
import {
  getFacade,
  seedFacadeFromEngine,
  installFacadeCommitShim,
  getHistoryProjection,
  __resetFacadeRegistryForTests,
} from "@/lib/protocol/facadeRegistry";
import { awaitBackgroundFlagCommit } from "@/lib/protocol/backgroundFlagRouting";
import {
  awaitNativeSeed,
  flushExternalTransitions,
  __resetNativeAuthorityForTests,
} from "@/lib/protocol/bridge";
import { __resetCanonicalRepushForTests } from "@/lib/protocol/canonicalSeed";
import { runFacadeExternalHandoff } from "../facadeHistoryHandoff";
import type { EditorContextValue } from "../shell/EditorContext";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const invokeMock = invoke as unknown as Mock<
  (cmd: string, args: Record<string, unknown>) => Promise<unknown>
>;

type WasmModule = {
  protocol_reset: (docId: string) => void;
  protocol_apply_command: (json: string, docId: string) => string;
  protocol_snapshot_json: (docId: string) => string;
  protocol_version: (docId: string) => bigint;
  protocol_history_query_json: (docId: string) => string;
  protocol_history_cursor_commit: (json: string, docId: string) => string;
  protocol_register_payload_adapter: (adapterId: string, docId: string) => void;
  protocol_seed_canonical: (json: string, docId: string) => string;
};

let wasmMod: WasmModule;

// The wasm bridge rejects with a JSON `{code,message}` string; the native command
// rejects with the bare `"CODE: message"` string (protocol_native_cmds.rs:59).
function nativeRejection(e: unknown): string {
  const raw = e instanceof Error ? e.message : String(e);
  try {
    const parsed = JSON.parse(raw) as { code?: unknown; message?: unknown };
    if (typeof parsed?.code === "string" && typeof parsed?.message === "string") {
      return `${parsed.code}: ${parsed.message}`;
    }
  } catch {
    /* not a JSON envelope */
  }
  return raw;
}

function installNativeTransport(): void {
  // The native engine lives in its own process registry, so its per-doc engines
  // are namespaced away from any wasm-authority engine in the same process.
  const NS = "native::";
  const opened = new Set<string>();
  invokeMock.mockImplementation(async (cmd: string, args: Record<string, unknown> = {}) => {
    const rawDocId = (args.docId as string) ?? "";
    const docId = rawDocId === "" ? "default" : rawDocId;
    const key = `${NS}${docId}`;
    switch (cmd) {
      case "rust_pixels_open_document":
        // Idempotent, like production (pixel_store.rs:327-329): an already-open
        // doc keeps its engine. addDocument fires this through a dynamic import,
        // so a duplicate open can land after the seeds.
        if (!opened.has(key)) {
          opened.add(key);
          wasmMod.protocol_reset(key);
        }
        return undefined;
      case "protocol_seed_native":
        // The wasm build exports no layer/version seed, so the document's layer
        // state reaches the engine through the canonical push below - the same
        // "THE GAP" hostPlumbingParity documents. Return the shape the command
        // produces so a caller reading it is not misled.
        return JSON.stringify({ version: 0, layers: [] });
      case "protocol_seed_canonical_native":
        return wasmMod.protocol_seed_canonical(args.payloadJson as string, key);
      case "protocol_apply_command_native":
        try {
          return wasmMod.protocol_apply_command(args.envelopeJson as string, key);
        } catch (e) {
          throw nativeRejection(e);
        }
      case "protocol_snapshot_native":
        return wasmMod.protocol_snapshot_json(key);
      case "protocol_version_native":
        return Number(wasmMod.protocol_version(key));
      case "protocol_history_query_native":
        return wasmMod.protocol_history_query_json(key);
      case "protocol_history_cursor_commit_native":
        try {
          return wasmMod.protocol_history_cursor_commit(
            JSON.stringify({ seq: args.seq, direction: args.direction }),
            key,
          );
        } catch (e) {
          throw nativeRejection(e);
        }
      case "protocol_register_adapter_native":
        wasmMod.protocol_register_payload_adapter(args.adapterId as string, key);
        return null;
      default:
        throw `E_UNKNOWN_COMMAND: ${cmd}`;
    }
  });
}

const TAURI_KEY = "__TAURI_INTERNALS__";
const DOC = "externalFlipEffect";

const priorTauri = (globalThis as Record<string, unknown>)[TAURI_KEY];

beforeAll(async () => {
  const mod = (await getWasmExportModule()) as WasmModule | null;
  expect(mod).not.toBeNull();
  expect(mod!.protocol_apply_command.length).toBe(2);
  wasmMod = mod as WasmModule;
  // Installed for the WHOLE file, not per test: the commit shim's mirror is
  // fire-and-forget (setExternalTransitionPending), so a record started in one
  // test can still dispatch its adapter registration in the next. With the global
  // removed per test that late invoke rejects unhandled, which fails the run for a
  // reason that has nothing to do with what this file asserts.
  (globalThis as Record<string, unknown>)[TAURI_KEY] = { invoke: invokeMock };
  // Production installs this once at EditorShell boot; the shim is module-sticky.
  installFacadeCommitShim({
    getEngine: () => liveEngine,
    getDocId: () => liveEngine?.getId() ?? "default",
  });
});

afterAll(() => {
  if (priorTauri === undefined) delete (globalThis as Record<string, unknown>)[TAURI_KEY];
  else (globalThis as Record<string, unknown>)[TAURI_KEY] = priorTauri;
});

// The shim resolves the engine at commit time, so it has to be the per-test one.
let liveEngine: { getId(): string; getLayers(): Array<{ id: string }> } | null = null;

describe("a non-pixel External undo reverts the operation's effect", () => {
  beforeEach(() => {
    localStorage.clear();
    localStorage.setItem("photrez.facade", "1");
    localStorage.setItem("photrez.facadeAuthority", "native");
    invokeMock.mockReset();
    __resetNativeAuthorityForTests();
    __resetCanonicalRepushForTests();
    __resetFacadeRegistryForTests();
    installNativeTransport();
    wasmMod.protocol_reset(`native::${DOC}`);
    liveEngine = null;
  });

  afterEach(() => {
    // Drain the shim's fire-and-forget mirror before the resets, so nothing is
    // still in flight when the per-doc engines go away.
    void flushExternalTransitions(DOC);
    localStorage.clear();
    liveEngine = null;
    __resetNativeAuthorityForTests();
    __resetCanonicalRepushForTests();
    __resetFacadeRegistryForTests();
  });

  it("one undo of 'Flip Horizontal' clears flipH and leaves the layer order alone", async () => {
    // The real open path: the document factory, then addDocument's own seeds.
    const wm = new WorkspaceManager();
    const session = WorkspaceManager.createBlankDocument(DOC, "External Flip", 400, 300);
    wm.addDocument(session);
    const engine = session.engine;
    liveEngine = engine as never;
    await seedFacadeFromEngine(engine as never, getFacade(DOC));
    // addDocument and the factory's background-flag commit both fire their writes
    // without awaiting; drain the same barriers a production reader does.
    await flushExternalTransitions(DOC);
    await awaitBackgroundFlagCommit(DOC);
    await awaitNativeSeed(DOC);

    // The user adds a layer through the legacy path, then flips it - the same two
    // steps MoveOptionBar's legacy branch runs for a non-facade-owned layer.
    const paint = engine.addLayer("Paint", 100, 100);
    const history = new CommandHistory();
    history.attachDocIdGetter(() => DOC);
    history.commit(engine.snapshot(), "Flip Horizontal");
    engine.transformLayer(paint.id, { flipH: true } as never);
    // The post-record shadow re-push is fire-and-forget; drain the barrier the next
    // production command would.
    await flushExternalTransitions(DOC);

    // The flip's External entry is the tip. Its own count is not asserted: the
    // open baseline entry depends on the background-flag protocol commit landing,
    // which this transport does not always deliver (it falls back to the direct
    // native setter, logging the miss), and the undo below behaves the same
    // either way - see rustStreamHoldsUserWork, which only lowers the refusal
    // floor when no baseline was recorded.
    await expect
      .poll(
        async () => (await getHistoryProjection(DOC)).entries.at(-1)?.label,
        { timeout: 2000 },
      )
      .toBe("Flip Horizontal");
    const before = await getHistoryProjection(DOC);
    expect(before.entries.at(-1)!.origin).toBe("external:ts-external");
    expect(before.cursor).toBe(before.entries.length);
    expect(engine.getLayer(paint.id)!.transform.flipH).toBe(true);
    const orderBefore = engine.getLayers().map((l) => l.name);

    const editor = {
      workspace: {
        getActiveEngine: () => engine,
        getActiveDocumentId: () => DOC,
        getActiveHistory: () => history,
        notifyVisualChange: () => {},
      },
      renderer: { uploadImage: () => {}, uploadSurfaceTiles: () => {} },
      scheduler: { requestRender: () => {} },
    } as unknown as EditorContextValue;

    const handled = await runFacadeExternalHandoff(editor, "undo");

    // The cursor step is preserved: a non-pixel External undo must still advance
    // the stream, or the step would be unrepeatable and the redo slot lost.
    expect(handled).toBe(true);
    const after = await getHistoryProjection(DOC);
    expect(after.cursor).toBe(before.cursor - 1);

    // The EFFECT is gone. This is the assertion the defect class is about: the
    // cursor moved, so Rust reported the step handled, but the layer the user
    // flipped was still flipped in the live model. Measured at 8441551 - a
    // titlebar "Flip horizontal" click, one Ctrl+Z, flipH still true.
    expect(engine.getLayer(paint.id)!.transform.flipH).toBe(false);
    // The restore must not reorder the stack. The captured pre-op vector is the
    // engine's own projection, which a host-added layer is absent from, so
    // adopting it as the authoritative order moved Background above Paint.
    expect(engine.getLayers().map((l) => l.name)).toEqual(orderBefore);
  });
});
