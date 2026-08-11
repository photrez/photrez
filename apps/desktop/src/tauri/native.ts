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

/** Extract a human-readable message from any invoke rejection.
 *  Rust commands return `Err(Value)` via err_response/error_value on failure,
 *  which makes the JS promise reject with an OBJECT envelope ({ok,error:{...}},
 *  NOT with an Error instance and NOT with {ok:false} — so `asError()` above never runs
 *  and the raw object leaks to callers. Normalize to a readable Error and export.
 */
export function ipcErrorMessage(err: unknown): string {
  if (err instanceof Error) return err.message;
  if (typeof err === "string") return err;
  const e = err as { error?: { message?: string }; message?: string; code?: string };
  return e.error?.message ?? (e.message || "Unknown IPC error");
}

/**
 * invoke() wrapper: normalize BOTH resolution (ok:false envelope) and rejection
 * (Tauri Err(Value)) into a rejected Error with a readable message, so callers
 * never see `[object Object]` and every failure has a code: message.
 */
async function invokeApi<T = unknown>(
  cmd: string,
  args?: unknown,
  options?: unknown,
): Promise<ApiSuccess<T>> {
  let result: ApiResponse<T>;
  try {
    // Only pass `options` when present — invoke() reports call shape in logs/tests.
    result = (options === undefined
      ? await invoke(cmd, args as never)
      : await invoke(cmd, args as never, options as never)) as ApiResponse<T>;
  } catch (err) {
    const e = err as { error?: { code?: string; message?: string }; message?: string; code?: string };
    const message =
      e?.error?.message ??
      e?.message ??
      (typeof err === "string" ? err : `Unknown IPC error for ${cmd}`);
    const code = e?.error?.code ?? e?.code;
    throw new Error(code ? `${code}: ${message}` : message);
  }
  if (!result.ok) throw asError(result);
  return result;
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
  await invokeApi("set_trusted_paths", { paths });
}

export async function saveProject(
  path: string,
  documentJson: string,
  layers: Record<string, string>
): Promise<void> {
  await invokeApi("save_project", { path, documentJson, layers });
}

/** Binary variant of `saveProject` — avoids a large base64 IPC round-trip. */
export async function saveProjectBinary(
  path: string,
  documentJson: string,
  layers: Record<string, Uint8Array>
): Promise<void> {
  await invokeApi("save_project_binary", { path, documentJson, layers });
}

// ─── Streaming Project Save ───

/** Begin a streaming project save — creates temp file, writes document.json, returns handle_id. */
export async function saveProjectStreamingBegin(
  path: string,
  documentJson: string,
): Promise<string> {
  const result = await invokeApi<{ handle_id: string }>("save_project_streaming_begin", { path, documentJson });
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
  await invokeApi("save_project_streaming_write_layer", pngBytes, {
    headers: {
      "handle-id": handleId,
      "layer-id": layerId,
    },
  });
}

/** Finalize the streaming save — close ZIP, fsync, atomic rename. */
export async function saveProjectStreamingEnd(handleId: string): Promise<void> {
  await invokeApi("save_project_streaming_end", { handleId });
}

/** Cancel an in-progress streaming save — drop zip, delete temp file. */
export async function saveProjectStreamingCancel(handleId: string): Promise<void> {
  await invokeApi("save_project_streaming_cancel", { handleId });
}

export async function loadProject(path: string): Promise<{ document_json: string; layers: Record<string, string> }> {
  const result = await invokeApi<{ document_json: string; layers: Record<string, string> }>("load_project", { path });
  return result.data;
}

// ─── File I/O ───
export async function readFileBytes(path: string): Promise<Uint8Array> {
  const result = await invokeApi<{ data: string }>("read_file_bytes", { path });
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

  await invokeApi("write_file_bytes", { path, data: b64 });
}

// ─── File Deletion (temp file cleanup) ───
export async function deleteFile(path: string): Promise<void> {
  await invokeApi("delete_file", { path });
}

/** Delete an autosave file from `appCacheDir()/photrez/autosave/`. Uses a dedicated
 * command scoped to the autosave directory — unlike `deleteFile` (temp-only). */
export async function deleteAutosaveFile(path: string): Promise<void> {
  await invokeApi("delete_autosave_file", { path });
}

// ─── System Fonts ───
export interface SystemFontFamily {
  family: string;
  styles: string[];
}

/** Native system font enumeration — no browser permission prompt. */
export async function listSystemFonts(): Promise<SystemFontFamily[]> {
  const result = await invokeApi<{ fonts: SystemFontFamily[] }>("list_system_fonts");
  return result.data.fonts;
}

// ─── Ping ───
export async function ping(): Promise<boolean> {
  try {
    const result = await invokeApi("ping");
    return result.ok;
  } catch {
    return false;
  }
}
