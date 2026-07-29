import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render } from "solid-js/web";
import { EditorProvider } from "@/components/editor/shell/EditorContext";
import { WorkspaceManager } from "@/engine/workspace";
import { useEditorCommands, dispatchEditorCommand } from "../useEditorCommands";
import type { EditorCommand } from "../useEditorCommands";
import { setSaveProgress, cancelPendingSaveDismiss } from "../saveState";

// Mock only encodeComposite so no real canvas/OffscreenCanvas work happens.
// getSavedQuality/setSavedQuality stay real (use jsdom localStorage).
vi.mock("../exportDocument", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../exportDocument")>();
  return {
    ...actual,
    encodeComposite: vi.fn().mockResolvedValue(new Uint8Array([1, 2, 3])),
  };
});

// Mock native write path.
vi.mock("@/tauri/native", () => ({
  writeFileBytes: vi.fn().mockResolvedValue(undefined),
  showSaveDialog: vi.fn(),
  showSaveDialogAllFormats: vi.fn(),
}));

// Controllable dialog.quality.
const qualitySpy = vi.fn();
vi.mock("@/components/editor/dialogs/DialogProvider", () => ({
  DialogProvider: (props: { children: unknown }) => props.children,
  useDialog: () => ({
    quality: qualitySpy,
    confirm: vi.fn(),
    alert: vi.fn(),
    confirmWithCheckbox: vi.fn(),
    confirmSave: vi.fn(),
    colorPicker: vi.fn(),
    newDocument: vi.fn().mockResolvedValue(null),
  }),
}));

function makeSession(sourcePath: string) {
  const ws = new WorkspaceManager();
  const session = WorkspaceManager.createBlankDocument("save-doc", "Save Doc", 800, 600);
  ws.addDocument(session);
  session.sourcePath = sourcePath;
  session.displayName = sourcePath.split(/[/\\]/).pop() || sourcePath;
  return { ws, session };
}

function harness() {
  useEditorCommands(() => undefined);
  return null;
}

async function run(command: EditorCommand) {
  dispatchEditorCommand(command);
  // flush the async save IIFE + dialog promise
  await new Promise((r) => setTimeout(r, 0));
  await new Promise((r) => setTimeout(r, 0));
}

describe("file.save quality prompt (lossy vs lossless)", () => {
  beforeEach(() => {
    qualitySpy.mockReset();
    vi.clearAllMocks();
    localStorage.clear();
  });
  afterEach(() => {
    vi.restoreAllMocks();
    // Reset manual-save state so the save-queue guard doesn't
    // carry over from a previous test (module-level signal leak).
    cancelPendingSaveDismiss();
    setSaveProgress({ phase: "idle", label: "", fraction: 0 });
  });

  it("asks quality for JPEG and passes it to encodeComposite", async () => {
    const { ws, session } = makeSession("C:/img/save.jpg");
    qualitySpy.mockResolvedValue(80);
    const { writeFileBytes } = await import("@/tauri/native");
    const { encodeComposite } = await import("../exportDocument");

    const container = document.createElement("div");
    document.body.appendChild(container);
    const dispose = render(
      () => (
        <EditorProvider workspace={ws} renderer={{ uploadImage: vi.fn() } as any} scheduler={{ requestRender: vi.fn() } as any}>
          {harness()}
        </EditorProvider>
      ),
      container,
    );

    await run("file.save");

    expect(qualitySpy).toHaveBeenCalledTimes(1);
    expect(qualitySpy).toHaveBeenCalledWith(expect.objectContaining({ format: "jpeg", defaultQuality: 92 }));
    expect(encodeComposite).toHaveBeenCalledWith(session.engine, "jpeg", 80);
    expect(writeFileBytes).toHaveBeenCalledWith("C:/img/save.jpg", expect.any(Uint8Array));

    dispose();
    container.remove();
  });

  it("does NOT ask quality for PNG (lossless) and saves directly", async () => {
    const { ws } = makeSession("C:/img/save.png");
    const { writeFileBytes } = await import("@/tauri/native");
    const { encodeComposite } = await import("../exportDocument");

    const container = document.createElement("div");
    document.body.appendChild(container);
    const dispose = render(
      () => (
        <EditorProvider workspace={ws} renderer={{ uploadImage: vi.fn() } as any} scheduler={{ requestRender: vi.fn() } as any}>
          {harness()}
        </EditorProvider>
      ),
      container,
    );

    await run("file.save");

    expect(qualitySpy).not.toHaveBeenCalled();
    expect(encodeComposite).toHaveBeenCalledWith(expect.anything(), "png", 92);
    expect(writeFileBytes).toHaveBeenCalledWith("C:/img/save.png", expect.any(Uint8Array));

    dispose();
    container.remove();
  });

  it("cancelling the quality dialog aborts the save (no write)", async () => {
    const { ws } = makeSession("C:/img/save.webp");
    qualitySpy.mockResolvedValue(null); // user cancelled
    const { writeFileBytes } = await import("@/tauri/native");
    const { encodeComposite } = await import("../exportDocument");

    const container = document.createElement("div");
    document.body.appendChild(container);
    const dispose = render(
      () => (
        <EditorProvider workspace={ws} renderer={{ uploadImage: vi.fn() } as any} scheduler={{ requestRender: vi.fn() } as any}>
          {harness()}
        </EditorProvider>
      ),
      container,
    );

    await run("file.save");

    expect(qualitySpy).toHaveBeenCalledTimes(1);
    expect(encodeComposite).not.toHaveBeenCalled();
    expect(writeFileBytes).not.toHaveBeenCalled();

    dispose();
    container.remove();
  });

  it("persists chosen quality and skips the dialog on the next save of the same format", async () => {
    const { ws, session } = makeSession("C:/img/persist.jpg");
    qualitySpy.mockResolvedValue(75);
    const { writeFileBytes } = await import("@/tauri/native");
    const { encodeComposite, getSavedQuality } = await import("../exportDocument");

    const container = document.createElement("div");
    document.body.appendChild(container);
    const dispose = render(
      () => (
        <EditorProvider workspace={ws} renderer={{ uploadImage: vi.fn() } as any} scheduler={{ requestRender: vi.fn() } as any}>
          {harness()}
        </EditorProvider>
      ),
      container,
    );

    // First save: dialog shown, choice persisted.
    await run("file.save");
    expect(qualitySpy).toHaveBeenCalledTimes(1);
    expect(getSavedQuality("jpeg")).toBe(75);
    expect(encodeComposite).toHaveBeenLastCalledWith(session.engine, "jpeg", 75);

    // Second save (same session, same format): no dialog, uses saved value.
    qualitySpy.mockClear();
    await run("file.save");
    expect(qualitySpy).not.toHaveBeenCalled();
    expect(encodeComposite).toHaveBeenLastCalledWith(session.engine, "jpeg", 75);
    expect(writeFileBytes).toHaveBeenCalledTimes(2);

    dispose();
    container.remove();
  });

  it("keeps per-format quality independent (jpg vs webp)", async () => {
    // First save a jpg at 70 → persisted for jpeg only.
    const jpg = makeSession("C:/img/a.jpg");
    qualitySpy.mockResolvedValue(70);
    const c1 = document.createElement("div");
    document.body.appendChild(c1);
    const d1 = render(
      () => (
        <EditorProvider workspace={jpg.ws} renderer={{ uploadImage: vi.fn() } as any} scheduler={{ requestRender: vi.fn() } as any}>
          {harness()}
        </EditorProvider>
      ),
      c1,
    );
    await run("file.save");
    d1();
    c1.remove();

    const { getSavedQuality } = await import("../exportDocument");
    expect(getSavedQuality("jpeg")).toBe(70);
    expect(getSavedQuality("webp")).toBeNull(); // untouched

    // Now a webp save → still prompts (no saved webp value yet).
    const webp = makeSession("C:/img/b.webp");
    qualitySpy.mockClear();
    qualitySpy.mockResolvedValue(60);
    const c2 = document.createElement("div");
    document.body.appendChild(c2);
    const d2 = render(
      () => (
        <EditorProvider workspace={webp.ws} renderer={{ uploadImage: vi.fn() } as any} scheduler={{ requestRender: vi.fn() } as any}>
          {harness()}
        </EditorProvider>
      ),
      c2,
    );
    await run("file.save");
    expect(qualitySpy).toHaveBeenCalledTimes(1);
    expect(getSavedQuality("webp")).toBe(60);
    d2();
    c2.remove();
  });
});
