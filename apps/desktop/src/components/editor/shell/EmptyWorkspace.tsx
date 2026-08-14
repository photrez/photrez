import { createSignal, For } from "solid-js";
import { Icon } from "../icons";
import { useEditor } from "./EditorContext";
import { useDragController, dragDropEffect } from "../DragController";
import { WorkspaceManager } from "@/engine/workspace";
import { MAX_OPEN_DOCUMENTS } from "@/engine/types";
import { showToast } from "../Toast";
import { useDialog } from "../dialogs/DialogProvider";
import { openImageFilesAsDocuments } from "../editorOpenImage";
import { createNewDocFromLayerDrag } from "../crossDocLayerOps";

const PRESETS = [
  { label: "Instagram Post", width: 1080, height: 1080, tag: "1080 × 1080 px", icon: "square" },
  { label: "Story / Reels", width: 1080, height: 1920, tag: "1080 × 1920 px", icon: "smartphone" },
  { label: "Full HD Canvas", width: 1920, height: 1080, tag: "1920 × 1080 px", icon: "image" },
  { label: "A4 Print (300 DPI)", width: 2480, height: 3508, tag: "2480 × 3508 px", icon: "file-text" },
] as const;

export function EmptyWorkspace() {
  const { openImage, workspace, scheduler, renderer } = useEditor();
  const dialog = useDialog();
  const dragController = useDragController();
  const [fileDragOver, setFileDragOver] = createSignal(false);
  const [dragFileCount, setDragFileCount] = createSignal(0);

  const handleNewCanvas = async () => {
    if (workspace.isFull()) {
      showToast(`Workspace full: close a document first (max ${MAX_OPEN_DOCUMENTS})`, "error");
      return;
    }
    const result = await dialog.newDocument();
    if (result) {
      const id = `doc-${crypto.randomUUID()}`;
      const session = WorkspaceManager.createBlankDocument(id, result.name, result.width, result.height, { backgroundColor: result.backgroundColor });
      workspace.addDocument(session);
      scheduler.requestRender();
    }
  };

  const handleCreatePreset = (preset: typeof PRESETS[number]) => {
    if (workspace.isFull()) {
      showToast(`Workspace full: close a document first (max ${MAX_OPEN_DOCUMENTS})`, "error");
      return;
    }
    const id = `doc-${crypto.randomUUID()}`;
    const session = WorkspaceManager.createBlankDocument(id, preset.label, preset.width, preset.height, { backgroundColor: "white" });
    workspace.addDocument(session);
    scheduler.requestRender();
  };

  const onDragOver = (e: DragEvent) => {
    const dt = e.dataTransfer;
    if (!dt) return;
    if (dt.types.includes("Files")) {
      e.preventDefault();
      const count = dt.items.length || dt.files.length || 0;
      setDragFileCount(count);
      setFileDragOver(true);
      return;
    }
    // Layer drag onto the empty workspace seeds a brand-new document
    // (cross-doc), so show the copy/move cursor for it.
    const state = dragController.state();
    if (state.dragKind === "layer" && state.payload) {
      e.preventDefault();
      dt.dropEffect = dragDropEffect(state.payload, true);
    }
  };
  const onFileDragLeave = (e: DragEvent) => {
    const related = e.relatedTarget as Node | null;
    if (related && e.currentTarget instanceof Element && e.currentTarget.contains(related)) return;
    setFileDragOver(false);
    setDragFileCount(0);
  };
  const onFileDrop = async (e: DragEvent) => {
    const files = e.dataTransfer?.files;
    if (!files || files.length === 0) return;
    e.preventDefault();
    setFileDragOver(false);
    setDragFileCount(0);
    await openImageFilesAsDocuments(Array.from(files), {
      workspace,
      renderer,
      scheduler,
      onError: (m) => showToast(m, "error"),
    });
  };

  const onDrop = async (e: DragEvent) => {
    const state = dragController.state();
    if (state.dragKind === "layer" && state.payload) {
      // Dropping a layer onto an empty workspace seeds a brand-new document
      // from that layer (see createNewDocFromLayerDrag).
      e.preventDefault();
      createNewDocFromLayerDrag(state.payload, workspace, renderer, scheduler);
      dragController.endDrag();
      return;
    }
    await onFileDrop(e);
  };

  const armed = () => fileDragOver();
  const dropPrompt = () => {
    const n = dragFileCount();
    return n > 1 ? `Release to open ${n} images` : "Release to open image";
  };

  return (
    <div
      class="flex flex-1 items-center justify-center bg-editor-canvas p-6"
      onDragOver={onDragOver}
      onDragLeave={onFileDragLeave}
      onDrop={onDrop}
    >
      <section
        aria-label="Start workspace"
        class="flex w-full max-w-[520px] flex-col rounded-[6px] border border-editor-divider bg-editor-panel text-editor-text shadow-[0_12px_32px_rgba(0,0,0,0.8)] transition-[transform,box-shadow] duration-150 ease-out"
        classList={{
          "ring-2 ring-editor-accent ring-offset-2 ring-offset-editor-canvas -translate-y-0.5 shadow-[0_16px_44px_rgba(0,0,0,0.55)]": armed(),
        }}
      >
        <div class="border-b border-editor-divider px-5 py-4">
          <div class="flex items-center gap-3">
            <div
              class="flex size-9 items-center justify-center rounded-[4px] text-editor-text-dim transition-colors duration-150"
              classList={{
                "bg-editor-accent text-white": armed(),
                "bg-editor-field": !armed(),
              }}
            >
              <Icon name="image-plus" class="size-5" strokeWidth={1.5} />
            </div>
            <div class="min-w-0">
              <h2 class="text-[14px] font-semibold text-editor-text">Start a Photrez document</h2>
              <p class="mt-0.5 text-[12px] text-editor-text-dim">Open an image, drop one into the workspace, or create a blank canvas.</p>
            </div>
          </div>
        </div>

        <div
          class="grid grid-cols-2 gap-2.5 px-5 py-3.5 transition-opacity duration-150"
          classList={{ "opacity-50": armed() }}
        >
          <button
            type="button"
            onClick={openImage}
            class="flex h-9 w-full items-center justify-center gap-2 rounded-[4px] bg-editor-accent px-3 text-[13px] font-medium text-white transition-colors hover:bg-editor-accent/90 focus-visible:outline focus-visible:outline-1 focus-visible:outline-editor-accent"
          >
            <Icon name="folder-plus" class="size-4" strokeWidth={1.75} />
            Open Image
          </button>
          <button
            type="button"
            onClick={handleNewCanvas}
            class="flex h-9 w-full items-center justify-center gap-2 rounded-[4px] border border-editor-field-border bg-editor-field px-3 text-[13px] font-medium text-editor-text transition-colors hover:bg-white/[0.045] focus-visible:outline focus-visible:outline-1 focus-visible:outline-editor-accent"
          >
            <Icon name="plus" class="size-4" strokeWidth={1.75} />
            New Document
          </button>
        </div>

        <div class="border-t border-editor-divider px-5 py-3.5">
          <div class="mb-2.5 text-[11px] font-semibold text-editor-text">Quick Presets</div>
          <div class="grid grid-cols-2 gap-2">
            <For each={PRESETS}>
              {(preset) => (
                <button
                  type="button"
                  onClick={() => handleCreatePreset(preset)}
                  class="flex items-center gap-2.5 rounded-[4px] border border-editor-field-border bg-editor-field p-2 text-left transition-all hover:border-editor-accent/50 hover:bg-editor-accent/10 focus-visible:outline focus-visible:outline-1 focus-visible:outline-editor-accent group"
                >
                  <div class="flex size-7 shrink-0 items-center justify-center rounded-[3px] bg-editor-panel text-editor-text-dim group-hover:text-editor-accent transition-colors">
                    <Icon name={preset.icon as any} class="size-3.5" strokeWidth={1.75} />
                  </div>
                  <div class="min-w-0">
                    <div class="text-[12px] font-medium text-editor-text truncate group-hover:text-white transition-colors">
                      {preset.label}
                    </div>
                    <div class="text-[10px] text-editor-text-dim truncate">
                      {preset.tag}
                    </div>
                  </div>
                </button>
              )}
            </For>
          </div>
        </div>

        <div class="flex items-center justify-between border-t border-editor-divider px-5 py-3 text-[11px] text-editor-text-dim">
          <span>{armed() ? dropPrompt() : "Drop PNG, JPEG, or WebP files anywhere in this workspace."}</span>
          <kbd class="font-sans rounded-[3px] border border-editor-divider bg-editor-field px-1.5 py-0.5 text-[10px] text-editor-text-dim">Ctrl+O</kbd>
        </div>
      </section>
    </div>
  );
}
