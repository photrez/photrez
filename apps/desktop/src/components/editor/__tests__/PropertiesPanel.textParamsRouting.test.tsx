// Call-site wiring for the properties panel's text-params controls (italic here;
// the font size / weight / align / color / stroke / box-mode controls all funnel
// through the same commitTextDataEdit).
//
// With native authority enabled, editing a facade-owned text layer must go through
// ONE SetLayerParams command and write NO TS history entry: the native arm records
// the step, and a TS entry would strand an undo point whose engine.restore()
// rejects with E_FACADE_OWNED. On a layer the arm does not hold the command
// restates nothing, so the edit is refused out loud instead of silently dropped.
// The flag-OFF assertions pin the legacy commit-before-mutate path unchanged.
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi, type MockInstance } from "vitest";
import { render } from "solid-js/web";
import { PropertiesPanel } from "../PropertiesPanel";
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
vi.mock("@/lib/desktop/tauriWindow", () => ({ isTauriRuntime: vi.fn() }));

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

interface Panel {
  engine: DocumentEngine;
  container: HTMLElement;
  dispose: () => void;
  history: () => NonNullable<ReturnType<WorkspaceManager["getActiveHistory"]>>;
  select: (id: string) => Promise<void>;
}

function mountPanel(ws: WorkspaceManager): Panel {
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
          return <PropertiesPanel />;
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
    history: () => ws.getActiveHistory()!,
    select,
  };
}

function italicButton(container: HTMLElement): HTMLButtonElement {
  const button = container.querySelector<HTMLButtonElement>('button[aria-label="Italic"]');
  if (!button) throw new Error("italic button not rendered");
  return button;
}

const textDataOf = (panel: Panel, id: string): TextData => panel.engine.getLayer(id)!.textData!;

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

async function openPanel(opts: { pushedToEngine: boolean }) {
  const ws = new WorkspaceManager();
  const session = WorkspaceManager.createBlankDocument("panel-text-params", "Text Params", DOC_W, DOC_H);
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

  const panel = mountPanel(ws);
  await panel.select(layer.id);
  applySpy.mockClear();
  return { panel, layerId: layer.id };
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

describe("properties panel text params - owned layer", () => {
  it("dispatches ONE params command, writes no TS entry, and lands the model on the arm's value", async () => {
    const { panel, layerId } = await openPanel({ pushedToEngine: true });

    italicButton(panel.container).click();
    await flush();

    expect(dispatchedTypes().filter((t) => t === "setLayerParams")).toHaveLength(1);
    expect(panel.history().getUndoCount()).toBe(0);

    const snapshot = await getSnapshot(panel.engine.getId());
    const native = (snapshot as { layers: Array<{ id: string; textData?: TextData }> }).layers.find((l) => l.id === layerId)!;
    expect(native.textData!.fontStyle).toBe("italic");
    expect(textDataOf(panel, layerId).fontStyle).toBe("italic");
    // No mismatch toast: a false "does not hold this layer" would mean the
    // patched-field equality check is too strict for the real round trip.
    expect(toastMock).not.toHaveBeenCalled();
    panel.dispose();
  });

  it("a layer the native engine never received: refused out loud, model not moved", async () => {
    const { panel, layerId } = await openPanel({ pushedToEngine: false });
    expect(textDataOf(panel, layerId).fontStyle).toBe("normal");

    italicButton(panel.container).click();
    await flush();

    expect(dispatchedTypes().filter((t) => t === "setLayerParams")).toHaveLength(1);
    expect(textDataOf(panel, layerId).fontStyle).toBe("normal");
    expect(panel.history().getUndoCount()).toBe(0);
    expect(toastMock).toHaveBeenCalledWith(
      expect.stringContaining("does not hold this layer"),
      "error",
    );
    panel.dispose();
  });
});

describe("properties panel text params - flag OFF", () => {
  it("legacy path: one history entry before the model write, zero commands", async () => {
    const { panel, layerId } = await openPanel({ pushedToEngine: false });
    localStorage.removeItem("photrez.facade");
    const history = panel.history();
    const commitSpy = vi.spyOn(history, "commit");

    italicButton(panel.container).click();
    await flush();

    expect(dispatchedTypes().filter((t) => t === "setLayerParams")).toHaveLength(0);
    expect(commitSpy).toHaveBeenCalledTimes(1);
    expect(history.getUndoCount()).toBe(1);
    expect(textDataOf(panel, layerId).fontStyle).toBe("italic");
    panel.dispose();
  });
});
