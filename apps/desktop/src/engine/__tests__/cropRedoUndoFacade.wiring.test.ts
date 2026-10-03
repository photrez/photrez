// SPDX-License-Identifier: AGPL-3.0-or-later

/**
 * crop -> undo -> redo -> undo must never reach the legacy TypeScript restore.
 *
 * THE SYMPTOM (real app, facade=1, facadeAuthority=native):
 *
 *   Undo failed: E_FACADE_OWNED: legacy restore blocked while facade owns layers
 *
 * `useEditorCommands.restoreHistorySnapshot` runs the facade handoff FIRST and returns
 * early when it reports the step handled; `engine.restore(snapshot)` - the line that
 * throws E_FACADE_OWNED - is only reachable when the handoff returns false. So the step
 * that surfaced the error is the one where `runFacadeExternalHandoff` declined a step
 * Rust was supposed to take.
 *
 * HARNESS FIDELITY. This drives the production crop entry point (`applyCropPreview`),
 * the real `DocumentEngine`, the real wasm graph mirror, the real `EditorFacade` and the
 * real `runFacadeExternalHandoff`. The IPC mock REJECTS where the real transport rejects
 * (Tauri v2 rejects with an error envelope for `Err`), because a mock that resolved would
 * make every rejection path look proven while never running - and the whole question here
 * is which path a rejection takes. The press helpers replicate
 * `restoreHistorySnapshot` verbatim but PROPAGATE the restore error instead of swallowing
 * it, so the RED is the exact production message rather than a generic branch label.
 */
import { describe, it, expect, vi, beforeAll, beforeEach, afterEach, type Mock } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { WorkspaceManager } from "@/engine/workspace";
import { hasFacadeOwnedLayers } from "@/engine/document";
import { CommandHistory } from "@/engine/history";
import { mergeActiveLayerDown } from "@/components/editor/layers/layerOperations";
import { applyCropPreview } from "@/components/editor/cropToolActions";
import { runFacadeExternalHandoff } from "@/components/editor/facadeHistoryHandoff";
import type { EditorContextValue } from "@/components/editor/shell/EditorContext";
import { getWasmExportModule } from "@/components/editor/wasmExport";
import {
  __resetFacadeRegistryForTests,
  seedFacadeFromEngine,
  installFacadeCommitShim,
  getFacade,
  historyDegraded,
} from "@/lib/protocol/facadeRegistry";
import { __resetNativeAuthorityForTests, awaitNativeSeed } from "@/lib/protocol/bridge";
import { __resetCanonicalRepushForTests } from "@/lib/protocol/canonicalSeed";
import {
  createRustStoreEmulator,
  type RustStoreEmulator,
} from "@/lib/paint/__tests__/rustStoreEmulator";
import { installFaithfulCanvas } from "@/__tests__/faithfulOffscreenCanvas";

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
let store: RustStoreEmulator;
let restoreCanvas: (() => void) | undefined;

const TAURI_KEY = "__TAURI_INTERNALS__";
const NS = "native::";
let DOC = "cropRedoUndo-0";
const SIZE = 128;

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

function installTransport(): void {
  const opened = new Set<string>();
  invokeMock.mockImplementation(async (cmd: string, args: Record<string, unknown> = {}) => {
    if (cmd.startsWith("rust_pixels_")) return store.invoke(cmd, args);
    const rawDocId = (args.docId as string) ?? "";
    const key = `${NS}${rawDocId === "" ? "default" : rawDocId}`;
    switch (cmd) {
      case "rust_pixels_open_document":
        if (!opened.has(key)) {
          opened.add(key);
          wasmMod.protocol_reset(key);
        }
        return undefined;
      case "protocol_seed_native":
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

type Engine = ReturnType<WorkspaceManager["getActiveEngine"]> & object;

function makeRenderer(): never {
  return {
    uploadImage: vi.fn(),
    uploadSurfaceTiles: vi.fn(),
    destroyTexture: vi.fn(),
    invalidatePaintSurface: vi.fn(),
    resizeToViewport: vi.fn(),
  } as never;
}

function makeEditor(engine: Engine, history: CommandHistory): EditorContextValue {
  return {
    workspace: {
      getActiveEngine: () => engine,
      getActiveHistory: () => history,
      getActiveDocumentId: () => engine.getId(),
      notifyVisualChange: vi.fn(),
    },
    renderer: { uploadImage: vi.fn(), uploadSurfaceTiles: vi.fn() },
    scheduler: { requestRender: vi.fn() },
  } as unknown as EditorContextValue;
}

type Press = "facade" | "ts" | "none";

/**
 * ONE history press, replicated from `useEditorCommands.restoreHistorySnapshot`: the
 * `hasFacadeOwnedLayers` gate, the production handoff unit, then the TS-history
 * fall-through. The restore is NOT wrapped in a catch - a fall-through that throws IS
 * the defect, and swallowing it here would hide the exact production message.
 */
async function press(
  engine: Engine,
  history: CommandHistory,
  direction: "undo" | "redo",
): Promise<Press> {
  const editor = makeEditor(engine, history);
  if (hasFacadeOwnedLayers()) {
    if (await runFacadeExternalHandoff(editor, direction)) return "facade";
  }
  const popped =
    direction === "undo" ? history.undo(engine.snapshot()) : history.redo(engine.snapshot());
  if (!popped) return "none";
  engine.restore(popped as never);
  return "ts";
}

function cursorReading(): string {
  const q = JSON.parse(wasmMod.protocol_history_query_json(`${NS}${DOC}`)) as {
    cursor?: number;
    entries?: unknown[];
    pendingExternal?: unknown;
  };
  return `cursor=${q.cursor} entries=${q.entries?.length ?? -1} pending=${q.pendingExternal ? 1 : 0}`;
}

let liveEngine: { getId(): string; getLayers(): Array<{ id: string }> } | null = null;

beforeAll(async () => {
  const mod = (await getWasmExportModule()) as WasmModule | null;
  expect(mod, "the REAL wasm pkg must load").not.toBeNull();
  wasmMod = mod!;
  (globalThis as Record<string, unknown>)[TAURI_KEY] = { invoke: invokeMock };
  installFacadeCommitShim({
    getEngine: () => liveEngine,
    getDocId: () => liveEngine?.getId() ?? "default",
  });
});

let docSeq = 0;

beforeEach(() => {
  localStorage.clear();
  localStorage.setItem("photrez.facade", "1");
  localStorage.setItem("photrez.facadeAuthority", "native");
  DOC = `cropRedoUndo-${(docSeq += 1)}`;
  restoreCanvas = installFaithfulCanvas();
  invokeMock.mockReset();
  store = createRustStoreEmulator();
  __resetNativeAuthorityForTests();
  __resetCanonicalRepushForTests();
  __resetFacadeRegistryForTests();
  installTransport();
  wasmMod.protocol_reset(`${NS}${DOC}`);
});

afterEach(() => {
  store.dispose();
  localStorage.clear();
  restoreCanvas?.();
  restoreCanvas = undefined;
  __resetNativeAuthorityForTests();
  __resetCanonicalRepushForTests();
  __resetFacadeRegistryForTests();
  vi.restoreAllMocks();
});

/** A seeded document whose single layer is FACADE-OWNED and carries real pixels. */
/**
 * A legacy TS history entry recorded on a document that ALSO owns facade layers.
 *
 * This is what makes a handoff fall-through OBSERVABLE instead of silent. With an
 * empty TS stack, a declined step pops nothing, `history.undo()` returns null and the
 * press simply reports "none" - the E_FACADE_OWNED throw is unreachable and the test
 * passes on a tree where the step was wrongly declined. With a real entry behind the
 * facade, the declined press pops it and `engine.restore` throws, which is exactly the
 * production sequence. A real document accumulates these: any legacy op committed
 * before facade ownership took the layers stays on the TS stack.
 */
function commitLegacyEntry(engine: Engine, history: CommandHistory, label: string): void {
  history.commit(engine.snapshot(), label, undefined, false, null, undefined);
}

async function openOwnedDocument(): Promise<{ engine: Engine; history: CommandHistory }> {
  const wm = new WorkspaceManager();
  const session = WorkspaceManager.createBlankDocument(DOC, "Crop", SIZE, SIZE);
  wm.addDocument(session);
  const engine = session.engine as never as Engine;
  liveEngine = engine as never;
  await seedFacadeFromEngine(engine as never, getFacade(DOC));
  await awaitNativeSeed(DOC);
  await engine.applyFacadeSnapshot(getFacade(DOC).snapshot as never);
  expect(hasFacadeOwnedLayers(), "premise: the document owns a facade layer").toBe(true);

  const layer = engine.getLayers()[0];
  const canvas = new OffscreenCanvas(layer.width, layer.height);
  (canvas.getContext("2d") as unknown as CanvasRenderingContext2D).fillRect(0, 0, 8, 8);
  engine.setLayerImageBitmap(layer.id, canvas.transferToImageBitmap());

  return { engine, history: new CommandHistory() };
}

/** The production crop apply, driven exactly as the UI drives it. */
async function cropTo(
  engine: Engine,
  history: CommandHistory,
  size: number,
): Promise<void> {
  applyCropPreview({
    workspace: {
      getActiveEngine: () => engine,
      getActiveHistory: () => history,
    } as never,
    renderer: makeRenderer(),
    viewport: { width: 800, height: 600 },
    cropRect: { x: 0, y: 0, w: size, h: size },
    cropMode: "free",
    cropSizeTarget: null,
    cropDeletePixels: true,
    cropRotation: 0,
    scheduler: { requestRender: vi.fn() } as never,
    setCropRect: vi.fn(),
    setCropRotation: vi.fn(),
    setHiddenCropPreview: vi.fn(),
    setActiveTool: vi.fn(),
    setSelectedLayerId: vi.fn(),
  });
  // The routed path resolves through a promise chain (`routeApplyCrop(...).then(...)`).
  for (let i = 0; i < 12; i += 1) await new Promise((r) => setTimeout(r, 0));
}

describe("crop -> undo -> redo -> undo never reaches the legacy restore", () => {
  it("each of the three presses is consumed by the facade handoff", async () => {
    const { engine, history } = await openOwnedDocument();
    commitLegacyEntry(engine, history, "Legacy Edit");

    await cropTo(engine, history, 40);
    expect(engine.getModel().width, "premise: the crop landed").toBe(40);
    expect(cursorReading(), "premise: the crop recorded a Rust history entry").not.toBe(
      "cursor=0 entries=0 pending=0",
    );
    const trace: string[] = [`after-crop ${cursorReading()} dims=${engine.getModel().width}`];

    // THREE presses: undo, redo, undo. Any one of them throwing E_FACADE_OWNED here
    // is the shipped defect; the `press` helper does not catch, so it propagates.
    const p1 = await press(engine, history, "undo");
    trace.push(`undo#1=${p1} ${cursorReading()} dims=${engine.getModel().width}`);
    expect(p1, `trace: ${trace.join(" | ")}`).toBe("facade");

    const p2 = await press(engine, history, "redo");
    trace.push(`redo=${p2} ${cursorReading()} dims=${engine.getModel().width}`);
    expect(p2, `trace: ${trace.join(" | ")}`).toBe("facade");

    const p3 = await press(engine, history, "undo");
    trace.push(`undo#2=${p3} ${cursorReading()} dims=${engine.getModel().width}`);
    expect(p3, `trace: ${trace.join(" | ")}`).toBe("facade");

    expect(historyDegraded(), `trace: ${trace.join(" | ")}`).toBeNull();
    expect(engine.getModel().width, `trace: ${trace.join(" | ")}`).toBe(SIZE);
  });

/**
 * The reported press sequence, with the engine in the state production reaches when
 * an external cursor commit fails.
 *
 * `confirmExternalCursor` is fail-fast: on failure it returns `{ok: false}` WITHOUT
 * clearing the pending-external barrier, and it is a latch - once `historyDegraded` is
 * set it never commits again. `runFacadeExternalHandoff` treats that outcome as
 * "handled" and returns true, so the user sees nothing. The wedge is now set and Rust
 * rejects every later command with E_EXTERNAL_PENDING - and THAT rejection is caught by
 * the handoff's single fall-through `catch`, which sends the press to the legacy store.
 * The legacy store holds an entry from before the layers became facade-owned, so
 * `engine.restore` throws E_FACADE_OWNED. That is the shipped message.
 *
 * The wedge is created here by driving the REAL Rust command without the host confirm,
 * which is exactly the post-failure state - not a mock, not a stub.
 */
it("a press blocked by a pending external barrier must not reach engine.restore", async () => {
    const { engine, history } = await openOwnedDocument();
    commitLegacyEntry(engine, history, "Legacy Edit");
    await cropTo(engine, history, 40);
    expect(engine.getModel().width, "premise: the crop landed").toBe(40);

    // The real Rust step, WITHOUT `confirmExternalCursor`: the walker steps the
    // external entry and the barrier is never committed.
    await getFacade(DOC).undo();
    const wedged = cursorReading();
    expect(wedged, "premise: an external barrier is pending").toContain("pending=1");

    // The exact rejection the press will meet, read off the real command. If the code
    // were absent from the message the handoff could not recognise it, and the test
    // would be guarding a fix that can never fire.
    let rejection = "";
    try {
      await getFacade(DOC).undo();
    } catch (e) {
      rejection = String((e as Error)?.message ?? e);
    }
    expect(rejection, "premise: the barrier rejects the next command").toContain(
      "E_EXTERNAL_PENDING",
    );
    // The press must be consumed by the facade handoff, NOT by the legacy store. It
    // used to fall through, pop the pre-facade TS entry and throw E_FACADE_OWNED.
    const p1 = await press(engine, history, "undo");
    expect(p1, `wedge=${wedged} rejection=${rejection}`).toBe("facade");

    // Refusing the press must COST NOTHING: the pre-facade entry is still on the TS
    // stack, so once the barrier clears the user still owns that step. A fix that
    // returned true by silently draining the stack would pass the assertion above and
    // lose their work.
    expect(history.canUndo(), "the refused press must not spend a legacy entry").toBe(true);
    expect(cursorReading(), "and the barrier is untouched").toContain("pending=1");
  });
});