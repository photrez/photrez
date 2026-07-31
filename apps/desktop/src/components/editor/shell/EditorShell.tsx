import { createSignal, lazy, onCleanup, onMount, Show, Suspense } from "solid-js";
import { AppTitleBar } from "./AppTitleBar";
import { BottomStatusBar } from "./BottomStatusBar";
import { CanvasViewport } from "../canvas/CanvasViewport";
import { DocumentTabsBar } from "./DocumentTabsBar";
import { LeftToolRail } from "./LeftToolRail";
import { OptionBar } from "./OptionBar";
import { RightDock } from "./RightDock";
import { useDesktopGuards, useDesktopShortcuts } from "@/lib/desktop";
import { EmptyWorkspace } from "./EmptyWorkspace";
import { ErrorBoundary } from "../ErrorBoundary";

const ResizeCanvasModal = lazy(() => import("../dialogs/ResizeCanvasModal").then(m => ({ default: m.ResizeCanvasModal })));
const ExportDialog = lazy(() => import("../dialogs/ExportDialog").then(m => ({ default: m.ExportDialog })));
const PrintDialog = lazy(() => import("../dialogs/PrintDialog").then(m => ({ default: m.PrintDialog })));
import { ToastHost } from "../Toast";
import { LoadingOverlay } from "../LoadingOverlay";
import { saveProgress } from "../saveState";
import { GlobalDragDropHost } from "../GlobalDragDropHost";
import { DragGlobalGuard } from "../DragController";
import { destroySaveWorkerPool } from "../saveWorkerPool";
import { isTauriRuntime } from "@/lib/desktop/tauriWindow";
import { useTauriCloseHandler } from "@/lib/desktop/useTauriCloseHandler";
import { useDialog } from "../dialogs/DialogProvider";
import { cancelLayerTransformSession } from "../transformSession";

// Core singletons import
import { WorkspaceManager } from "@/engine/workspace";
import { WebGL2Backend } from "@/renderer/webgl2";
import { RenderScheduler } from "@/renderer/scheduler";
import { EditorProvider, useEditor, useGPUCameraForModernCrop } from "./EditorContext";
import { ViewportCamera } from "../../../viewport/viewportCamera";

function EditorLayout(props: {
  rightDockOpen: boolean;
  toggleRightDock: () => void;
  setRightDockOpen: (val: boolean) => void;
}) {
  const { documents, activeDocumentId, chromeVisible } = useEditor();
  const hasDocument = () => documents().length > 0;
  const hasActiveDocument = () => activeDocumentId() !== null;

  return (
    <div class="photrez-app flex h-dvh min-h-[640px] min-w-[960px] flex-col overflow-hidden bg-editor-bg text-editor-text">
      <AppTitleBar
        isRightDockOpen={props.rightDockOpen}
        onToggleRightDock={props.toggleRightDock}
      />

      <main class="relative flex min-h-0 flex-1 overflow-hidden">
        <Show when={chromeVisible()}>
          <LeftToolRail disabled={!hasActiveDocument()} />
        </Show>

        <div class="flex flex-1 min-w-0 overflow-hidden">
          <section class="flex min-w-0 flex-1 flex-col overflow-hidden">
            <Show when={hasDocument()}>
              <DocumentTabsBar />
            </Show>
            <Show when={chromeVisible() && hasActiveDocument()}>
              <OptionBar />
            </Show>
            <Show when={hasActiveDocument()} fallback={<EmptyWorkspace />}>
              <CanvasViewport />
            </Show>
          </section>

          <Show when={chromeVisible()}>
            <RightDock
              open={props.rightDockOpen}
              onClose={() => props.setRightDockOpen(false)}
            />
          </Show>
        </div>
      </main>

      <Show when={chromeVisible()}>
        <BottomStatusBar />
      </Show>
      <Suspense fallback={null}><ResizeCanvasModal /></Suspense>
      <Suspense fallback={null}><ExportDialog /></Suspense>
      <Suspense fallback={null}><PrintDialog /></Suspense>
      <LoadingOverlay />
      <ToastHost />
    </div>
  );
}

export function EditorShell() {
  const [rightDockOpen, setRightDockOpen] = createSignal(true); // Default open for premium dual panel layout
  const toggleRightDock = () => setRightDockOpen((open) => !open);

  useDesktopGuards();

  // ─── Singletons Initialization ───
  const workspace = new WorkspaceManager();
  const camera = new ViewportCamera();
  const renderer = new WebGL2Backend();
  // Render scheduler uses the shared useGPUCameraForModernCrop signal
  // (defined at module level in EditorContext) so it stays in sync
  // with the context flag. When the flag is true, the new GPU-camera
  // path always passes a VP matrix (image transform is in the matrix).
  // When false, falls back to the legacy CSS-path conditional.
  const scheduler = new RenderScheduler(() => {
    const engine = workspace.getActiveEngine();
    if (!engine) return;
    if (useGPUCameraForModernCrop()) {
      const matrix = camera.getViewProjectionMatrix();
      renderer.render(engine.getRenderState(), matrix);
    } else if (camera.isModernCropActive) {
      renderer.render(engine.getRenderState());
    } else {
      const matrix = camera.getViewProjectionMatrix();
      renderer.render(engine.getRenderState(), matrix);
    }
  });

  // Fallback: browser beforeunload for non-Tauri environments (web preview, etc.)
  if (!isTauriRuntime()) {
    onMount(() => {
      const handleBeforeUnload = (e: BeforeUnloadEvent) => {
        if (workspace.getTabSummaries().some((t) => t.isDirty)) {
          e.preventDefault();
          e.returnValue = "";
        }
      };
      window.addEventListener("beforeunload", handleBeforeUnload);
      onCleanup(() => window.removeEventListener("beforeunload", handleBeforeUnload));
    });
  } else {
    // We are in Tauri environment. The UI is now hydrated and ready to be displayed.
    // Show the main window directly (no splashscreen — app launches fast enough
    // that a splashscreen only adds a black flash; Tauri guidance + KNOWN_ISSUES #1).
    onMount(() => {
      import("@tauri-apps/api/core").then(({ invoke }) => {
        invoke("close_splashscreen").catch(() => {
          /* splashscreen may already be absent; main window is visible by default */
        });
      }).catch(() => {});
    });
  }

  onCleanup(() => {
    scheduler.dispose();
    // Clean up worker pool to prevent orphan Workers in long-running sessions.
    destroySaveWorkerPool();
  });

  return (
    <EditorProvider
      workspace={workspace}
      renderer={renderer}
      scheduler={scheduler}
      camera={camera}
      rightDockOpen={rightDockOpen}
      setRightDockOpen={setRightDockOpen}
    >
      <GlobalDragDropHost />
      <DragGlobalGuard />
      <TauriCloseGuard workspace={workspace} scheduler={scheduler} />
      <DesktopShortcutsGuard onToggleRightDock={toggleRightDock} />
      <ErrorBoundary fallback={(err, reset) => (
        <div class="flex flex-1 items-center justify-center bg-editor-bg p-8">
          <div class="max-w-md rounded-lg border border-editor-divider bg-editor-panel p-6 shadow-lg">
            <h2 class="mb-2 text-lg font-semibold text-editor-accent">Editor Error</h2>
            <p class="mb-4 text-sm text-editor-text-dim">
              An unexpected error occurred in the editor. Other panels and tabs remain accessible.
            </p>
            <details class="text-xs text-editor-text-dim">
              <summary class="cursor-pointer hover:text-editor-text">Error details</summary>
              <pre class="mt-2 max-h-32 overflow-auto whitespace-pre-wrap rounded border border-editor-divider bg-editor-bg p-2 font-mono text-[11px]">{err.message}</pre>
            </details>
            <button
              type="button"
              class="mt-4 rounded-md bg-editor-accent px-4 py-2 text-sm font-medium text-white hover:opacity-90"
              onClick={() => reset()}
            >
              Try Again
            </button>
          </div>
        </div>
      )}>
        <EditorLayout
          rightDockOpen={rightDockOpen()}
          toggleRightDock={toggleRightDock}
          setRightDockOpen={setRightDockOpen}
        />
      </ErrorBoundary>
    </EditorProvider>
  );
}

/** Inner component that has access to useDialog() (DialogProvider is inside EditorProvider). */
function TauriCloseGuard(props: {
  workspace: import("@/engine/workspace").WorkspaceManager;
  scheduler: import("@/renderer/scheduler").RenderScheduler;
}) {
  const dialog = useDialog();
  useTauriCloseHandler(props.workspace, dialog as any, props.scheduler);
  return null;
}

/** Inner component that wires desktop shortcuts requiring useEditor() access. */
function DesktopShortcutsGuard(props: {
  onToggleRightDock: () => void;
}) {
  const { workspace, activeDocumentId, scheduler, layerTransformSession, setLayerTransformSession } = useEditor();
  const dialog = useDialog();

  useDesktopShortcuts({
    onToggleRightDock: props.onToggleRightDock,
    onCloseDocument: async () => {
      const id = activeDocumentId();
      if (!id) return;

      // Cancel any active transform session first (consistent with DocumentTabsBar.handleCloseTab)
      if (layerTransformSession()) {
        const engine = workspace.getActiveEngine();
        if (cancelLayerTransformSession(layerTransformSession(), engine)) {
          setLayerTransformSession(null);
          scheduler.requestRender();
        }
      }

      const session = workspace.getSession(id);
      if (session?.dirty) {
        const confirmed = await dialog.confirm({
          title: "Unsaved Changes",
          message: `"${session.displayName}" has unsaved changes. Discard them?`,
          confirmLabel: "Discard",
          cancelLabel: "Cancel",
          tone: "danger",
        });
        if (!confirmed) return;
      }

      workspace.removeDocument(id);
      scheduler.requestRender();
    },
  });

  return null;
}
