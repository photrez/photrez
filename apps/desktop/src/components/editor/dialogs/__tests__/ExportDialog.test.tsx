import { afterEach, describe, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import { EditorProvider, useEditor } from "../../shell/EditorContext";
import { ExportDialog } from "../ExportDialog";
import { WorkspaceManager } from "@/engine/workspace";
import { tick } from "@/lib/dom";

const exportActiveDocumentMock = vi.hoisted(() => vi.fn());
vi.mock("../../exportDocument", () => ({ exportActiveDocument: exportActiveDocumentMock }));
let setShowExport: (value: boolean) => void = () => {};

function TestConsumer() {
  setShowExport = useEditor().setShowExportDialog;
  return null;
}

function renderDialog(show = true) {
  const workspace = new WorkspaceManager();
  workspace.addDocument(WorkspaceManager.createBlankDocument("test", "Test Doc", 800, 600));
  const renderer = { uploadImage: vi.fn(), destroyTexture: vi.fn(), resize: vi.fn() };
  const scheduler = { requestRender: vi.fn() };
  const trigger = document.createElement("button");
  const container = document.createElement("div");
  document.body.append(trigger, container);
  trigger.focus();
  const disposeRoot = render(
    () => (
      <EditorProvider workspace={workspace} renderer={renderer as any} scheduler={scheduler as any}>
        <ExportDialog />
        <TestConsumer />
      </EditorProvider>
    ),
    container,
  );
  if (show) setShowExport(true);
  return {
    scheduler,
    trigger,
    dialog: () => document.querySelector<HTMLElement>('[data-dialog-kind="export"]'),
    dispose: () => {
      disposeRoot();
      trigger.remove();
      container.remove();
    },
  };
}

const button = (dialog: HTMLElement, label: string) => Array.from(dialog.querySelectorAll("button"))
  .find((candidate) => candidate.textContent?.trim() === label) as HTMLButtonElement;

describe("ExportDialog", () => {
  afterEach(() => {
    exportActiveDocumentMock.mockReset();
    document.querySelectorAll("[data-photrez-dialog], [data-dialog-backdrop]").forEach((node) => node.remove());
  });

  it("stays unmounted until requested", () => {
    const view = renderDialog(false);
    expect(view.dialog()).toBeNull();
    view.dispose();
  });

  it("uses the shared dialog and reveals quality only for lossy formats", async () => {
    const view = renderDialog();
    await tick();
    const dialog = view.dialog()!;
    expect(dialog.textContent).toContain("Export Image");
    const selectEl = dialog.querySelector<HTMLSelectElement>('#export-format-select')!;
    expect(selectEl.value).toBe("png");
    const formatBtn = dialog.querySelector<HTMLButtonElement>('button[aria-haspopup="listbox"]')!;
    expect(document.activeElement).toBe(formatBtn);
    expect(dialog.querySelector('input[type="range"]')).toBeNull();
    selectEl.value = "jpeg";
    selectEl.dispatchEvent(new Event("change", { bubbles: true }));
    expect(selectEl.value).toBe("jpeg");
    expect(dialog.querySelector<HTMLInputElement>('#export-quality')?.value).toBe("90");

    // TIFF is lossless → quality slider stays hidden
    selectEl.value = "tiff";
    selectEl.dispatchEvent(new Event("change", { bubbles: true }));
    expect(selectEl.value).toBe("tiff");
    expect(dialog.querySelector('input[type="range"]')).toBeNull();
    view.dispose();
  });

  it("exports through the production handler and reports the saved file", async () => {
    exportActiveDocumentMock.mockResolvedValue("./output/result.webp");
    const view = renderDialog();
    const dialog = view.dialog()!;
    const selectEl = dialog.querySelector<HTMLSelectElement>('#export-format-select')!;
    selectEl.value = "webp";
    selectEl.dispatchEvent(new Event("change", { bubbles: true }));
    button(dialog, "Export").click();
    await tick();
    expect(exportActiveDocumentMock).toHaveBeenCalledWith(expect.anything(), "Test Doc", "webp", 90);
    expect(view.dialog()?.querySelector('[role="status"]')?.textContent).toContain("result.webp");
    expect(view.scheduler.requestRender).toHaveBeenCalledOnce();
    expect(button(view.dialog()!, "Close")).toBeTruthy();
    view.dispose();
  });

  it("keeps the dialog open and exposes export errors accessibly", async () => {
    exportActiveDocumentMock.mockRejectedValue(new Error("Disk full"));
    const view = renderDialog();
    button(view.dialog()!, "Export").click();
    await tick();
    expect(view.dialog()?.querySelector('[role="alert"]')?.textContent).toContain("Disk full");
    expect(button(view.dialog()!, "Export").disabled).toBe(false);
    view.dispose();
  });

  it("locks dismissal and actions while an export is running", async () => {
    let finish!: (value: string | null) => void;
    exportActiveDocumentMock.mockReturnValue(new Promise((resolve) => { finish = resolve; }));
    const view = renderDialog();
    button(view.dialog()!, "Export").click();
    await tick();
    const dialog = view.dialog()!;
    expect(button(dialog, "Exporting...").disabled).toBe(true);
    expect(button(dialog, "Cancel").disabled).toBe(true);
    dialog.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    expect(view.dialog()).not.toBeNull();
    finish(null);
    await tick();
    expect(button(view.dialog()!, "Export").disabled).toBe(false);
    view.dispose();
  });

  it.each(["Cancel", "Escape", "Backdrop"])("dismisses via %s and restores focus", async (method) => {
    const view = renderDialog();
    await tick();
    const dialog = view.dialog()!;
    if (method === "Cancel") button(dialog, "Cancel").click();
    if (method === "Escape") document.activeElement?.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    if (method === "Backdrop") document.querySelector("[data-dialog-backdrop]")?.dispatchEvent(new MouseEvent("pointerdown", { bubbles: true }));
    await tick();
    expect(view.dialog()).toBeNull();
    expect(document.activeElement).toBe(view.trigger);
    view.dispose();
  });
});
