import { cacheDir } from "@tauri-apps/api/path";
import { serializeAndSaveProject } from "./projectSerialize";
import { writeFileBytes, readFileBytes, deleteFile } from "@/tauri/native";
import type { WorkspaceManager } from "@/engine/workspace";
import type { WebGL2Backend } from "@/renderer/webgl2";
import type { RenderScheduler } from "@/renderer/scheduler";

interface OpenImageParams {
  workspace: WorkspaceManager;
  renderer: WebGL2Backend;
  scheduler: RenderScheduler;
  onError?: (message: string) => void;
  onLoading?: (message: string | null) => void;
}

const AUTOSAVE_SUBDIR = "photrez/autosave";
const MANIFEST = "manifest.json";

export interface AutosaveEntry {
  docId: string;
  displayName: string;
  path: string;
}

async function autosaveDir(): Promise<string> {
  const base = await cacheDir();
  const sep = base.endsWith("/") || base.endsWith("\\") ? "" : "/";
  return `${base}${sep}${AUTOSAVE_SUBDIR}`;
}

export async function autosavePathFor(docId: string): Promise<string> {
  const dir = await autosaveDir();
  return `${dir}/${docId}.ptz`;
}

const encoder = new TextEncoder();
const decoder = new TextDecoder("utf-8");

function strToBytes(s: string): Uint8Array {
  return encoder.encode(s);
}

function bytesToStr(bytes: Uint8Array): string {
  return decoder.decode(bytes);
}

/**
 * Persist every dirty document to the cache directory so an abrupt crash can
 * be recovered. Called on a debounced timer — not on every edit.
 */
export async function autosaveDirtyDocs(
  workspace: WorkspaceManager,
  onError?: (message: string) => void,
): Promise<void> {
  try {
    const dir = await autosaveDir();
    const manifest: Record<string, string> = {};
    const sessions = workspace.getSessions();
    for (const session of sessions) {
      const engine = session.engine;
      if (!engine || !engine.isDirty()) continue;
      const docId = engine.getId();
      const path = await autosavePathFor(docId);
      await serializeAndSaveProject(engine, path);
      manifest[docId] = session.displayName ?? docId;
    }
    await writeFileBytes(`${dir}/${MANIFEST}`, strToBytes(JSON.stringify(manifest)));
  } catch (e) {
    onError?.(`Auto-save failed: ${e instanceof Error ? e.message : String(e)}`);
  }
}

/** List recoverable autosaved sessions from a previous run. */
export async function listAutosaves(): Promise<AutosaveEntry[]> {
  try {
    const dir = await autosaveDir();
    const manifestPath = `${dir}/${MANIFEST}`;
    let raw: string;
    try {
      raw = bytesToStr(await readFileBytes(manifestPath));
    } catch {
      return [];
    }
    const manifest = JSON.parse(raw) as Record<string, string>;
    return Object.entries(manifest).map(([docId, displayName]) => ({
      docId,
      displayName,
      path: `${dir}/${docId}.ptz`,
    }));
  } catch {
    return [];
  }
}

export async function clearAutosave(docId: string): Promise<void> {
  try {
    const path = await autosavePathFor(docId);
    await deleteFile(path);
  } catch {
    /* best-effort */
  }
}

export async function clearAllAutosaves(): Promise<void> {
  try {
    const entries = await listAutosaves();
    for (const e of entries) {
      try { await deleteFile(e.path); } catch { /* ignore */ }
    }
    const dir = await autosaveDir();
    try { await deleteFile(`${dir}/${MANIFEST}`); } catch { /* ignore */ }
  } catch {
    /* best-effort */
  }
}

export type { OpenImageParams };
