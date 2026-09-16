// Call-site wiring for the text option bar's edit-mode controls. Italic stands in
// for the whole bar: font family, size, weight, align, color, stroke, and box mode
// all funnel through the same applyEdit.
//
// With native authority enabled, editing a facade-owned text layer must go through
// ONE SetLayerParams command and write NO TS history entry: the native arm records
// the step, and a TS entry here would strand an undo point whose engine.restore()
// rejects with E_FACADE_OWNED. On a layer the arm does not hold the command
// restates nothing, so the edit is kept and the miss is reported out loud.
//
// The flag-OFF assertions pin the legacy commit-before-mutate path unchanged, and
// the live-session assertions pin the no-history live-mutate path (the session
// commits exactly once at close).
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi, type MockInstance } from "vitest";
import { render } from "solid-js/web";
import { TextOptionBar } from "../TextOptionBar";
import { EditorProvider, useEditor } from "../shell/EditorContext";
import { WorkspaceManager } from "@/engine/workspace";
import type { DocumentEngine } from "@/engine/document";
import { DEFAULT_TEXT_DATA, type TextData } from "@/engine/textTypes";
import {
  __resetFacadeRegistryForTests,
  getFacade,
  seedFacadeFromEngine,
} from "@/lib/protocol/facadeRegistry";
import * as bridge from "@/lib/protocol/bridge";
import { applyCommand, getSnapshot } from "@/lib/protocol/bridge";
import { CONTRACT_VERSION } from "@/lib/protocol/types";
import type { CommandEnvelope } from "@/lib/protocol/types";
import { getWasmExportModule } from "../wasmExport";
import { showToast } from "../Toast";

vi.mock("@/components/editor/Toast", () => ({ showToast: vi.fn() }));
vi.mock("@/lib/desktop/tauriWindow", () => ({ isTauriRuntime: vi.fn(() => false) }));

const toastMock = vi.mocked(showToast);

const DOC_W = 800;
const DOC_H = 600;

let applySpy: MockInstance;

function tick(): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, 0));
}

async function flush(): Promise<void> {
  for (let i = 0; i < 5; i++) await tick();
}

function dispatchedTypes(): string[] {
  const types: string[] = [];
  for (const call of applySpy.mock.calls) {
    const type = (call[0] as CommandEnvelope).command?.type;
    if (typeof type === "string") types.push(type);
  }
  return types;
}

interface Bar {
  engine: DocumentEngine;
  container: HTMLElement;
  dispose: () => void;
  editor: () => ReturnType<typeof useEditor>;
  history: () => NonNullable<ReturnType<WorkspaceManager["getActiveHistory"]>>;
  renderer: { uploadImage: MockInstance };
  select: (id: string) => Promise<void>;
}

function mountBar(ws: WorkspaceManager): Bar {
  const renderer = { uploadImage: vi.fn(), destroyTexture: vi.fn(), resize: vi.fn(), resizeToViewport: vi.fn() };
  const scheduler = { requestRender: vi.fn() };
  const container = document.createElement("div");
  document.body.appendChild(container);
  let editor: ReturnType<typeof useEditor> | undefined;
  const dispose = render(
    () => (
      <EditorProvider workspace={ws} renderer={renderer as never} scheduler={scheduler as never}>
        {(() => {
          editor = useEditor();
          return <TextOptionBar />;
        })()}
      </EditorProvider>
    ),
    container,
  );
  const select = async (id: string) => {
    editor!.setSelectedLayerId(id);
    editor!.setSelectedLayerIds([id]);
    await tick();
  };
  return {
    engine: ws.getActiveEngine() as DocumentEngine,
    container,
    dispose,
    editor: () => editor!,
    history: () => ws.getActiveHistory()!,
    renderer,
    select,
  };
}

function italicButton(container: HTMLElement): HTMLButtonElement {
  const bar = container.querySelector("[data-text-option-bar]");
  const button = bar?.querySelector<HTMLButtonElement>('button[aria-label="Italic"]');
  if (!button) throw new Error("text option bar italic button not rendered");
  return button;
}

const textDataOf = (bar: Bar, id: string): TextData => bar.engine.getLayer(id)!.textData!;

// The native engine only learns a layer through a protocol arm, so the model's
// layers are pushed one addLayer command each under their own ids; the projection
// then marks them owned.
async function pushModelIntoNativeEngine(engine: DocumentEngine): Promise<void> {
  let version = 0;
  const layers = engine.getLayers();
  for (let index = 0; index < layers.length; index++) {
    const l = layers[index];
    const res = await applyCommand({
      contractVersion: CONTRACT_VERSION,
      expectedVersion: version,
      docId: engine.getId(),
      command: {
        type: "addLayer",
        id: l.id,
        name: l.name,
        width: l.width,
        height: l.height,
        index,
        ...(l.type === "text" ? { layerType: "text", textData: l.textData } : {}),
      } as never,
    });
    version = res.delta.version;
  }
}

async function openBar(opts: { pushedToEngine: boolean }) {
  const ws = new WorkspaceManager();
  const session = WorkspaceManager.createBlankDocument("bar-text-params", "Text Params", DOC_W, DOC_H);
  ws.addDocument(session);
  const engine = session.engine;
  const data: TextData = { ...DEFAULT_TEXT_DATA, content: "hello", boxMode: "area", boxWidth: 200, boxHeight: 100 };
  const layer = engine.addTextLayer("Text", data);

  if (opts.pushedToEngine) {
    await pushModelIntoNativeEngine(engine);
    const snapshot = await getSnapshot(engine.getId());
    engine.applyFacadeSnapshot(snapshot as never);
    getFacade(engine.getId()).syncRenderedVersionTo(snapshot.version);
  } else {
    // Owned without the native engine holding it: the facade snapshot carries the
    // model, and applyFacadeSnapshot marks every cache-vector id owned.
    const facade = getFacade(engine.getId());
    await seedFacadeFromEngine(engine as never, facade);
    engine.applyFacadeSnapshot(facade.snapshot as never);
  }

  const bar = mountBar(ws);
  await bar.select(layer.id);
  applySpy.mockClear();
  return { bar, layerId: layer.id };
}

beforeAll(async () => {
  await getWasmExportModule();
});

beforeEach(() => {
  localStorage.clear();
  localStorage.setItem("photrez.facade", "1");
  __resetFacadeRegistryForTests();
  (globalThis as unknown as Record<string, () => void>).__clearFacadeOwnedForTests?.();
  toastMock.mockClear();
  applySpy = vi.spyOn(bridge, "applyCommand");
});

afterEach(() => {
  localStorage.removeItem("photrez.facade");
  __resetFacadeRegistryForTests();
  vi.restoreAllMocks();
  document.body.replaceChildren();
});

describe("text option bar - facade-owned layer", () => {
  it("dispatches ONE params command, writes no TS entry, and lands the model on the arm's value", async () => {
    const { bar, layerId } = await openBar({ pushedToEngine: true });

    italicButton(bar.container).click();
    await flush();

    expect(dispatchedTypes().filter((t) => t === "setLayerParams")).toHaveLength(1);
    expect(bar.history().getUndoCount()).toBe(0);

    const snapshot = await getSnapshot(bar.engine.getId());
    const native = (snapshot as { layers: Array<{ id: string; textData?: TextData }> }).layers.find((l) => l.id === layerId)!;
    expect(native.textData!.fontStyle).toBe("italic");
    expect(textDataOf(bar, layerId).fontStyle).toBe("italic");
    // No mismatch toast: a false "does not hold this layer" would mean the
    // patched-field equality check is too strict for the real round trip.
    expect(toastMock).not.toHaveBeenCalled();
    bar.dispose();
  });

  it("a layer the native engine never received: refused out loud, the intended edit is kept", async () => {
    const { bar, layerId } = await openBar({ pushedToEngine: false });
    expect(textDataOf(bar, layerId).fontStyle).toBe("normal");

    italicButton(bar.container).click();
    await flush();

    expect(dispatchedTypes().filter((t) => t === "setLayerParams")).toHaveLength(1);
    expect(bar.history().getUndoCount()).toBe(0);
    // The arm restated nothing, so the settled read is the pre-edit value. For an id
    // the engine does not hold, the user's edit is kept and the miss is surfaced:
    // silently writing the old value back loses the edit the UI just reported.
    expect(textDataOf(bar, layerId).fontStyle).toBe("italic");
    expect(toastMock).toHaveBeenCalledWith(
      expect.stringContaining("does not hold this layer"),
      "error",
    );
    bar.dispose();
  });
});

describe("text option bar - live edit session on the same layer", () => {
  it("live-mutates and re-uploads the raster with no history entry and no command", async () => {
    const { bar, layerId } = await openBar({ pushedToEngine: false });
    const engine = bar.engine;
    bar.editor().setTextEditSession({
      layerId,
      docX: 0,
      docY: 0,
      boxMode: "area",
      boxWidth: 200,
      boxHeight: 100,
      isNewLayer: false,
      preSnapshot: engine.snapshot(),
    });

    italicButton(bar.container).click();
    await flush();

    // The session owns the value until it closes, so no history entry and no
    // command: the session-close flush commits exactly once.
    expect(textDataOf(bar, layerId).fontStyle).toBe("italic");
    expect(bar.history().getUndoCount()).toBe(0);
    expect(dispatchedTypes().filter((t) => t === "setLayerParams")).toHaveLength(0);
    // The live tick must reach the raster: nothing else uploads a changed layer
    // texture after a model write.
    expect(bar.renderer.uploadImage).toHaveBeenCalledWith(layerId, expect.anything());
    bar.dispose();
  });
});

describe("text option bar - keeps photrez.facade-OFF behavior", () => {
  it("legacy path: one history entry before the model write, zero commands", async () => {
    const { bar, layerId } = await openBar({ pushedToEngine: false });
    localStorage.removeItem("photrez.facade");
    const history = bar.history();
    const commitSpy = vi.spyOn(history, "commit");
    const updateSpy = vi.spyOn(bar.engine, "updateTextData");

    italicButton(bar.container).click();
    await flush();

    expect(dispatchedTypes().filter((t) => t === "setLayerParams")).toHaveLength(0);
    expect(commitSpy).toHaveBeenCalledTimes(1);
    // The order is the contract: the snapshot handed to history must predate the
    // model write, or the recorded undo step restores the already-edited value.
    expect(commitSpy).toHaveBeenCalledBefore(updateSpy);
    expect(history.getUndoCount()).toBe(1);
    expect(textDataOf(bar, layerId).fontStyle).toBe("italic");
    bar.dispose();
  });
});
