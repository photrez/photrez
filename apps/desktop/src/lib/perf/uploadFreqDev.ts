// DEV-only GPU texture-upload recorder for the live re-upload probe.
// Every entry is gated on `import.meta.env.DEV` in a browser window and never
// throws, so production behavior is unchanged. Read by
// scripts/ipc-frequency-live.mjs through `window.__uploadFreq`.
// uploadSurfaceTiles (paint-commit tile path) is out of scope here:
// per-undo numbers come from full/patch uploads only.

export type UploadFreqKind = "full" | "patch";

export interface UploadFreqEvent {
  layerId: string;
  bytes: number;
  kind: UploadFreqKind;
  t: number;
}

const UPLOAD_FREQ_CAP = 50000;

type UploadFreqWindow = { __uploadFreq?: UploadFreqEvent[]; __uploadFreqDropped?: number };

export function recordUploadFreq(layerId: string, bytes: number, kind: UploadFreqKind): void {
  if (!import.meta.env.DEV) return;
  if (typeof window === "undefined") return;
  try {
    const holder = window as unknown as UploadFreqWindow;
    if (!Array.isArray(holder.__uploadFreq)) holder.__uploadFreq = [];
    if (typeof holder.__uploadFreqDropped !== "number") holder.__uploadFreqDropped = 0;
    const events = holder.__uploadFreq as UploadFreqEvent[];
    if (events.length >= UPLOAD_FREQ_CAP) { events.shift(); holder.__uploadFreqDropped += 1; }
    events.push({ layerId, bytes, kind, t: performance.now() });
  } catch {
    // Recording must never disturb the production upload path.
  }
}

export function resetUploadFreq(): void {
  if (!import.meta.env.DEV) return;
  if (typeof window === "undefined") return;
  try {
    const holder = window as unknown as UploadFreqWindow;
    if (Array.isArray(holder.__uploadFreq)) holder.__uploadFreq.length = 0;
    holder.__uploadFreqDropped = 0;
  } catch {
    // Recording must never disturb the production upload path.
  }
}
