// Ticket 1-2 — Typed bridge: CommandEnvelope -> CommandResult{delta} via wasm JSON transport.
// Control path only (no binary optimization yet). Correctness via Rust expectedVersion check.

import type {
  CommandEnvelope,
  CommandResult,
  RenderLayer,
  RenderSnapshot,
  HistoryQueryResult,
} from "./types";
import { CONTRACT_VERSION } from "./types";

type WasmProtocol = {
  protocol_contract_version: () => number;
  protocol_apply_command: (json: string) => string;
  protocol_snapshot_json: () => string;
  // ADR 0008 H0 exports (optional so an older pkg degrades to emulation):
  protocol_register_payload_adapter?: (id: string) => void;
  protocol_history_query_json?: () => string;
  protocol_history_cursor_commit?: (json: string) => string;
};

let wasm: WasmProtocol | null = null;

export function setProtocolWasm(mod: WasmProtocol): void {
  wasm = mod;
}

export function getContractVersion(): number {
  if (wasm) return wasm.protocol_contract_version();
  return CONTRACT_VERSION;
}

export function applyCommand(envelope: CommandEnvelope): CommandResult {
  if (envelope.contractVersion !== CONTRACT_VERSION) {
    throw new Error(
      `E_CONTRACT_VERSION: expected ${CONTRACT_VERSION} got ${envelope.contractVersion}`,
    );
  }
  const rustEnvelope = toRustEnvelope(envelope);
  const json = JSON.stringify(rustEnvelope);
  if (!wasm) {
    return emulateApply(envelope);
  }
  try {
    const outJson = wasm.protocol_apply_command(json);
    const parsed = JSON.parse(outJson) as CommandResult;
    return parsed;
  } catch (e) {
    const msg = e instanceof Error ? e.message : String(e);
    try {
      const parsed = JSON.parse(msg) as { code: string; message: string };
      throw new Error(`${parsed.code}: ${parsed.message}`);
    } catch {
      throw new Error(msg);
    }
  }
}

export function getSnapshot(): RenderSnapshot {
  if (!wasm) return { version: 0, layers: [] };
  const j = wasm.protocol_snapshot_json();
  return JSON.parse(j) as RenderSnapshot;
}

function toRustEnvelope(env: CommandEnvelope): unknown {
  const c = env.command as unknown as Record<string, unknown>;
  let rustCmd: unknown;
  if (c.type === "noop") rustCmd = { type: "noop" };
  else if (c.type === "ping") rustCmd = { type: "ping", echo: c.echo };
  else if (c.type === "addLayer") rustCmd = { type: "addLayer", name: c.name };
  else if (c.type === "deleteLayer") rustCmd = { type: "deleteLayer", id: c.id };
  else if (c.type === "transformLayer") rustCmd = { type: "transformLayer", id: c.id, transform: c.transform };
  else if (c.type === "setOpacity") rustCmd = { type: "setOpacity", id: c.id, opacity: c.opacity };
  else if (c.type === "brushStroke") rustCmd = { type: "brushStroke", layer_id: c.layerId, points: c.points, settings: c.settings };
  else if (c.type === "undo") rustCmd = { type: "undo" };
  else if (c.type === "redo") rustCmd = { type: "redo" };
  else if (c.type === "recordExternalTransition")
    rustCmd = {
      type: "recordExternalTransition",
      label: c.label,
      affected_layer_ids: c.affectedLayerIds,
      adapter_id: c.adapterId,
      token: c.token,
      memory_cost_bytes: c.memoryCostBytes,
    };
  else rustCmd = c;
  return { contractVersion: env.contractVersion, expectedVersion: env.expectedVersion, command: rustCmd };
}

// JS emulation mirrors Rust ProtocolEngine — includes expectedVersion check.
// ADR 0008 H0: entry-stream metadata parallels the Rust engine (cursor/seq/
// adapters), and undo/redo walk entries exactly like the Rust walker.
let emuVersion = 0;
let emuLayers: RenderSnapshot["layers"] = [];
let emuNextResource = 1;
let emuHistory: RenderSnapshot["layers"][] = []; // kept: pre-states for legacy parity checks
let emuFuture: RenderSnapshot["layers"][] = [];

type EmuEntry = {
  seq: number;
  groupId: number;
  origin: "native" | { external: string };
  label: string;
  affected: string[];
  vb: number;
  va: number;
  bytes: number;
  before: RenderSnapshot["layers"];
  after: RenderSnapshot["layers"];
  token?: string;
};
let emuEntries: EmuEntry[] = [];
let emuCursor = 0;
let emuNextSeq = 1;
const emuAdapters = new Set<string>();
// External-pending barrier (H0 invariant): while set, EVERY emulated command
// rejects with E_EXTERNAL_PENDING until the matching cursor commit lands.
let emuPendingExternal: { seq: number; direction: "undo" | "redo" } | null = null;

// TS-side stand-ins for the Rust `estimate_*` constants. The per-layer struct
// base in Rust is `size_of::<RenderLayer>()` (not knowable in TS), so this is a
// stable approximation that keeps the emulator on the SAME SEMANTICS as the
// engine (unique layer count + per-layer byte estimate + set buffers) rather
// than the stale JSON-serialize double-count.
//
// CONTRACT NOTE (photrez-counter residual 2): the COUNT SEMANTICS are the
// contract (each UNIQUE layer object/reference counted exactly once across
// before/after), NOT the byte VALUE. These byte figures are documented
// approximations - they are NOT byte-equal to Rust's `estimate_native_entry_cost`
// (which uses `size_of::<RenderLayer>()` for the per-layer base). A test
// (`bridgeEmuCost.test.ts`) pins the unique-count semantics so a regression that
// re-introduces a deep-copy before+after double-count is caught.
const EMU_PER_LAYER_BASE_BYTES = 128;
const EMU_PER_LAYER_SLACK_BYTES = 64;
const EMU_ARC_PTR_BYTES = 8; // size_of::<Arc<T>>() on a 64-bit target

function estimateEmuLayerBytes(l: RenderLayer): number {
  return EMU_PER_LAYER_BASE_BYTES + l.id.length + l.name.length + EMU_PER_LAYER_SLACK_BYTES;
}

// Mirrors Rust `estimate_native_entry_cost`: counts UNIQUE layer objects across
// before/after (unchanged layers are the SAME object reference, so they count
// once - the before+after double-count is gone), times a cheap per-layer byte
// estimate, plus the two set buffers. No serde / no JSON.stringify.
export function estimateEmuNativeBytes(before: RenderSnapshot["layers"], after: RenderSnapshot["layers"]): number {
  const seen = new Set<RenderLayer>();
  let layerBytes = 0;
  let uniqueCount = 0;
  for (const l of before) {
    if (!seen.has(l)) {
      seen.add(l);
      uniqueCount += 1;
      layerBytes += estimateEmuLayerBytes(l);
    }
  }
  for (const l of after) {
    if (!seen.has(l)) {
      seen.add(l);
      uniqueCount += 1;
      layerBytes += estimateEmuLayerBytes(l);
    }
  }
  const ptrSlots = uniqueCount * 2 * EMU_ARC_PTR_BYTES;
  const wrappers = 2 * EMU_ARC_PTR_BYTES;
  return layerBytes + ptrSlots + wrappers;
}

function beginEmu(label: string, affected: string[]): number {
  emuEntries = emuEntries.slice(0, emuCursor);
  const seq = emuNextSeq++;
  emuEntries.push({ seq, groupId: seq, origin: "native", label, affected, vb: emuVersion, va: emuVersion + 1, bytes: 0, before: [...emuLayers], after: [] });
  return emuEntries.length - 1;
}
function finishEmu(idx: number): void {
  const e = emuEntries[idx];
  if (e) {
    e.after = [...emuLayers];
    e.bytes = estimateEmuNativeBytes(e.before, e.after);
  }
  emuCursor = emuEntries.length;
}

export function registerPayloadAdapter(adapterId: string): void {
  if (wasm?.protocol_register_payload_adapter) wasm.protocol_register_payload_adapter(adapterId);
  else if (adapterId !== "native") emuAdapters.add(adapterId);
}

export function getHistoryQuery(): HistoryQueryResult {
  if (wasm?.protocol_history_query_json) return JSON.parse(wasm.protocol_history_query_json()) as HistoryQueryResult;
  return {
    cursor: emuCursor,
    lastSeq: emuEntries.length ? emuEntries[emuEntries.length - 1].seq : 0,
    degradedHint: false,
    pendingExternal: emuPendingExternal ? { ...emuPendingExternal } : null,
    entries: emuEntries.map((e) => ({
      seq: e.seq,
      groupId: e.groupId,
      origin: e.origin === "native" ? "native" : `external:${e.origin.external}`,
      label: e.label,
      affectedLayerIds: e.affected,
      versionBefore: e.vb,
      versionAfter: e.va,
      memoryCostBytes: e.bytes,
      payloadRef: e.token ?? null,
    })),
  };
}

export function historyCursorCommit(seq: number, direction: "undo" | "redo"): CommandResult {
  if (wasm?.protocol_history_cursor_commit) {
    return JSON.parse(wasm.protocol_history_cursor_commit(JSON.stringify({ seq, direction }))) as CommandResult;
  }
  const pendingMatches = emuPendingExternal?.seq === seq && emuPendingExternal.direction === direction;
  const ok = direction === "undo" ? emuCursor === seq : emuCursor + 1 === seq;
  if (!ok || !pendingMatches) {
    throw new Error(
      `E_CURSOR_MISMATCH: cursor ${emuCursor} incompatible with seq ${seq} direction ${direction} (pending: ${JSON.stringify(emuPendingExternal)})`,
    );
  }
  emuCursor += direction === "undo" ? -1 : 1;
  emuVersion += 1;
  emuPendingExternal = null; // barrier cleared on success only
  return {
    documentVersion: emuVersion,
    delta: { baseVersion: emuVersion - 1, version: emuVersion, changes: [] },
    status: "external-confirmed",
    externalSeq: seq,
  };
}

export function __resetEmulatedForTests(): void {
  emuVersion = 0;
  emuLayers = [];
  emuNextResource = 1;
  emuHistory = [];
  emuFuture = [];
  emuEntries = [];
  emuCursor = 0;
  emuNextSeq = 1;
  emuAdapters.clear();
  emuPendingExternal = null;
}

function diffEmu(old: RenderSnapshot["layers"], next: RenderSnapshot["layers"]): CommandResult["delta"]["changes"] {
  const changes: CommandResult["delta"]["changes"] = [];
  for (const l of next) {
    const o = old.find((x) => x.id === l.id);
    if (!o || JSON.stringify(o) !== JSON.stringify(l)) changes.push({ kind: "upsert", layer: l });
  }
  for (const o of old) if (!next.find((x) => x.id === o.id)) changes.push({ kind: "remove", id: o.id, resourceId: o.resourceId });
  return changes;
}

function emulateApply(env: CommandEnvelope): CommandResult {
  if (env.expectedVersion !== undefined && env.expectedVersion !== emuVersion) {
    throw new Error(`E_VERSION_MISMATCH: expected version ${env.expectedVersion} got ${emuVersion}`);
  }
  // External-pending barrier (ADR 0008 H0 invariant): host owes a cursor
  // commit — every command rejects until it lands.
  if (emuPendingExternal) {
    throw new Error(
      `E_EXTERNAL_PENDING: external history transition pending: commit seq ${emuPendingExternal.seq} direction ${emuPendingExternal.direction} first`,
    );
  }
  const base = emuVersion;
  let changes: CommandResult["delta"]["changes"] = [];
  let externalSeq: number | null = null;
  const cmd = env.command as unknown as { type: string; [k: string]: unknown };
  if (cmd.type === "ping") {
    const echo = cmd.echo as string;
    const id = `ping:${echo}`;
    const layer = { id, name: echo, visible: true, opacity: 1, resourceId: emuNextResource, x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, dirtyRect: { x: 0, y: 0, width: 1, height: 1 } };
    const idx = emuLayers.findIndex((l) => l.id === id);
    if (idx >= 0) emuLayers[idx] = layer;
    else { emuNextResource += 1; emuLayers.push(layer); }
    changes = [{ kind: "upsert", layer }];
  } else if (cmd.type === "noop") {
  } else if (cmd.type === "recordExternalTransition") {
    const adapterId = cmd.adapterId as string;
    if (adapterId !== "native" && !emuAdapters.has(adapterId)) {
      throw new Error(`E_UNKNOWN_ADAPTER: adapter '${adapterId}' is not registered`);
    }
    emuEntries = emuEntries.slice(0, emuCursor);
    const seq = emuNextSeq++;
    emuEntries.push({ seq, groupId: seq, origin: { external: adapterId }, label: cmd.label as string, affected: (cmd.affectedLayerIds as string[]) ?? [], vb: emuVersion, va: emuVersion + 1, bytes: (cmd.memoryCostBytes as number) ?? 0, before: [...emuLayers], after: [...emuLayers], token: cmd.token as string });
    emuCursor = emuEntries.length;
    emuVersion += 1;
    return {
      documentVersion: emuVersion,
      delta: { baseVersion: base, version: emuVersion, changes: [] },
      status: "external-recorded",
    };
  } else if (cmd.type === "addLayer") {
    const _e = beginEmu("Add Layer", []);
    const id = `layer-${Date.now()}-${Math.random().toString(36).slice(2, 6)}`;
    const layer = { id, name: cmd.name as string, visible: true, opacity: 1, resourceId: emuNextResource, x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, dirtyRect: { x: 0, y: 0, width: 1, height: 1 } };
    emuNextResource += 1; emuLayers.push(layer); finishEmu(_e); changes = [{ kind: "upsert", layer }];
  } else if (cmd.type === "deleteLayer") {
    const id = cmd.id as string;
    const idx = emuLayers.findIndex((l) => l.id === id);
    if (idx >= 0) { const _e = beginEmu("Delete Layer", [id]); const rid = emuLayers[idx].resourceId; emuLayers.splice(idx, 1); finishEmu(_e); changes = [{ kind: "remove", id, resourceId: rid }]; }
  } else if (cmd.type === "transformLayer") {
    const id = cmd.id as string;
    const idx = emuLayers.findIndex((l) => l.id === id);
    if (idx >= 0) { const _e = beginEmu("Transform Layer", [id]); const t = cmd.transform as { x:number;y:number;scaleX:number;scaleY:number;rotation:number }; const layer = { ...emuLayers[idx], x: t.x, y: t.y, scaleX: t.scaleX, scaleY: t.scaleY, rotation: t.rotation, dirtyRect: { x: 0, y: 0, width: 1, height: 1 } }; emuLayers[idx]=layer; finishEmu(_e); changes=[{kind:"upsert", layer}]; }
  } else if (cmd.type === "setOpacity") {
    const id = cmd.id as string;
    const idx = emuLayers.findIndex((l) => l.id === id);
    if (idx >= 0) { const _e = beginEmu("Set Opacity", [id]); const layer = { ...emuLayers[idx], opacity: Math.max(0, Math.min(1, cmd.opacity as number)), dirtyRect: { x: 0, y: 0, width: 1, height: 1 } }; emuLayers[idx]=layer; finishEmu(_e); changes=[{kind:"upsert", layer}]; }
  } else if (cmd.type === "brushStroke") {
    const id = cmd.layerId as string;
    const idx = emuLayers.findIndex((l) => l.id === id);
    const points = cmd.points as { x:number; y:number; pressure:number }[];
    const settings = cmd.settings as { size:number };
    if (points.length === 0) {
      changes = [];
    } else if (idx >= 0) {
      const _e = beginEmu("Brush Stroke", [id]);
      let minX = Infinity, maxX = -Infinity, minY = Infinity, maxY = -Infinity;
      for (const p of points) { if (p.x < minX) minX = p.x; if (p.x > maxX) maxX = p.x; if (p.y < minY) minY = p.y; if (p.y > maxY) maxY = p.y; }
      const radius = Math.ceil((settings.size as number) / 2);
      const x = Math.floor(minX) - radius;
      const y = Math.floor(minY) - radius;
      const w = Math.max(1, Math.ceil(maxX - minX) + Math.ceil(settings.size as number));
      const h = Math.max(1, Math.ceil(maxY - minY) + Math.ceil(settings.size as number));
      const layer = { ...emuLayers[idx], dirtyRect: { x, y, width: w, height: h } };
      emuLayers[idx]=layer; finishEmu(_e); changes=[{kind:"upsert", layer}];
    }
  } else if (cmd.type === "undo") {
    if (emuCursor === 0) {
      // no-op undo: DV still bumps below
    } else {
      const e = emuEntries[emuCursor - 1];
      if (e.origin === "native") {
        const cur = [...emuLayers];
        changes = diffEmu(cur, e.before);
        emuLayers = [...e.before];
        emuCursor -= 1;
      } else {
        externalSeq = e.seq; // host handoff — no cursor move, no DV bump here
        emuPendingExternal = { seq: e.seq, direction: "undo" };
      }
    }
  } else if (cmd.type === "redo") {
    if (emuCursor < emuEntries.length) {
      const e = emuEntries[emuCursor];
      if (e.origin === "native") {
        const cur = [...emuLayers];
        changes = diffEmu(cur, e.after);
        emuLayers = [...e.after];
        emuCursor += 1;
      } else {
        externalSeq = e.seq;
        emuPendingExternal = { seq: e.seq, direction: "redo" };
      }
    }
  }
  if (externalSeq !== null) {
    return {
      documentVersion: emuVersion,
      delta: { baseVersion: base, version: emuVersion, changes: [] },
      status: "external",
      externalSeq,
    };
  }
  emuVersion += 1;
  return { documentVersion: emuVersion, delta: { baseVersion: base, version: emuVersion, changes } };
}
