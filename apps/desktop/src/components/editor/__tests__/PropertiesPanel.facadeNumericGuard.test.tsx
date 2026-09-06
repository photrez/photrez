// Regression coverage for the async facade migration leak (reviewer find).
// Before the fix, PropertiesPanel.commitTransform() did
// `const ok = facadeCommitNumericTransform(engine, id, patch);` (a Promise) and then
// `if (!ok) return false;` — a Promise is always truthy, so the guard was dead and the
// success render/notify path fired even when the underlying command FAILED.
// This test proves the guard is live: when facadeCommitNumericTransform resolves false,
// commitTransform returns false and the success path (scheduler.requestRender /
// workspace.notifyVisualChange) is NOT executed.
import { afterEach, describe, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import { EditorProvider, useEditor } from "../shell/EditorContext";
import { PropertiesPanel } from "../PropertiesPanel";
import { WorkspaceManager } from "@/engine/workspace";
import * as Toast from "../Toast";

vi.mock("@/engine/document", async (importOriginal) => ({
  ...(await importOriginal<Record<string, unknown>>()),
  isFacadeOwnedLayer: () => true,
}));

vi.mock("@/lib/protocol/facadeRegistry", () => ({
  isFacadeEnabled: () => true,
  facadeCommitNumericTransform: vi.fn(() => Promise.resolve(false)),
  opacityPreview: () => null,
  setOpacityPreview: vi.fn(),
  clearOpacityPreview: vi.fn(),
  commitFacadeOpacity: vi.fn(),
}));

function renderWithSelectedLayer(workspace: WorkspaceManager, layerId: string) {
  const renderer = {
    uploadImage: vi.fn(),
    destroyTexture: vi.fn(),
    resize: vi.fn(),
    resizeToViewport: vi.fn(),
  };
  const scheduler = { requestRender: vi.fn() };
  const container = document.createElement("div");
  document.body.appendChild(container);
  const dispose = render(
    () => (
      <EditorProvider workspace={workspace} renderer={renderer as any} scheduler={scheduler as any}>
        <SelectedLayerHarness layerId={layerId} />
      </EditorProvider>
    ),
    container,
  );
  return { container, dispose, scheduler };
}

function SelectedLayerHarness(props: { layerId: string }) {
  const editor = useEditor();
  editor.setSelectedLayerId(props.layerId);
  return <PropertiesPanel />;
}

function clickButton(container: HTMLElement, aria: string) {
  const btn = container.querySelector<HTMLButtonElement>(`button[aria-label='${aria}']`);
  btn?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
  return btn;
}

describe("PropertiesPanel facade numeric-transform failure guard (async leak regression)", () => {
  afterEach(() => {
    document.body.replaceChildren();
    vi.restoreAllMocks();
  });

  it("Flip H on a facade-owned layer: failure guard returns false and skips the success render/notify path", async () => {
    const { facadeCommitNumericTransform } = await import("@/lib/protocol/facadeRegistry");
    const commitSpy = vi.mocked(facadeCommitNumericTransform);

    const workspace = new WorkspaceManager();
    const session = WorkspaceManager.createBlankDocument("guard-doc", "Guard Doc", 100, 100);
    workspace.addDocument(session);
    const engine = session.engine;
    const layer = engine.getLayers()[0];

    const notifySpy = vi.spyOn(workspace, "notifyVisualChange");

    const { container, dispose, scheduler } = renderWithSelectedLayer(workspace, layer.id);

    const btn = clickButton(container, "Flip horizontal");
    expect(btn).toBeTruthy();

    // The commit is now async: flush the microtask so the awaited guard resolves.
    await new Promise((r) => setTimeout(r, 0));

    // The facade command path was taken (not the legacy engine path).
    expect(commitSpy).toHaveBeenCalledTimes(1);
    // Failure guard is LIVE: success path must NOT run.
    expect(scheduler.requestRender).not.toHaveBeenCalled();
    expect(notifySpy).not.toHaveBeenCalled();
    // Engine layer is untouched by the facade path (projection only; commit failed).
    expect(engine.getLayer(layer.id)!.transform.flipH).toBe(false);

    dispose();
  });

  it("Flip H on a facade-owned layer: a REJECTED commit is caught, surfaced as an error toast, and skips the success render/notify path", async () => {
    const { facadeCommitNumericTransform } = await import("@/lib/protocol/facadeRegistry");
    const commitSpy = vi.mocked(facadeCommitNumericTransform);
    // Mock-fidelity: the real facade path REJECTS (Rust Err / E_EXTERNAL_PENDING
    // pending-external barrier / serde-rejected Infinity), not resolve(false).
    commitSpy.mockReturnValue(Promise.reject(new Error("E_EXTERNAL_PENDING")) as never);

    const toastSpy = vi.spyOn(Toast, "showToast");

    // Capture unhandled rejections so we can prove commitTransform never rejects.
    const unhandled: unknown[] = [];
    const onUnhandled = (e: unknown) => unhandled.push(e);
    process.on("unhandledRejection", onUnhandled);

    const workspace = new WorkspaceManager();
    const session = WorkspaceManager.createBlankDocument("guard-doc-rej", "Guard Doc Rej", 100, 100);
    workspace.addDocument(session);
    const engine = session.engine;
    const layer = engine.getLayers()[0];

    const notifySpy = vi.spyOn(workspace, "notifyVisualChange");

    const { container, dispose, scheduler } = renderWithSelectedLayer(workspace, layer.id);

    commitSpy.mockClear();
    const btn = clickButton(container, "Flip horizontal");
    expect(btn).toBeTruthy();

    // The commit is async: flush the microtask so the awaited rejection settles.
    // Without the try/catch guard this rejection escapes as an unhandled rejection
    // (the flip handler discards the returned promise).
    await new Promise((r) => setTimeout(r, 0));

    process.off("unhandledRejection", onUnhandled);

    // (a) the component's handler completed without an unhandled rejection.
    expect(unhandled).toHaveLength(0);
    // (b) the rejection was surfaced as an error toast.
    expect(toastSpy).toHaveBeenCalledWith(expect.stringContaining("E_EXTERNAL_PENDING"), "error");
    // (c) the success side-effects did NOT fire.
    expect(commitSpy).toHaveBeenCalledTimes(1);
    expect(scheduler.requestRender).not.toHaveBeenCalled();
    expect(notifySpy).not.toHaveBeenCalled();
    // Engine layer is untouched (facade path only; rejection means no projection).
    expect(engine.getLayer(layer.id)!.transform.flipH).toBe(false);

    dispose();
  });
});
