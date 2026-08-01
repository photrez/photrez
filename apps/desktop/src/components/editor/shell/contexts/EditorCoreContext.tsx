import { createContext, useContext, type JSX } from "solid-js";
import type { Accessor, Setter } from "solid-js";
import type { WorkspaceManager } from "@/engine/workspace";
import type { WebGL2Backend } from "@/renderer/webgl2";
import type { RenderScheduler } from "@/renderer/scheduler";
import type { ViewportCamera } from "../../../../viewport/viewportCamera";

/**
 * Runtime infrastructure: engine services + camera/viewport ops + app actions.
 * Domain context of EditorProvider (see EditorContext.tsx for composition).
 */
export interface EditorCoreValue {
  workspace: WorkspaceManager;
  renderer: WebGL2Backend;
  scheduler: RenderScheduler;
  camera: ViewportCamera;
  setViewportState: (next: { x: number; y: number; zoom: number }) => void;
  syncFromCamera: () => void;
  syncViewport: () => void;
  openImage: () => Promise<void>;
  useGPUCameraForModernCrop: Accessor<boolean>;
  setUseGPUCameraForModernCrop: Setter<boolean>;
  showToast: (message: string, severity?: "info" | "warn" | "error") => void;
}

export const EditorCoreContext = createContext<EditorCoreValue>();

export function EditorCoreProvider(props: { value: EditorCoreValue; children: JSX.Element }) {
  return (
    <EditorCoreContext.Provider value={props.value}>
      {props.children}
    </EditorCoreContext.Provider>
  );
}

export function useEditorCore(): EditorCoreValue {
  const ctx = useContext(EditorCoreContext);
  if (!ctx) {
    throw new Error("useEditorCore must be used within an EditorProvider");
  }
  return ctx;
}
