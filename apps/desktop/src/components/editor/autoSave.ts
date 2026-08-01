import { createSignal } from "solid-js";
import { appCacheDir } from "@tauri-apps/api/path";
import { serializeAndSaveProject } from "./projectSerialize";
import { writeFileBytes, readFileBytes, deleteAutosaveFile } from "@/tauri/native";
import type { WorkspaceManager } from "@/engine/workspace";
import type { WebGL2Backend } from "@/renderer/webgl2";
import type { RenderScheduler } from "@/renderer/scheduler";

/** Autosave status indicator — module-level signal so both EditorContext and BottomStatusBar can read it. */
export type AutosaveStatus = "idle" | "saving" | "error" | "saved";
export const [autosaveStatus, setAutosaveStatus] = createSignal<AutosaveStatus>("idle");
export const [autosaveError, setAutosaveError] = createSignal<string | null>(null);
export const [autosaveTimestamp, setAutosaveTimestamp] = createSignal<number>(0);

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
  // Must match the Rust side's `app_cache_dir()` (cacheDir() alone resolves to
  // %LOCALAPPDATA% without the app identifier, which TrustedPathsState does
  // not auto-approve — autosave would fail every write with E_VALIDATION).
  const base = await appCacheDir();
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
 *
 * `signal` lets a manual save abort an in-flight autosave: the PNG encode
 * (and any pending write) is cancelled promptly and the worker pool stays
 * warm (reset, not terminated), so the manual save runs without waiting.
 * An aborted autosave is not an error — it is silently skipped.
 */
export async function autosaveDirtyDocs(
  workspace: WorkspaceManager,
  onError?: (message: string) => void,
  signal?: AbortSignal,
): Promise<void> {
  if (signal?.aborted) return;
  setAutosaveStatus("saving");
  setAutosaveError(null);
  try {
    const dir = await autosaveDir();
    const now = Date.now();
    const manifest: Record<string, string> = {};

    // Read existing manifest to preserve timestamps for unchanged docs.
    try {
      const existingRaw = bytesToStr(await readFileBytes(`${dir}/${MANIFEST}`));
      const existing = JSON.parse(existingRaw) as Record<string, string>;
      for (const [docId, value] of Object.entries(existing)) {
        manifest[docId] = value;
      }
    } catch {
      // first run — no existing manifest
    }

    const sessions = workspace.getSessions();
    for (const session of sessions) {
      const engine = session.engine;
      if (!engine || !engine.isDirty()) continue;
      const docId = engine.getId();
      const path = await autosavePathFor(docId);
      await serializeAndSaveProject(engine, path, { signal });
      // Store displayName + timestamp so stale cleanup knows when the file was saved.
      manifest[docId] = `${session.displayName ?? docId}|${now}`;
    }
    await writeFileBytes(`${dir}/${MANIFEST}`, strToBytes(JSON.stringify(manifest)));

    // Opportunistic stale cleanup (best-effort).
    void clearStaleAutosaves();

    setAutosaveStatus("saved");
    setAutosaveTimestamp(now);
  } catch (e: unknown) {
    if (signal?.aborted) {
      // Cancelled by a manual save — not a failure.
      setAutosaveStatus("idle");
      return;
    }
    const msg = extractErrorMessage(e);
    console.error("[autosaveDirtyDocs]", msg, e);
    setAutosaveStatus("error");
    setAutosaveError(msg);
    onError?.(`Auto-save failed: ${msg}`);
  }
}

/** Abort controller for the in-flight autosave cycle, if any. */
let autosaveAbort: AbortController | null = null;

/**
 * Abort an in-flight autosave. Called by the manual save path (Ctrl+S /
 * File → Save / Save As) so the user's save never waits for — or collides
 * with — a background autosave. Safe to call when no autosave is running.
 */
export function cancelAutosave(): void {
  if (autosaveAbort) {
    autosaveAbort.abort();
    autosaveAbort = null;
  }
}

/** Debounced wrapper for autosaveDirtyDocs used by EditorContext timer. */
export function createAutosaveTimerDebouncer(
  workspace: WorkspaceManager,
  onError?: (message: string) => void,
): () => void {
  let pending = false;
  return () => {
    if (pending) return; // skip if previous cycle still in-flight
    pending = true;
    autosaveAbort = new AbortController();
    const signal = autosaveAbort.signal;
    setAutosaveStatus("saving");
    autosaveDirtyDocs(workspace, onError, signal).finally(() => {
      if (autosaveAbort?.signal === signal) autosaveAbort = null;
      pending = false;
    });
  };
}

/** Extract a human-readable message from a thrown value, including Tauri IPC rejection objects. */
function extractErrorMessage(e: unknown): string {
  if (e instanceof Error) return e.message;
  if (e && typeof e === "object") {
    // Tauri v2 `invoke` rejects with a plain object that may have a `.message` property.
    const maybe = (e as Record<string, unknown>).message;
    if (typeof maybe === "string") return maybe;
    try {
      return JSON.stringify(e);
    } catch {
      return String(e);
    }
  }
  return String(e);
}

const STALE_AGE_MS = 7 * 24 * 60 * 60 * 1000; // 7 days

/**
 * Remove autosave files and manifest entries older than STALE_AGE_MS.
 * Timestamps are stored in the manifest value as `displayName|timestamp`.
 * Called opportunistically during autosave; failures are silently ignored.
 */
async function clearStaleAutosaves(): Promise<void> {
  try {
    const dir = await autosaveDir();
    const manifestPath = `${dir}/${MANIFEST}`;
    let raw: string;
    try {
      raw = bytesToStr(await readFileBytes(manifestPath));
    } catch {
      return; // no manifest yet
    }
    const manifest = JSON.parse(raw) as Record<string, string>;
    const now = Date.now();
    const fresh: Record<string, string> = {};
    for (const [docId, value] of Object.entries(manifest)) {
      // Parse stored timestamp: value format = `displayName|timestamp` or just `displayName` (legacy)
      const pipeIdx = value.lastIndexOf("|");
      const savedAt = pipeIdx > 0 ? Number(value.slice(pipeIdx + 1)) : NaN;
      const displayName = pipeIdx > 0 ? value.slice(0, pipeIdx) : value;
      if (!isNaN(savedAt) && (now - savedAt > STALE_AGE_MS)) {
        const path = `${dir}/${docId}.ptz`;
        try { await deleteAutosaveFile(path); } catch { /* ignore */ }
        continue; // drop from manifest
      }
      fresh[docId] = value; // keep as-is
    }
    // Rewrite manifest with only fresh entries
    await writeFileBytes(manifestPath, strToBytes(JSON.stringify(fresh)));
  } catch {
    // best-effort
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
    return Object.entries(manifest).map(([docId, value]) => {
      // value format = `displayName|timestamp` or just `displayName` (legacy)
      const pipeIdx = value.lastIndexOf("|");
      const displayName = pipeIdx > 0 ? value.slice(0, pipeIdx) : value;
      return { docId, displayName, path: `${dir}/${docId}.ptz` };
    });
  } catch {
    return [];
  }
}

export async function clearAutosave(docId: string): Promise<void> {
  try {
    const path = await autosavePathFor(docId);
    await deleteAutosaveFile(path);
  } catch {
    /* best-effort */
  }
}

export async function clearAllAutosaves(): Promise<void> {
  try {
    const entries = await listAutosaves();
    for (const e of entries) {
      try { await deleteAutosaveFile(e.path); } catch { /* ignore */ }
    }
    const dir = await autosaveDir();
    try { await deleteAutosaveFile(`${dir}/${MANIFEST}`); } catch { /* ignore */ }
  } catch {
    /* best-effort */
  }
}

export type { OpenImageParams };
