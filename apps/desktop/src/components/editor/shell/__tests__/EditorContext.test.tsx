import { describe, it, expect, vi } from "vitest";
import { render } from "solid-js/web";
import { createSignal, createEffect, type JSX } from "solid-js";
import { EditorProvider, useEditor } from "../EditorContext";
import { useEditorCore } from "../contexts/EditorCoreContext";
import { useToolSettings } from "../contexts/ToolSettingsContext";
import { useDocumentState } from "../contexts/DocumentStateContext";
import { useDialogChrome } from "../contexts/DialogChromeContext";
import { useHistoryDock } from "../contexts/HistoryDockContext";
import { WorkspaceManager } from "@/engine/workspace";

// Minimal Tauri mocks so EditorProvider's onMount chain (startup open,
// autosave wiring) does not touch a real webview in jsdom.
vi.mock("@tauri-apps/plugin-dialog", () => ({
  ask: vi.fn(),
  save: vi.fn(),
  open: vi.fn(),
}));
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue({ ok: true, contract_version: "2.0.0", data: {} }),
}));

function makeWorkspace() {
  const ws = new WorkspaceManager();
  const session = WorkspaceManager.createBlankDocument("ctx-doc", "Ctx Doc", 800, 600);
  ws.addDocument(session);
  return ws;
}

/** Reads the context flag reactively and reports every change. */
function Harness(props: { onValue: (v: boolean) => void }) {
  const editor = useEditor();
  createEffect(() => {
    props.onValue(editor.useGPUCameraForModernCrop());
  });
  return null;
}

function mount(children: () => JSX.Element) {
  const container = document.createElement("div");
  document.body.appendChild(container);
  const dispose = render(children, container);
  return { dispose, container };
}

describe("EditorProvider — GPU camera feature flag ownership (#24)", () => {
  it("defaults to true when the caller does not pass the flag props", () => {
    const values: boolean[] = [];
    const { dispose, container } = mount(() => (
      <EditorProvider workspace={makeWorkspace()} renderer={{ uploadImage: vi.fn() } as any} scheduler={{ requestRender: vi.fn() } as any}>
        <Harness onValue={(v) => values.push(v)} />
      </EditorProvider>
    ));

    expect(values[0]).toBe(true);

    dispose();
    container.remove();
  });

  it("uses the caller-owned signal passed via props and stays in sync with the caller's setter", async () => {
    const [flag, setFlag] = createSignal(false);
    const values: boolean[] = [];
    const { dispose, container } = mount(() => (
      <EditorProvider
        workspace={makeWorkspace()}
        renderer={{ uploadImage: vi.fn() } as any}
        scheduler={{ requestRender: vi.fn() } as any}
        useGPUCameraForModernCrop={flag}
        setUseGPUCameraForModernCrop={setFlag}
      >
        <Harness onValue={(v) => values.push(v)} />
      </EditorProvider>
    ));

    expect(values.at(-1)).toBe(false);
    setFlag(true);
    await Promise.resolve();
    expect(values.at(-1)).toBe(true);

    dispose();
    container.remove();
  });
});

describe("EditorProvider — domain context composition (#19)", () => {
  it("provisions all 5 domain contexts and the facade reads the same state", async () => {
    const seen: Record<string, boolean> = {};
    function Probe() {
      const editor = useEditor();
      const core = useEditorCore();
      const tool = useToolSettings();
      const doc = useDocumentState();
      const dialog = useDialogChrome();
      const dock = useHistoryDock();

      // Same underlying accessor functions are exposed per domain and via facade.
      seen.coreWorkspace = core.workspace === editor.workspace;
      seen.coreShowToast = core.showToast === editor.showToast;
      seen.toolActiveTool = tool.activeTool === editor.activeTool;
      seen.toolCropRect = tool.cropRect === editor.cropRect;
      seen.docDocuments = doc.documents === editor.documents;
      seen.docZoom = doc.zoom === editor.zoom;
      seen.dialogLoading = dialog.loadingMessage === editor.loadingMessage;
      seen.dockPanel = dock.rightDockPanel === editor.rightDockPanel;

      // Write via a domain setter, read via the facade (and vice versa) to prove
      // the facade reads the SAME signals the domain context exposes.
      tool.setActiveTool("brush");
      editor.setActiveTool("move");
      doc.setSelectedLayerId("layer-a");
      editor.setSelectedLayerId("layer-b");
      dialog.setLoadingMessage("loading");
      editor.setLoadingMessage("done");
      dock.setRightDockPanel("history");
      editor.setRightDockPanel("layers");
      core.setUseGPUCameraForModernCrop(false);
      editor.setUseGPUCameraForModernCrop(true);

      seen.facadeReadsTool = editor.activeTool() === "move";
      seen.domainReadsFacadeTool = tool.activeTool() === "move";
      seen.facadeReadsDoc = editor.selectedLayerId() === "layer-b";
      seen.domainReadsFacadeDoc = doc.selectedLayerId() === "layer-b";
      seen.facadeReadsDialog = editor.loadingMessage() === "done";
      seen.domainReadsFacadeDialog = dialog.loadingMessage() === "done";
      seen.facadeReadsDock = editor.rightDockPanel() === "layers";
      seen.domainReadsFacadeDock = dock.rightDockPanel() === "layers";
      seen.facadeReadsCore = editor.useGPUCameraForModernCrop() === true;
      seen.domainReadsFacadeCore = core.useGPUCameraForModernCrop() === true;

      return null;
    }

    const { dispose, container } = mount(() => (
      <EditorProvider workspace={makeWorkspace()} renderer={{ uploadImage: vi.fn() } as any} scheduler={{ requestRender: vi.fn() } as any}>
        <Probe />
      </EditorProvider>
    ));

    for (const [key, value] of Object.entries(seen)) {
      expect([key, value]).toEqual([key, true]);
    }

    dispose();
    container.remove();
  });

  it("throws a loud error when any domain hook is used outside EditorProvider", () => {
    const hooks: Array<[string, () => unknown]> = [
      ["useEditorCore", useEditorCore],
      ["useToolSettings", useToolSettings],
      ["useDocumentState", useDocumentState],
      ["useDialogChrome", useDialogChrome],
      ["useHistoryDock", useHistoryDock],
    ];

    for (const [name, hook] of hooks) {
      const container = document.createElement("div");
      function Probe() {
        hook();
        return null;
      }

      expect(() => render(() => Probe(), container)).toThrow(
        `${name} must be used within an EditorProvider`,
      );
      container.remove();
    }
  });

  it("exposes window.__photrezEditor composed from all 5 domain values (E2E contract)", () => {
    const { dispose, container } = mount(() => (
      <EditorProvider workspace={makeWorkspace()} renderer={{ uploadImage: vi.fn() } as any} scheduler={{ requestRender: vi.fn() } as any}>
        <Harness onValue={() => {}} />
      </EditorProvider>
    ));

    const handle = (window as unknown as { __photrezEditor?: ReturnType<typeof useEditor> }).__photrezEditor;
    expect(handle).toBeTruthy();
    // One representative member per domain proves the handle is composed from all 5.
    expect(typeof handle!.setActiveTool).toBe("function"); // ToolSettings
    expect(handle!.documents).toBeTypeOf("function"); // DocumentState
    expect(typeof handle!.setLoadingMessage).toBe("function"); // DialogChrome
    expect(handle!.rightDockPanel).toBeTypeOf("function"); // HistoryDock
    expect(handle!.workspace).toBeInstanceOf(WorkspaceManager); // EditorCore

    dispose();
    container.remove();
  });
});
