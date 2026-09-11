// Flag-OFF legacy toast parity for the structural layer ops.
//
// When the facade flag is OFF the routed op defers to the byte-identical legacy
// helper. The keyboard handler must still surface the legacy failure toast that
// the original (pre-routing) handler showed - it must not be swallowed by the
// routing wrapper. (The flag-ON routed-success "never commits history" check is
// covered at the routing-function level in facadeStructuralOps.test.ts because
// mounting the full keyboard harness needs the native bridge mock.)
import { afterEach, describe, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import { EditorProvider } from "../../shell/EditorContext";
import { useCanvasKeyboard } from "../useCanvasKeyboard";
import { clearRegistry } from "../../keyboardRegistry";
import { WorkspaceManager } from "@/engine/workspace";
import * as Toast from "../../Toast";

function KeyboardHarness() {
  useCanvasKeyboard({
    isSpacePressed: () => false,
    setIsSpacePressed: vi.fn(),
    isAltPressed: () => false,
    setIsAltPressed: vi.fn(),
    isPanning: () => false,
    setIsPanning: vi.fn(),
    stopMomentum: vi.fn(),
    fitToScreenAndRender: vi.fn(),
    syncViewport: vi.fn(),
    getCanvasContainerRef: () => undefined,
  });
  return null;
}

function renderHarness(session: ReturnType<typeof WorkspaceManager.createBlankDocument>) {
  const ws = new WorkspaceManager();
  const renderer = { uploadImage: vi.fn(), destroyTexture: vi.fn() };
  const scheduler = { requestRender: vi.fn() };
  const container = document.createElement("div");
  document.body.appendChild(container);
  const dispose = render(
    () => (
      <EditorProvider workspace={ws} renderer={renderer as any} scheduler={scheduler as any}>
        <KeyboardHarness />
      </EditorProvider>
    ),
    container,
  );
  ws.addDocument(session);
  return {
    ws,
    renderer,
    scheduler,
    dispose: () => {
      dispose();
      container.parentNode?.removeChild(container);
    },
  };
}

describe("layer keyboard: flag-OFF legacy toast parity", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
    clearRegistry();
    localStorage.removeItem("photrez.facade");
  });

  it("Ctrl+Shift+E on a single-layer doc shows the legacy 'Could not flatten layers' warn toast", async () => {
    const session = WorkspaceManager.createBlankDocument("flatten-toast", "Flatten Toast", 800, 600);
    const { dispose } = renderHarness(session);
    // Flag OFF -> the routed op defers to the byte-identical legacy helper.
    localStorage.removeItem("photrez.facade");
    await new Promise((r) => setTimeout(r, 0));

    const toastSpy = vi.spyOn(Toast, "showToast");
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "e", ctrlKey: true, shiftKey: true, bubbles: true }));
    await new Promise((r) => setTimeout(r, 0));

    expect(toastSpy).toHaveBeenCalledWith("Could not flatten layers", "warn");
    dispose();
  });
});
