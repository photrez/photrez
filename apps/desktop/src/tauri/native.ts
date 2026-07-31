import { invoke } from "@tauri-apps/api/core";
import { open, save } from "@tauri-apps/plugin-dialog";

interface ApiSuccess<T> {
  ok: true;
  contract_version: string;
  data: T;
}

interface ApiError {
  ok: false;
  contract_version: string;
  error: { code: string; message: string; details: unknown };
}

type ApiResponse<T = unknown> = ApiSuccess<T> | ApiError;

function asError(result: ApiError): Error {
  return new Error(`${result.error.code}: ${result.error.message}`);
}

// ─── File Dialog ───
export async function showOpenImageDialog(): Promise<string[] | null> {
  const selected = await open({
    multiple: true,
    filters: [
      {
        name: "Supported Formats",
        extensions: ["ptz", "png", "jpg", "jpeg", "webp", "bmp"]
      },
      {
        name: "Photrez Project (*.ptz)",
        extensions: ["ptz"]
      },
      {
        name: "Images",
        extensions: ["png", "jpg", "jpeg", "webp", "bmp"]
      }
    ]
  });

  if (!selected) return null;
  const paths = Array.isArray(selected) ? selected : [selected];
  // Rust only allows file-IO on user-approved paths — approve dialog results.
  await setTrustedPaths(paths);
  return paths;
}

export async function showSaveDialog(defaultName: string): Promise<string | null> {
  const ext = defaultName.split(".").pop() || "png";
  const path = await save({
    defaultPath: defaultName,
    filters: [{ name: ext.toUpperCase(), extensions: [ext] }]
  });
  if (!path) return null;
  await setTrustedPaths([path]);
  return path;
}

export async function showSaveDialogAllFormats(defaultName: string): Promise<string | null> {
  const path = await save({
    defaultPath: defaultName,
    filters: [
      { name: "All Supported Formats", extensions: ["ptz", "png", "jpg", "jpeg", "webp"] },
      { name: "Photrez Project (*.ptz)", extensions: ["ptz"] },
      { name: "PNG Image (*.png)", extensions: ["png"] },
      { name: "JPEG Image (*.jpg)", extensions: ["jpg", "jpeg"] },
      { name: "WebP Image (*.webp)", extensions: ["webp"] }
    ]
  });
  if (!path) return null;
  await setTrustedPaths([path]);
  return path;
}

/** Approve file paths (from OS dialogs) for Rust-side file-IO commands. */
export async function setTrustedPaths(paths: string[]): Promise<void> {
  if (paths.length === 0) return;
  const result = await invoke("set_trusted_paths", { paths }) as ApiResponse;
  if (!result.ok) throw asError(result);
}

export async function saveProject(
  path: string,
  documentJson: string,
  layers: Record<string, string>
): Promise<void> {
  const result = await invoke("save_project", { path, documentJson, layers }) as ApiResponse;
  if (!result.ok) throw asError(result);
}

/** Binary variant of `saveProject` — avoids a large base64 IPC round-trip. */
export async function saveProjectBinary(
  path: string,
  documentJson: string,
  layers: Record<string, Uint8Array>
): Promise<void> {
  const result = await invoke("save_project_binary", { path, documentJson, layers }) as ApiResponse;
  if (!result.ok) throw asError(result);
}

// ─── Streaming Project Save ───

/** Begin a streaming project save — creates temp file, writes document.json, returns handle_id. */
export async function saveProjectStreamingBegin(
  path: string,
  documentJson: string,
): Promise<string> {
  const result = await invoke("save_project_streaming_begin", { path, documentJson }) as ApiResponse<{ handle_id: string }>;
  if (!result.ok) throw asError(result);
  return result.data.handle_id;
}

/**
 * Write one layer's PNG bytes to the in-progress ZIP file.
 * Uses raw IPC (Uint8Array body + headers) — zero base64 overhead.
 * Matches the print_image_raw pattern.
 */
export async function saveProjectStreamingWriteLayer(
  handleId: string,
  layerId: string,
  pngBytes: Uint8Array,
): Promise<void> {
  const result = await invoke("save_project_streaming_write_layer", pngBytes, {
    headers: {
      "handle-id": handleId,
      "layer-id": layerId,
    },
  }) as ApiResponse;
  if (!result.ok) throw asError(result);
}

/** Finalize the streaming save — close ZIP, fsync, atomic rename. */
export async function saveProjectStreamingEnd(handleId: string): Promise<void> {
  const result = await invoke("save_project_streaming_end", { handleId }) as ApiResponse;
  if (!result.ok) throw asError(result);
}

/** Cancel an in-progress streaming save — drop zip, delete temp file. */
export async function saveProjectStreamingCancel(handleId: string): Promise<void> {
  const result = await invoke("save_project_streaming_cancel", { handleId }) as ApiResponse;
  if (!result.ok) throw asError(result);
}

export async function loadProject(path: string): Promise<{ document_json: string; layers: Record<string, string> }> {
  const result = await invoke("load_project", { path }) as ApiResponse<{ document_json: string; layers: Record<string, string> }>;
  if (!result.ok) throw asError(result);
  return result.data;
}

// ─── File I/O ───
export async function readFileBytes(path: string): Promise<Uint8Array> {
  const result = await invoke("read_file_bytes", { path }) as ApiResponse<{ data: string }>;
  if (!result.ok) throw asError(result);

  const binaryString = atob(result.data.data);
  const bytes = new Uint8Array(binaryString.length);
  for (let i = 0; i < binaryString.length; i++) {
    bytes[i] = binaryString.charCodeAt(i);
  }
  return bytes;
}

export async function writeFileBytes(path: string, data: Uint8Array): Promise<void> {
  let binary = "";
  for (let i = 0; i < data.length; i++) {
    binary += String.fromCharCode(data[i]);
  }
  const b64 = btoa(binary);

  const result = await invoke("write_file_bytes", { path, data: b64 }) as ApiResponse;
  if (!result.ok) throw asError(result);
}

// ─── File Deletion (temp file cleanup) ───
export async function deleteFile(path: string): Promise<void> {
  const result = await invoke("delete_file", { path }) as ApiResponse;
  if (!result.ok) throw asError(result);
}

/** Delete an autosave file from `cacheDir()/photrez/autosave/`. Uses a dedicated
 * command scoped to the autosave directory — unlike `deleteFile` (temp-only). */
export async function deleteAutosaveFile(path: string): Promise<void> {
  const result = await invoke("delete_autosave_file", { path }) as ApiResponse;
  if (!result.ok) throw asError(result);
}

// ─── Ping ───
export async function ping(): Promise<boolean> {
  try {
    const result = await invoke("ping") as ApiResponse;
    return result.ok;
  } catch {
    return false;
  }
}
