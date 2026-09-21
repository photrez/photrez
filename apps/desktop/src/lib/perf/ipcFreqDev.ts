// DEV-only Tauri invoke-frequency recorder for the live frequency probe.
// Every entry is gated on `import.meta.env.DEV` in a browser window and never
// throws, so production behavior is unchanged. Read by
// scripts/ipc-frequency-live.mjs through `window.__ipcFreqNative`.

export interface IpcFreqEventNative {
  cmd: string;
  bytes: number;
  t: number;
}

const IPC_FREQ_NATIVE_CAP = 50000;

type IpcFreqWindow = { __ipcFreqNative?: IpcFreqEventNative[] };

export function recordIpcFreq(cmd: string, args: Record<string, unknown>): void {
  if (!import.meta.env.DEV) return;
  if (typeof window === "undefined") return;
  try {
    const holder = window as unknown as IpcFreqWindow;
    if (!Array.isArray(holder.__ipcFreqNative)) holder.__ipcFreqNative = [];
    const events = holder.__ipcFreqNative as IpcFreqEventNative[];
    let bytes = 0;
    try {
      bytes = (JSON.stringify(args) ?? "").length;
    } catch {
      bytes = -1;
    }
    if (events.length >= IPC_FREQ_NATIVE_CAP) events.shift();
    events.push({ cmd, bytes, t: performance.now() });
  } catch {
    // Recording must never disturb the production invoke path.
  }
}

export function resetIpcFreqNative(): void {
  if (!import.meta.env.DEV) return;
  if (typeof window === "undefined") return;
  try {
    const holder = window as unknown as IpcFreqWindow;
    if (Array.isArray(holder.__ipcFreqNative)) holder.__ipcFreqNative.length = 0;
  } catch {
    // Recording must never disturb the production invoke path.
  }
}
