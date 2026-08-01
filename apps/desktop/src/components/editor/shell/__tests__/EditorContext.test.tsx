import { describe, it, expect, vi } from "vitest";
import { render } from "solid-js/web";
import { createSignal, createEffect, type JSX } from "solid-js";
import { EditorProvider, useEditor } from "../EditorContext";
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
