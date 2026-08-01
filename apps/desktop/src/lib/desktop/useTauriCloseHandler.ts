import { onCleanup } from "solid-js";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import type { WorkspaceManager } from "@/engine/workspace";
import type { DialogContextValue } from "@/components/editor/dialogs/DialogProvider";

// Minimal dialog interface: only the two dialog methods the close flow needs
// are required (duck typing), so tests can pass any conforming object.
type CloseDialog = Pick<DialogContextValue, "confirm" | "confirmSave">;

/**
 * Handles Tauri window close requests with sequential save-confirm dialogs
 * for each dirty document.
 *
 * When the user tries to close the window via the OS button or custom
 * titlebar close, Rust's CloseRequested handler prevents the default
 * close and emits a "close-requested" event. This hook listens for that
 * event, iterates through dirty documents, and shows a dialog per doc.
 *
 * After all dirty docs are handled (or none exist), invoke("close_app")
 * is called which calls app.exit(0) on the Rust side, bypassing the
 * CloseRequested handler entirely.
 *
 * Only "Discard" and "Cancel" stop / continue the flow.
 * "Save" persists the document before moving to the next dirty doc.
 */
export function useTauriCloseHandler(
  workspace: WorkspaceManager,
  dialog: CloseDialog,
  _scheduler: { requestRender: () => void },
) {
  const handleCloseRequested = async () => {
    const summaries = workspace.getTabSummaries();
    const dirtyDocs = summaries.filter((s) => s.isDirty);

    for (const doc of dirtyDocs) {
      const session = workspace.getSession(doc.id);
      if (!session) continue;

      // Use confirmSave: "save" | "discard" | "cancel"
      let skipDoc = false;

      if (dialog.confirmSave) {
        const result = await dialog.confirmSave({
          title: "Unsaved Changes",
          message: `"${session.displayName}" has unsaved changes. Save before closing?`,
          saveLabel: "Save",
          discardLabel: "Don't Save",
          cancelLabel: "Cancel",
        });
        if (result === "cancel") {
          // Cancel → stop closing entirely
          return;
        } else if (result === "discard") {
          // Discard → skip this doc
          skipDoc = true;
        }
        // "save" → save below
      } else {
        const confirmed = await dialog.confirm({
          title: "Unsaved Changes",
          message: `"${session.displayName}" has unsaved changes. Discard them?`,
          confirmLabel: "Discard",
          cancelLabel: "Cancel",
          tone: "danger",
        });
        if (!confirmed) return; // Cancel
        skipDoc = true; // Discard
      }

      if (skipDoc) {
        session.dirty = false;
        continue;
      }

      // Save: try quick overwrite if sourcePath exists
      if (session.sourcePath) {
        try {
          const ext = session.sourcePath.split(".").pop()?.toLowerCase();
          if (session.sourcePath.endsWith(".ptz")) {
            const { serializeAndSaveProject } = await import("@/components/editor/projectSerialize");
            await serializeAndSaveProject(session.engine, session.sourcePath);
          } else {
            const { encodeComposite } = await import("@/components/editor/exportDocument");
            const ext = session.sourcePath.split(".").pop()?.toLowerCase();
            const format = ext === "jpg" || ext === "jpeg" ? "jpeg" as const
              : ext === "webp" ? "webp" as const : "png" as const;
            const bytes = await encodeComposite(session.engine, format, 92);
            const { writeFileBytes } = await import("@/tauri/native");
            await writeFileBytes(session.sourcePath, bytes);
          }
          session.dirty = false;
          session.engine.clearDirty();
        } catch {
          const retry = await dialog.confirm({
            title: "Save Failed",
            message: `Could not save "${session.displayName}". Discard changes anyway?`,
            confirmLabel: "Discard",
            cancelLabel: "Cancel",
            tone: "danger",
          });
          if (!retry) return; // Cancel
          // User chose to discard despite save failure
          session.dirty = false;
        }
      } else {
        // No source path — prompt user for destination
        const { showSaveDialogAllFormats, writeFileBytes } = await import("@/tauri/native");
        const path = await showSaveDialogAllFormats(`${session.displayName}.png`);
        if (!path) return; // User cancelled save dialog

        const ext = path.split(".").pop()?.toLowerCase();
        if (ext === "ptz") {
          const { serializeAndSaveProject } = await import("@/components/editor/projectSerialize");
          await serializeAndSaveProject(session.engine, path);
        } else {
          const { encodeComposite } = await import("@/components/editor/exportDocument");
          const format = ext === "jpg" || ext === "jpeg" ? "jpeg" as const
            : ext === "webp" ? "webp" as const : "png" as const;
          const bytes = await encodeComposite(session.engine, format, 92);
          await writeFileBytes(path, bytes);
        }
        session.sourcePath = path;
        session.displayName = path.split(/[/\\]/).pop() || session.displayName;
        session.dirty = false;
        session.engine.clearDirty();
      }
    }

    // All dirty docs handled — close the app via Rust app.exit().
    // invoke("close_app") bypasses the CloseRequested handler entirely.
    try {
      await invoke("close_app");
    } catch (error) {
      // Fallback for non-Tauri or test env
      console.warn("[useTauriCloseHandler] invoke('close_app') failed:", error);
    }
  };

  // Only set up listener in Tauri runtime
  if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    listen("close-requested", handleCloseRequested).then((fn) => {
      if (!cancelled) unlisten = fn;
    });
    onCleanup(() => {
      cancelled = true;
      unlisten?.();
    });
  }
}
