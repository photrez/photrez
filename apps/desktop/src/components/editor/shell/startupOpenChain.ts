import type { AutosaveEntry } from "../autoSave";

export interface StartupOpenChainDeps {
  getPendingOpenPath: () => Promise<{ path: string | null }>;
  openSingleFile: (path: string) => Promise<void>;
  listAutosaves: () => Promise<AutosaveEntry[]>;
  askRecover: (count: number) => Promise<boolean>;
  recoverAutosave: (entry: AutosaveEntry) => Promise<void>;
  clearAutosaves: () => Promise<void>;
  onError: (message: string) => void;
  onRecovered: (count: number) => void;
  onRecoverFailed: (entry: AutosaveEntry, message: string) => void;
}

/**
 * Serialized startup open chain: CLI file first, crash recovery second.
 * Previously these ran as independent async chains, so a slow `listAutosaves`
 * could pop the recovery dialog while (or after) the CLI document opened — a
 * confusing race (review #35). Each step is isolated so one failure never
 * blocks the next step.
 */
export async function runStartupOpenChain(deps: StartupOpenChainDeps): Promise<void> {
  // 1. CLI file — the user explicitly launched Photrez with this file.
  try {
    const res = await deps.getPendingOpenPath();
    if (res.path) {
      await deps.openSingleFile(res.path);
    }
  } catch (e) {
    deps.onError(
      `Failed to open file from command line: ${e instanceof Error ? e.message : String(e)}`,
    );
  }

  // 2. Crash recovery — only after the CLI open has settled.
  try {
    const entries = await deps.listAutosaves();
    if (entries.length === 0) return;
    const recover = await deps.askRecover(entries.length);
    if (recover) {
      for (const e of entries) {
        try {
          await deps.recoverAutosave(e);
        } catch (err) {
          deps.onRecoverFailed(e, err instanceof Error ? err.message : String(err));
        }
      }
      deps.onRecovered(entries.length);
    }
    await deps.clearAutosaves();
  } catch {
    // best-effort recovery path — never block startup
  }
}
