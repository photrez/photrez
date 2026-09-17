// Call-site wiring for the properties panel's text-params controls (italic here;
// the font size / weight / align / color / stroke / box-mode controls all funnel
// through the same commitTextDataEdit).
//
// With native authority enabled, editing a facade-owned text layer must go through
// ONE SetLayerParams command and write NO TS history entry: the native arm records
// the step, and a TS entry would strand an undo point whose engine.restore()
// rejects with E_FACADE_OWNED. On a layer the arm does not hold the command
// restates nothing, so the edit is kept and the miss is reported out loud.
// The flag-OFF assertions pin the legacy commit-before-mutate path unchanged.
//
// The color picker drives the same commitTextDataEdit, but it emits onChange on
// every HSV tick and once at mount, so its ticks stay transient on a routed layer
// (model + raster only) and ONE command fires when the dialog resolves - a command
// per tick would share one expectedVersion and the arm would reject all but the
// first.
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi, type MockInstance } from "vitest";
import { render } from "solid-js/web";
import { PropertiesPanel } from "../PropertiesPanel";
import { EditorProvider, useEditor } from "../shell/EditorContext";
import { WorkspaceManager } from "@/engine/workspace";
import type { DocumentEngine } from "@/engine/document";
import { DEFAULT_TEXT_DATA, type TextData } from "@/engine/textTypes";
import { sameParams } from "../layers/paramsRouting";
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

function strokeToggleButton(container: HTMLElement): HTMLButtonElement {
  const button = container.querySelector<HTMLButtonElement>('button[aria-label="Toggle stroke"]');
  if (!button) throw new Error("stroke toggle not rendered");
  return button;
}

function textColorButton(container: HTMLElement): HTMLButtonElement {
  const button = container.querySelector<HTMLButtonElement>('button[aria-label="Text color"]');
  if (!button) throw new Error("text color button not rendered");
  return button;
}

// The color picker is the real dialog from DialogProvider (EditorProvider wraps the
// panel in it), so these drive the same DOM the user does.
function dialogHexInput(): HTMLInputElement {
  const input = document.querySelector<HTMLInputElement>('[role="dialog"] input[type="text"]');
  if (!input) throw new Error("color picker dialog is not open");
  return input;
}

function dialogButton(label: string): HTMLButtonElement {
  const button = Array.from(document.querySelectorAll<HTMLButtonElement>('[role="dialog"] button')).find(
    (b) => b.textContent?.trim() === label,
  );
  if (!button) throw new Error(`dialog button "${label}" not rendered`);
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
  localStorage.setItem("photrez.facadeAuthority", "wasm");
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

  it("a layer the native engine never received: refused out loud, the pre-edit value stays", async () => {
    const { panel, layerId } = await openPanel({ pushedToEngine: false });
    expect(textDataOf(panel, layerId).fontStyle).toBe("normal");

    italicButton(panel.container).click();
    await flush();

    expect(dispatchedTypes().filter((t) => t === "setLayerParams")).toHaveLength(1);
    expect(panel.history().getUndoCount()).toBe(0);
    // The funnel's settled-value check throws before the caller can report
    // applied, so the model keeps the pre-edit value and the miss is toasted.
    expect(textDataOf(panel, layerId).fontStyle).toBe("normal");
    expect(toastMock).toHaveBeenCalledWith(
      expect.stringContaining("does not hold this layer"),
      "error",
    );
    panel.dispose();
  });

  it("a nested stroke patch is not a mismatch: the restatement's extra null field is absence", async () => {
    const { panel, layerId } = await openPanel({ pushedToEngine: true });
    expect(textDataOf(panel, layerId).stroke.width).toBe(0);

    // The toggle sends { stroke: { width, color } } (no align), while the native
    // TextStroke rounds back with align: null - a 3-key restatement of a 2-key
    // patch. Key-count equality would read that as "the engine does not hold this
    // layer" and refuse a stroke on a layer it does hold.
    strokeToggleButton(panel.container).click();
    await flush();

    expect(dispatchedTypes().filter((t) => t === "setLayerParams")).toHaveLength(1);
    expect(toastMock).not.toHaveBeenCalled();
    expect(textDataOf(panel, layerId).stroke.width).toBe(4);

    const snapshot = await getSnapshot(panel.engine.getId());
    const native = (snapshot as { layers: Array<{ id: string; textData?: TextData }> }).layers.find((l) => l.id === layerId)!;
    expect(native.textData!.stroke.width).toBe(4);
    panel.dispose();
  });
});

// The comparator both verify sites share (paramsRouting, ShapeOptionBar) and the
// overlay's box check uses. Keys are compared as the UNION of both sides, null and
// undefined are the same absence, nested objects recurse, and a non-finite number
// never compares equal - not even to itself, because a settled NaN/Infinity is not
// a value any renderer can use and the caller has to surface it.
describe("sameParams", () => {
  it("key union: an extra null field on one side still matches", () => {
    expect(sameParams({ width: 4, color: "#000000" }, { width: 4, color: "#000000", align: null })).toBe(true);
    expect(sameParams({ width: 4, color: "#000000", align: null }, { width: 4, color: "#000000" })).toBe(true);
  });

  it("null and undefined are the same absence", () => {
    expect(sameParams({ align: null }, {})).toBe(true);
    expect(sameParams({ align: undefined }, { align: null })).toBe(true);
    expect(sameParams(null, undefined)).toBe(true);
  });

  it("an absent member on one side is a mismatch when the other side has a value", () => {
    expect(sameParams({ width: 4 }, { width: 4, align: "outside" })).toBe(false);
    expect(sameParams({ align: "outside" }, {})).toBe(false);
  });

  it("nested objects are compared field by field, not by reference", () => {
    expect(sameParams({ stroke: { width: 4, color: "#000000" } }, { stroke: { width: 4, color: "#000000", align: null } })).toBe(true);
    expect(sameParams({ stroke: { width: 4 } }, { stroke: { width: 5 } })).toBe(false);
  });

  it("layer width/height divergence is a mismatch", () => {
    expect(sameParams({ width: 100, height: 50 }, { width: 120, height: 50 })).toBe(false);
  });

  it("non-finite numbers never compare equal, not even to themselves", () => {
    expect(sameParams(Number.NaN, Number.NaN)).toBe(false);
    expect(sameParams(Number.POSITIVE_INFINITY, Number.POSITIVE_INFINITY)).toBe(false);
    expect(sameParams({ boxWidth: Number.NaN }, { boxWidth: Number.NaN })).toBe(false);
    expect(sameParams(0, -0)).toBe(true);
  });

  it("scalars compare by value", () => {
    expect(sameParams("italic", "italic")).toBe(true);
    expect(sameParams("italic", "normal")).toBe(false);
    expect(sameParams({ fontSize: 48 }, { fontSize: 48 })).toBe(true);
  });
});

describe("properties panel text color picker - owned layer", () => {
  it("same-tick HSV ticks stay on the model, and the dialog boundary dispatches ONE command", async () => {
    const { panel, layerId } = await openPanel({ pushedToEngine: true });
    const before = textDataOf(panel, layerId).color;
    expect(before).toBe(DEFAULT_TEXT_DATA.color);

    textColorButton(panel.container).click();
    await flush();
    // Opening the picker emits one onChange at mount. Committing there would send a
    // command for a color the user never picked.
    expect(dispatchedTypes().filter((t) => t === "setLayerParams")).toHaveLength(0);

    const hex = dialogHexInput();
    for (const value of ["ff0000", "00ff00", "0000ff"]) {
      hex.value = value;
      hex.dispatchEvent(new Event("input", { bubbles: true }));
    }
    await flush();

    // All three ticks land in the same tick. Dispatching per tick would build every
    // envelope from the same expectedVersion and the arm would reject all but the
    // first, so the burst of ticks must reach the model only.
    expect(dispatchedTypes().filter((t) => t === "setLayerParams")).toHaveLength(0);
    expect(panel.history().getUndoCount()).toBe(0);
    const previewed = textDataOf(panel, layerId).color;
    expect(previewed).toBe("#0000ff");

    dialogButton("OK").click();
    await flush();

    expect(dispatchedTypes().filter((t) => t === "setLayerParams")).toHaveLength(1);
    const snapshot = await getSnapshot(panel.engine.getId());
    const native = (snapshot as { layers: Array<{ id: string; textData?: TextData }> }).layers.find((l) => l.id === layerId)!;
    expect(native.textData!.color).toBe("#0000ff");
    expect(textDataOf(panel, layerId).color).toBe("#0000ff");
    expect(toastMock).not.toHaveBeenCalled();
    panel.dispose();
  });

  it("cancel rolls the previewed color back: no command, the model at its starting color", async () => {
    const { panel, layerId } = await openPanel({ pushedToEngine: true });
    const before = textDataOf(panel, layerId).color;

    textColorButton(panel.container).click();
    await flush();
    const hex = dialogHexInput();
    hex.value = "ff0000";
    hex.dispatchEvent(new Event("input", { bubbles: true }));
    await flush();
    expect(textDataOf(panel, layerId).color).toBe("#ff0000");

    dialogButton("Cancel").click();
    await flush();

    expect(dispatchedTypes().filter((t) => t === "setLayerParams")).toHaveLength(0);
    expect(textDataOf(panel, layerId).color).toBe(before);
    panel.dispose();
  });
});

describe("properties panel text params - keeps photrez.facade=0 opt-out behavior", () => {
  it("legacy path: one history entry before the model write, zero commands", async () => {
    const { panel, layerId } = await openPanel({ pushedToEngine: false });
    localStorage.setItem("photrez.facade", "0");
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
