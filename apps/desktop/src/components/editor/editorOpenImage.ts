import { showOpenImageDialog, readFileBytes, loadProject } from "@/tauri/native";
import { WorkspaceManager, DocumentSession } from "@/engine/workspace";
import { DocumentEngine } from "@/engine/document";
import { CommandHistory } from "@/engine/history";
import { DocumentModel, MAX_OPEN_DOCUMENTS } from "@/engine/types";
import type { WebGL2Backend } from "@/renderer/webgl2";
import type { RenderScheduler } from "@/renderer/scheduler";
import { isTauriRuntime } from "@/lib/desktop/tauriWindow";
import { addRecentFile } from "@/lib/recentFiles";
import { showToast } from "./Toast";
import { tick } from "@/lib/dom";

export interface OpenImageParams {
  workspace: WorkspaceManager;
  renderer: WebGL2Backend;
  scheduler: RenderScheduler;
  // Callback for UI error reporting. EditorContext passes showToast here.
  onError?: (message: string) => void;
  // Callback for loading state. Called with a message string when loading starts,
  // and with null when loading finishes.
  onLoading?: (message: string | null) => void;
}

export async function openImage(params: OpenImageParams) {
  const reportError = (message: string) => {
    params.onError?.(message);
  };

  // 1. Web browser fallback check
  if (!isTauriRuntime()) {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = "image/*";
    input.multiple = true;
      input.onchange = async (e: Event) => {
        const files = (e.target as HTMLInputElement).files;
        if (!files) return;
        await openImageFilesAsDocuments(Array.from(files), params);
      };
    input.click();
    return;
  }

  // 2. Tauri native environment
  try {
    const paths = await showOpenImageDialog();
    if (!paths || paths.length === 0) return;

    const total = paths.length;
    params.onLoading?.(`Opening ${total} file${total > 1 ? "s" : ""}...`);

    try {
      // Separate .ptz project files from regular images — .ptz needs
      // sequential DocumentEngine restoration (loadProjectFile), while
      // images can be decoded in parallel (decodeSessionFromFile).
      const ptzPaths = paths.filter((p) => p.toLowerCase().endsWith(".ptz"));
      const imagePaths = paths.filter((p) => !p.toLowerCase().endsWith(".ptz"));

      let loadedAny = false;

      // Decode regular images in parallel
      const sessions = await Promise.all(
        imagePaths.map((p) => decodeSessionFromFile(p, params)),
      );
      for (const s of sessions) {
        if (!s) continue;
        params.workspace.addDocument(s.session);
        params.renderer.uploadImage(s.bgLayerId, s.bitmap);
        addRecentFile(s.path, s.fileName);
        loadedAny = true;
      }

      // Open .ptz projects sequentially (each involves engine restoration
      // and per-layer bitmap decoding — not parallelizable)
      for (const p of ptzPaths) {
        const fileName = p.split(/[/\\]/).pop() || "Project";
        params.onLoading?.(`Loading project ${fileName}...`);
        await loadProjectFile(p, params, fileName);
        loadedAny = true;
      }

      if (loadedAny) {
        params.scheduler.requestRender();
        showToast("File(s) loaded", "info");
      }
    } finally {
      params.onLoading?.(null);
    }
  } catch (e) {
    reportError(`Failed to open image: ${e}`);
  }
}

/** Open one or more dropped/selected image Files as brand-new documents. */
export async function openImageFilesAsDocuments(
  files: File[],
  params: OpenImageParams,
): Promise<void> {
  for (const file of files) {
    if (params.workspace.isFull()) {
      showToast(`Workspace full: close a document first (max ${MAX_OPEN_DOCUMENTS})`, "error");
      break;
    }
    try {
      const bitmap = await createImageBitmap(file);
      const id = `doc-${crypto.randomUUID()}`;
      const session = WorkspaceManager.createDocumentFromImage(id, file.name, bitmap);
      params.workspace.addDocument(session);
      const bgLayerId = session.engine.getLayers().find((l) => l.isBackground)?.id
        ?? session.engine.getLayers()[0].id;
      params.renderer.uploadImage(bgLayerId, bitmap);
      params.scheduler.requestRender();
    } catch (err) {
      params.onError?.(`Failed to load ${file.name}: ${err}`);
    }
  }
}

function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

interface DecodedSession {
  session: DocumentSession;
  bgLayerId: string;
  bitmap: ImageBitmap;
  path: string;
  fileName: string;
}

/**
 * Decode an image file and build its DocumentSession WITHOUT adding it to the
 * workspace. Kept side-effect-free so batch imports can decode many files in
 * parallel (Promise.all) and commit them in a single batch.
 * Returns null when the workspace is full or the path is a .ptz (handled
 * separately by loadProjectFile).
 */
async function decodeSessionFromFile(
  path: string,
  params: OpenImageParams,
): Promise<DecodedSession | null> {
  if (params.workspace.isFull()) return null;
  if (path.toLowerCase().endsWith(".ptz")) return null; // projects use loadProjectFile

  const fileName = path.split(/[/\\]/).pop() || "Image";
  const bytes = await readFileBytes(path);
  const buffer = bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength) as ArrayBuffer;
  const blob = new Blob([buffer]);
  const bitmap = await createImageBitmap(blob);

  const id = `doc-${crypto.randomUUID()}`;
  const session = WorkspaceManager.createDocumentFromImage(id, fileName, bitmap);
  session.sourcePath = path;

  const bgLayerId = session.engine.getLayers()[0].id;
  return { session, bgLayerId, bitmap, path, fileName };
}

/** Open a single image or .ptz project file by path. */
export async function openSingleFile(path: string, params: OpenImageParams): Promise<void> {
  if (params.workspace.isFull()) return;

  const fileName = path.split(/[/\\]/).pop() || "Image";

  if (path.toLowerCase().endsWith(".ptz")) {
    params.onLoading?.(`Loading project ${fileName}...`);
    await loadProjectFile(path, params, fileName);
    return;
  }

  const decoded = await decodeSessionFromFile(path, params);
  if (!decoded) return;

  params.workspace.addDocument(decoded.session);
  params.renderer.uploadImage(decoded.bgLayerId, decoded.bitmap);
  params.scheduler.requestRender();

  addRecentFile(decoded.path, decoded.fileName);
  await tick();
}

export async function loadProjectFile(path: string, params: OpenImageParams, fileName: string) {
  const result = await loadProject(path);
  const model = JSON.parse(result.document_json) as DocumentModel;

  // .ptz versioning (KNOWN_ISSUES #9): alpha.1 files have no `version` field.
  // Absent / 0 / 1 are compatible with this loader. Newer files warn but still load.
  const fileVersion = (model as DocumentModel & { version?: number }).version;
  if (typeof fileVersion === "number" && fileVersion > 1) {
    showToast(`This project was saved by a newer Photrez version (v${fileVersion}). It may not load correctly.`, "error");
  }

  params.onLoading?.(`Loading project layers...`);
  for (const layer of model.layers) {
    const base64Data = result.layers[layer.id];
    if (base64Data) {
      const binaryString = atob(base64Data);
      const bytes = new Uint8Array(binaryString.length);
      for (let i = 0; i < binaryString.length; i++) {
        bytes[i] = binaryString.charCodeAt(i);
      }
      const blob = new Blob([bytes], { type: "image/png" });
      layer.imageBitmap = await createImageBitmap(blob);
      await tick(); // yield so UI stays responsive during project load
    } else {
      layer.imageBitmap = null;
    }
  }

  const engine = new DocumentEngine(model.id, model.name, model.width, model.height);
  engine.restore(model, { restoreViewport: true });
  engine.clearDirty();

  const session: DocumentSession = {
    engine,
    history: new CommandHistory(),
    displayName: fileName,
    sourcePath: path,
    dirty: false,
  };

  params.workspace.addDocument(session);

  for (const layer of engine.getLayers()) {
    if (layer.imageBitmap) {
      params.renderer.uploadImage(layer.id, layer.imageBitmap);
    }
  }

  addRecentFile(path, fileName);
  params.scheduler.requestRender();
}
