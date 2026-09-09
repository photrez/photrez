import type {
  CommandEnvelope,
  CommandResult,
  RenderLayer,
  RenderSnapshot,
  HistoryQueryResult,
} from "./types";
import { nativeProtocol } from "./nativeClient";
import {
  awaitNativeSeed,
  isNativeAuthority,
  nativeAdapterRegByDoc,
  nativeSeedPromiseByDoc,
  normalizeProtocolError,
  wasm,
} from "./bridge";

// In-memory emulator: a JS mirror of the Rust ProtocolEngine (applyCommand falls
// back to this when the wasm runtime is unarmed). Extracted from bridge.ts to keep
// that module under the 1000-line guard. Re-exported from bridge.ts so every
// existing import path stays valid; bridge.ts dispatches here via emulateApply.
//
// JS emulation mirrors Rust ProtocolEngine - includes expectedVersion check.
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
  // Canvas size before/after this entry (the crop/resize arms mutate it). Stored
  // so undo/redo restores the document size, mirroring the Rust EntryPayload doc
  // size swap in document_core_apply.rs.
  docWBefore: number;
  docHBefore: number;
  docWAfter: number;
  docHAfter: number;
  token?: string;
};
let emuEntries: EmuEntry[] = [];
let emuCursor = 0;
let emuNextSeq = 1;
const emuAdapters = new Set<string>();
// External-pending barrier (H0 invariant): while set, EVERY emulated command
// rejects with E_EXTERNAL_PENDING until the matching cursor commit lands.
let emuPendingExternal: { seq: number; direction: "undo" | "redo" } | null = null;
// Engine-local selection mirror. The emulator has no
// canonical shadow, so selectAll dims come from an explicit test hook
// (setEmuDocumentDims). Native reads the canonical shadow seeded at open; the
// emulator approximates via the hook.
let emuSelection: any = null;
let emuDocWidth = 0;
let emuDocHeight = 0;

// TS-side stand-ins for the Rust `estimate_*` constants. The per-layer struct
// base in Rust is `size_of::<RenderLayer>()` (not knowable in TS), so this is a
// stable approximation that keeps the emulator on the SAME SEMANTICS as the
// engine (unique layer count + per-layer byte estimate + set buffers) rather
// than the stale JSON-serialize double-count.
//
// Contract note: the COUNT SEMANTICS are the
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

// Normalize a rotation angle in degrees to the canonical (-180, 180] range,
// mirroring the Rust apply_crop_canvas normalize_rotation (and the TS
// normalizeRotation in viewport/transformGeometry.ts): a = angle % 360, then
// > 180 -> -360, then < -180 -> +360. JS % on a negative dividend matches Rust
// (e.g. -270 % 360 === -270 -> +360 === 90).
function normalizeRotation(angleDeg: number): number {
  let a = angleDeg % 360;
  if (a > 180) a -= 360;
  if (a < -180) a += 360;
  return a;
}

function beginEmu(label: string, affected: string[]): number {
  emuEntries = emuEntries.slice(0, emuCursor);
  const seq = emuNextSeq++;
  emuEntries.push({ seq, groupId: seq, origin: "native", label, affected, vb: emuVersion, va: emuVersion + 1, bytes: 0, before: [...emuLayers], after: [], docWBefore: emuDocWidth, docHBefore: emuDocHeight, docWAfter: emuDocWidth, docHAfter: emuDocHeight });
  return emuEntries.length - 1;
}
function finishEmu(idx: number): void {
  const e = emuEntries[idx];
  if (e) {
    e.after = [...emuLayers];
    e.docWAfter = emuDocWidth;
    e.docHAfter = emuDocHeight;
    e.bytes = estimateEmuNativeBytes(e.before, e.after);
  }
  emuCursor = emuEntries.length;
}

// Per-document adapter registration. The engine owns its adapters, so a fresh
// per-document engine must be registered before its first external transition.
// Register before EVERY record; dedup via the nativeAdapterRegByDoc key so
// repeated calls reuse the same promise instead of spawning redundant
// registrations. A per-doc engine reset evicts the key (clearNativeSeed deletes
// every `${key}::` entry), so the next record re-registers against the fresh
// engine: the dedup does NOT block re-registration after a reset.
export function registerPayloadAdapter(adapterId: string, docId = "default"): void {
  if (isNativeAuthority()) {
    // Register the adapter on the per-document native ProtocolEngine so the first
    // legacy-commit mirror (recordExternalTransitionFor) does not hit Rust
    // E_UNKNOWN_ADAPTER. Chained after the authoritative seed so the engine exists;
    // idempotent per (doc, adapter).
    const key = docId === "" ? "default" : docId;
    const regKey = `${key}::${adapterId}`;
    if (!nativeAdapterRegByDoc.has(regKey)) {
      const seedP = nativeSeedPromiseByDoc.get(key) ?? Promise.resolve();
      nativeAdapterRegByDoc.set(
        regKey,
        seedP.then(async () => {
          // Surface a failed registration instead of swallowing it: a swallowed
          // rejection here degrades the ordering guarantee to a downstream
          // E_UNKNOWN_ADAPTER. (The Rust command is idempotent, so a genuine
          // failure is a real engine problem that must not be hidden.)
          await nativeProtocol.protocol_register_adapter_native(key, adapterId);
        }),
      );
    }
    return;
  }
  if (wasm?.protocol_register_payload_adapter) wasm.protocol_register_payload_adapter(adapterId, docId);
  else if (adapterId !== "native") emuAdapters.add(adapterId);
}

export async function getHistoryQuery(docId = "default"): Promise<HistoryQueryResult> {
  if (isNativeAuthority()) {
    await awaitNativeSeed(docId);
    try {
      const j = await nativeProtocol.protocol_history_query_native(docId);
      return JSON.parse(j) as HistoryQueryResult;
    } catch (e) {
      throw normalizeProtocolError(e);
    }
  }
  if (wasm?.protocol_history_query_json) return JSON.parse(wasm.protocol_history_query_json(docId)) as HistoryQueryResult;
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

export async function historyCursorCommit(seq: number, direction: "undo" | "redo", docId = "default"): Promise<CommandResult> {
  if (isNativeAuthority()) {
    await awaitNativeSeed(docId);
    try {
      const j = await nativeProtocol.protocol_history_cursor_commit_native(docId, seq, direction);
      return JSON.parse(j) as CommandResult;
    } catch (e) {
      throw normalizeProtocolError(e);
    }
  }
  if (wasm?.protocol_history_cursor_commit) {
    return JSON.parse(wasm.protocol_history_cursor_commit(JSON.stringify({ seq, direction }), docId)) as CommandResult;
  }
  // ADR 0008 C1: HistorySeq (monotonic entry id) != HistoryCursor (position).
  // On a redo-truncated (non-dense) stream entries[i].seq != i+1, so index
  // arithmetic (cursor == seq) is wrong. Validation relies on the walker-recorded
  // barrier (seq, direction) alone, mirroring the Rust predicate in protocol.rs.
  const pendingMatches = emuPendingExternal?.seq === seq && emuPendingExternal.direction === direction;
  if (!pendingMatches) {
    throw new Error(
      `E_CURSOR_MISMATCH: cursor ${emuCursor} pendingExternal ${JSON.stringify(emuPendingExternal)} incompatible with seq ${seq} direction ${direction}`,
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

// Resets the per-document engine backing one doc id (no-op when the wasm is
// unarmed). Used by the registry reset so engine state does not leak across
// tests that use real doc ids (e.g. getFacade("doc-a")).
export function resetWasmDoc(docId = "default"): void {
  if (wasm?.protocol_reset) wasm.protocol_reset(docId);
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
  emuSelection = null;
  emuDocWidth = 0;
  emuDocHeight = 0;
}

// Test hook: the emulator has no canonical shadow, so selectAll needs explicit
// document dims. Native reads the canonical shadow seeded at open; the emulator
// approximates via this hook.
export function setEmuDocumentDims(width: number, height: number): void {
  emuDocWidth = width;
  emuDocHeight = height;
}

// Test hook: the emulated CommandResult has no selection surface, so read the
// emulator's selection state directly to assert selection-arm behavior.
export function getEmuSelection(): any {
  return emuSelection;
}

// Test hook: read the emulator's current document (canvas) size, set by the
// cropCanvas / applyCrop / resizeCanvas arms and restored on undo/redo.
export function getEmuDocumentDims(): { width: number; height: number } {
  return { width: emuDocWidth, height: emuDocHeight };
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

// Local mirror of the Rust ProtocolEngine's next_duplicate_name (parity with the
// TS layerOps.nextDuplicateName): bump a numeric suffix past any existing sibling.
// The emulator has no canonical shadow, so names come from emuLayers directly.
function emuNextDuplicateName(names: string[], layerName: string): string {
  const m = layerName.match(/^(.*?)\s*(\d+)$/);
  const base = m ? m[1].trimEnd() : layerName.trimEnd();
  const prefix = `${base} `;
  let maxNum = 1;
  for (const name of names) {
    if (name.startsWith(prefix)) {
      const num = parseInt(name.slice(prefix.length), 10);
      if (!isNaN(num) && num > maxNum) maxNum = num;
    }
  }
  return `${base} ${maxNum + 1}`;
}

export function emulateApply(env: CommandEnvelope, _docId?: string): CommandResult {
  if (env.expectedVersion !== undefined && env.expectedVersion !== emuVersion) {
    throw new Error(`E_VERSION_MISMATCH: expected version ${env.expectedVersion} got ${emuVersion}`);
  }
  // External-pending barrier (ADR 0008 H0 invariant): host owes a cursor
  // commit - every command rejects until it lands.
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
  } else if (cmd.type === "setSelection") {
    // Mirror the Rust SetSelection arm: no history entry, empty delta, DV bump.
    // The Rust arm validates finite/non-negative geometry; the emulator trusts the
    // host-supplied selection and mirrors the same no-entry semantics.
    emuSelection = cmd.selection;
  } else if (cmd.type === "clearSelection") {
    emuSelection = null;
  } else if (cmd.type === "selectAll") {
    // Emulator has no canonical shadow; dims come from the explicit test hook.
    emuSelection = { x: 0, y: 0, width: emuDocWidth, height: emuDocHeight, angle: 0, shape: undefined, inverted: undefined };
  } else if (cmd.type === "invertSelection") {
    // Mirrors the host op, which falls back to select-all when nothing is
    // selected: with no emuSelection, build the same full-canvas rect selectAll
    // builds from the emu doc-dims hook. No hook + no selection rejects with
    // E_INVALID, matching the Rust arm / selectAll. With a selection, toggle.
    if (emuSelection) {
      emuSelection = { ...emuSelection, inverted: !emuSelection.inverted };
    } else if (emuDocWidth > 0 && emuDocHeight > 0) {
      emuSelection = { x: 0, y: 0, width: emuDocWidth, height: emuDocHeight, angle: 0, shape: undefined, inverted: undefined };
    } else {
      throw { code: "E_INVALID", message: "invertSelection with no selection falls back to select-all, which requires seeded document dims (call setEmuDocumentDims)" };
    }
  } else if (cmd.type === "recordExternalTransition") {
    const adapterId = cmd.adapterId as string;
    if (adapterId !== "native" && !emuAdapters.has(adapterId)) {
      throw new Error(`E_UNKNOWN_ADAPTER: adapter '${adapterId}' is not registered`);
    }
    emuEntries = emuEntries.slice(0, emuCursor);
    const seq = emuNextSeq++;
    emuEntries.push({ seq, groupId: seq, origin: { external: adapterId }, label: cmd.label as string, affected: (cmd.affectedLayerIds as string[]) ?? [], vb: emuVersion, va: emuVersion + 1, bytes: (cmd.memoryCostBytes as number) ?? 0, before: [...emuLayers], after: [...emuLayers], docWBefore: emuDocWidth, docHBefore: emuDocHeight, docWAfter: emuDocWidth, docHAfter: emuDocHeight, token: cmd.token as string });
    emuCursor = emuEntries.length;
    emuVersion += 1;
    return {
      documentVersion: emuVersion,
      delta: { baseVersion: base, version: emuVersion, changes: [] },
      status: "external-recorded",
    };
  } else if (cmd.type === "addLayer") {
    // Host owns identity + placement: use the payload id (TS-minted) and insert
    // at the supplied index (clamped), mirroring the Rust AddLayer arm. The
    // emulator no longer mints its own id (divergence #3 resolved). A non-empty
    // id is required and must be unique - the arm rejects an empty or duplicate
    // id with E_INVALID, so the emulator mirrors that instead of minting.
    const id = cmd.id as string | undefined;
    if (!id || id.length === 0) throw new Error("E_INVALID: addLayer requires a non-empty id");
    if (emuLayers.some((l) => l.id === id)) throw new Error(`E_INVALID: addLayer id '${id}' already exists`);
    const width = (cmd.width as number) ?? 100;
    const height = (cmd.height as number) ?? 100;
    const index = (cmd.index as number) ?? 0;
    const _e = beginEmu("Add Layer", [id]);
    // Mirror the Rust AddLayer arm: an absent layer type yields a raster/normal
    // layer; a present type projects the real layer (blendMode stays "normal",
    // the matching nested payload rides verbatim).
    const layerType = (cmd.layerType as string | undefined) ?? "raster";
    const layer = {
      id,
      name: cmd.name as string,
      visible: true,
      opacity: 1,
      resourceId: emuNextResource,
      x: 0,
      y: 0,
      scaleX: 1,
      scaleY: 1,
      rotation: 0,
      dirtyRect: { x: 0, y: 0, width: 1, height: 1 },
      width,
      height,
      layerType,
      blendMode: "normal",
      shapeParams: (cmd.shapeParams as any) ?? undefined,
      textData: (cmd.textData as any) ?? undefined,
    };
    emuNextResource += 1;
    const idx = index < 0 ? 0 : Math.min(index, emuLayers.length);
    emuLayers.splice(idx, 0, layer);
    finishEmu(_e);
    changes = [{ kind: "upsert", layer }];
  } else if (cmd.type === "deleteLayer") {
    const id = cmd.id as string;
    const idx = emuLayers.findIndex((l) => l.id === id);
    if (idx >= 0) { const _e = beginEmu("Delete Layer", [id]); const rid = emuLayers[idx].resourceId; emuLayers.splice(idx, 1); finishEmu(_e); changes = [{ kind: "remove", id, resourceId: rid }]; }
  } else if (cmd.type === "transformLayer") {
    const id = cmd.id as string;
    const idx = emuLayers.findIndex((l) => l.id === id);
    if (idx >= 0) {
      const _e = beginEmu("Transform Layer", [id]);
      const t = cmd.transform as { x?:number;y?:number;scaleX?:number;scaleY?:number;rotation?:number;flipH?:boolean;flipV?:boolean };
      const layer = { ...emuLayers[idx], dirtyRect: { x: 0, y: 0, width: 1, height: 1 } };
      if (t.x !== undefined) layer.x = t.x;
      if (t.y !== undefined) layer.y = t.y;
      if (t.scaleX !== undefined) layer.scaleX = t.scaleX;
      if (t.scaleY !== undefined) layer.scaleY = t.scaleY;
      if (t.rotation !== undefined) layer.rotation = t.rotation;
      if (t.flipH !== undefined) layer.flipH = t.flipH;
      if (t.flipV !== undefined) layer.flipV = t.flipV;
      emuLayers[idx] = layer; finishEmu(_e); changes = [{ kind: "upsert", layer }];
    }
  } else if (cmd.type === "setVisible") {
    const id = cmd.id as string;
    const idx = emuLayers.findIndex((l) => l.id === id);
    if (idx >= 0) { const _e = beginEmu("Set Visible", [id]); const layer = { ...emuLayers[idx], visible: cmd.visible as boolean, dirtyRect: { x: 0, y: 0, width: 1, height: 1 } }; emuLayers[idx]=layer; finishEmu(_e); changes=[{kind:"upsert", layer}]; }
  } else if (cmd.type === "setLocked") {
    const id = cmd.id as string;
    const idx = emuLayers.findIndex((l) => l.id === id);
    if (idx >= 0) {
      const _e = beginEmu("Set Lock", [id]);
      const layer = { ...emuLayers[idx], dirtyRect: { x: 0, y: 0, width: 1, height: 1 } };
      const kind = cmd.kind as string;
      const locked = cmd.locked as boolean;
      if (kind === "base") layer.locked = locked;
      else if (kind === "transparency") layer.lockTransparency = locked;
      else if (kind === "position") layer.lockPosition = locked;
      else if (kind === "rotation") layer.lockRotation = locked;
      emuLayers[idx] = layer; finishEmu(_e); changes = [{ kind: "upsert", layer }];
    }
  } else if (cmd.type === "rename") {
    const id = cmd.id as string;
    const idx = emuLayers.findIndex((l) => l.id === id);
    if (idx >= 0) { const _e = beginEmu("Rename", [id]); const layer = { ...emuLayers[idx], name: cmd.name as string, dirtyRect: { x: 0, y: 0, width: 1, height: 1 } }; emuLayers[idx]=layer; finishEmu(_e); changes=[{kind:"upsert", layer}]; }
  } else if (cmd.type === "reorder") {
    const id = cmd.id as string;
    const idx = emuLayers.findIndex((l) => l.id === id);
    if (idx >= 0) {
      const to = cmd.to as number;
      // Mirror the TS graph-mirror reorder_layer guard: an out-of-range target is
      // invalid. Reject before any mutation, history entry, or DV bump.
      if (to < 0 || to >= emuLayers.length) {
        throw new Error(
          `E_INVALID: reorder target index ${to} out of range [0, ${emuLayers.length - 1}]`,
        );
      }
      // Mirror TS applyReorderLayer: Background pinned to bottom, never reordered.
      if (emuLayers[idx].isBackground) {
        // no-op reorder (Background) - no history entry; the DV still bumps below.
      } else {
        const _e = beginEmu("Reorder", [id]);
        const clampedTo = Math.max(0, Math.min(to, emuLayers.length - 1));
        const [moved] = emuLayers.splice(idx, 1);
        emuLayers.splice(clampedTo, 0, moved);
        const bgIdx = emuLayers.findIndex((l) => l.isBackground);
        if (bgIdx >= 0 && bgIdx !== emuLayers.length - 1) {
          const [bg] = emuLayers.splice(bgIdx, 1);
          emuLayers.push(bg);
        }
        finishEmu(_e);
        changes = emuLayers.map((l) => ({ kind: "upsert", layer: l }));
      }
    }
  } else if (cmd.type === "setBackgroundFlag") {
    const id = cmd.id as string;
    const idx = emuLayers.findIndex((l) => l.id === id);
    if (idx >= 0) { const _e = beginEmu("Set Background Flag", [id]); const layer = { ...emuLayers[idx], isBackground: true, lockPosition: true, lockRotation: true, dirtyRect: { x: 0, y: 0, width: 1, height: 1 } }; emuLayers[idx]=layer; finishEmu(_e); changes=[{kind:"upsert", layer}]; }
  } else if (cmd.type === "setBlendMode") {
    const id = cmd.id as string;
    const idx = emuLayers.findIndex((l) => l.id === id);
    if (idx >= 0) { const _e = beginEmu("Set Blend Mode", [id]); const layer = { ...emuLayers[idx], blendMode: cmd.mode as string, dirtyRect: { x: 0, y: 0, width: 1, height: 1 } }; emuLayers[idx]=layer; finishEmu(_e); changes=[{kind:"upsert", layer}]; }
  } else if (cmd.type === "setLayerParams") {
    // Mirror the Rust SetLayerParams arm: both absent is an invalid no-op
    // (reject before any mutation); whichever is present is written; an unknown
    // id is a silent no-op (no delta).
    const id = cmd.id as string;
    const sp = cmd.shapeParams as any;
    const td = cmd.textData as any;
    if (!sp && !td) throw new Error("E_INVALID: setLayerParams requires shapeParams or textData");
    const idx = emuLayers.findIndex((l) => l.id === id);
    if (idx >= 0) {
      const _e = beginEmu("Set Layer Params", [id]);
      const layer = { ...emuLayers[idx], dirtyRect: { x: 0, y: 0, width: 1, height: 1 } };
      if (sp) layer.shapeParams = sp;
      if (td) layer.textData = td;
      emuLayers[idx] = layer; finishEmu(_e); changes = [{ kind: "upsert", layer }];
    }
  } else if (cmd.type === "setAdjustment") {
    // Mirror the Rust SetAdjustment arm (and TS applyBasicAdjustment /
    // clearBasicAdjustments): Some clamps each channel to [-100, 100] and derives
    // hasAdjustments from whether any channel is non-zero; None clears and sets
    // hasAdjustments false. Unknown id is a silent no-op.
    const id = cmd.id as string;
    const adj = cmd.adjustment as { brightness: number; contrast: number; saturation: number } | undefined;
    const idx = emuLayers.findIndex((l) => l.id === id);
    if (idx >= 0) {
      const _e = beginEmu("Set Adjustment", [id]);
      const layer = { ...emuLayers[idx], dirtyRect: { x: 0, y: 0, width: 1, height: 1 } };
      if (adj) {
        const clamp = (v: number) => Math.max(-100, Math.min(100, v));
        layer.basicAdjustment = {
          brightness: clamp(adj.brightness),
          contrast: clamp(adj.contrast),
          saturation: clamp(adj.saturation),
        };
        layer.hasAdjustments = adj.brightness !== 0 || adj.contrast !== 0 || adj.saturation !== 0;
      } else {
        layer.basicAdjustment = undefined;
        layer.hasAdjustments = false;
      }
      emuLayers[idx] = layer; finishEmu(_e); changes = [{ kind: "upsert", layer }];
    }
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
  } else if (cmd.type === "duplicateLayer") {
    // Mirror Rust apply_duplicate: clone the source verbatim, derive a numeric-
    // suffix name, mint a fresh resource id (the clone owns its own bitmap), drop
    // the background flag + locks. Insert directly above the source.
    const id = cmd.id as string;
    const newId = cmd.newId as string;
    if (!newId || newId.length === 0) {
      throw new Error("E_INVALID: duplicateLayer newId must not be empty");
    }
    if (emuLayers.some((l) => l.id === newId)) {
      throw new Error(`E_INVALID: duplicateLayer newId already present: ${newId}`);
    }
    const idx = emuLayers.findIndex((l) => l.id === id);
    if (idx < 0) {
      // Unknown source id is a silent no-op (mirrors DeleteLayer / TS guarded op).
    } else {
      const src = emuLayers[idx];
      const names = emuLayers.map((l) => l.name);
      const name = emuNextDuplicateName(names, src.name);
      const rid = emuNextResource;
      emuNextResource += 1;
      const nl: any = { ...src };
      nl.id = newId;
      nl.name = name;
      nl.resourceId = rid;
      nl.locked = false;
      nl.isBackground = undefined;
      nl.lockPosition = undefined;
      nl.lockRotation = undefined;
      nl.lockTransparency = undefined;
      const _e = beginEmu("Duplicate Layer", [newId]);
      emuLayers.splice(idx, 0, nl);
      finishEmu(_e);
      changes = emuLayers.map((l) => ({ kind: "upsert", layer: l }));
    }
  } else if (cmd.type === "mergeDown") {
    // Mirror Rust apply_merge_down: top + bottom -> merged raster named "top +
    // bottom", blend = bottom's blendMode, locked = either source locked. Unknown
    // id or a bottom-most layer is a silent no-op. Dims from the emu doc-dims hook.
    const id = cmd.id as string;
    const mergedId = cmd.mergedId as string;
    if (!mergedId || mergedId.length === 0) {
      throw new Error("E_INVALID: mergeDown mergedId must not be empty");
    }
    if (emuLayers.some((l) => l.id === mergedId)) {
      throw new Error(`E_INVALID: mergeDown mergedId already present: ${mergedId}`);
    }
    const pos = emuLayers.findIndex((l) => l.id === id);
    if (pos < 0) {
      // unknown id no-op
    } else {
      const bottomPos = pos + 1;
      if (bottomPos >= emuLayers.length) {
        // bottom-most: nothing below to merge
      } else {
        if (!(emuDocWidth > 0 && emuDocHeight > 0)) {
          throw new Error(
            "E_INVALID: mergeDown requires document dims (call setEmuDocumentDims)",
          );
        }
        const top = emuLayers[pos];
        const bottom = emuLayers[bottomPos];
        const name = `${top.name} + ${bottom.name}`;
        const locked = !!(bottom.locked || top.locked);
        const blendMode = bottom.blendMode ?? "normal";
        const w = emuDocWidth;
        const h = emuDocHeight;
        const rid = emuNextResource;
        emuNextResource += 1;
        const merged: any = {
          id: mergedId,
          name,
          visible: true,
          opacity: 1,
          resourceId: rid,
          x: 0,
          y: 0,
          scaleX: 1,
          scaleY: 1,
          rotation: 0,
          dirtyRect: { x: 0, y: 0, width: 1, height: 1 },
          layerType: "raster",
          blendMode,
          locked,
          isBackground: undefined,
          lockPosition: undefined,
          lockRotation: undefined,
          lockTransparency: undefined,
          width: w,
          height: h,
          shapeParams: undefined,
          textData: undefined,
        };
        const _e = beginEmu("Merge Down", [top.id, bottom.id, mergedId]);
        // Remove bottom then top (descending index keeps positions stable), then
        // insert the merged node back at the pair's original position.
        emuLayers.splice(bottomPos, 1);
        emuLayers.splice(pos, 1);
        emuLayers.splice(pos, 0, merged);
        finishEmu(_e);
        changes = [
          { kind: "remove", id: top.id, resourceId: top.resourceId },
          { kind: "remove", id: bottom.id, resourceId: bottom.resourceId },
          ...emuLayers.map((l) => ({ kind: "upsert" as const, layer: l })),
        ];
      }
    }
  } else if (cmd.type === "mergeSelected") {
    // Mirror Rust apply_merge_selected: selected = engine layers with id in ids IN
    // STACK ORDER (top first); name "A + B" (2) or "{first} (+{n-1} merged)" (>2);
    // blend always Normal; locked when ANY selected locked. <2 matched ids is a
    // silent no-op. Merged node placed at the highest stack position (first
    // occurrence index) among the selected.
    const ids = (cmd.ids as string[]) ?? [];
    const mergedId = cmd.mergedId as string;
    if (!mergedId || mergedId.length === 0) {
      throw new Error("E_INVALID: mergeSelected mergedId must not be empty");
    }
    if (emuLayers.some((l) => l.id === mergedId)) {
      throw new Error(`E_INVALID: mergeSelected mergedId already present: ${mergedId}`);
    }
    if (ids.length < 2) {
      // no-op
    } else {
      const selected = emuLayers.filter((l) => ids.includes(l.id));
      if (selected.length < 2) {
        // no-op (oracle filter: only present ids count)
      } else {
        if (!(emuDocWidth > 0 && emuDocHeight > 0)) {
          throw new Error(
            "E_INVALID: mergeSelected requires document dims (call setEmuDocumentDims)",
          );
        }
        const name =
          selected.length === 2
            ? `${selected[0].name} + ${selected[1].name}`
            : `${selected[0].name} (+${selected.length - 1} merged)`;
        const locked = selected.some((l) => l.locked);
        const w = emuDocWidth;
        const h = emuDocHeight;
        const rid = emuNextResource;
        emuNextResource += 1;
        const merged: any = {
          id: mergedId,
          name,
          visible: true,
          opacity: 1,
          resourceId: rid,
          x: 0,
          y: 0,
          scaleX: 1,
          scaleY: 1,
          rotation: 0,
          dirtyRect: { x: 0, y: 0, width: 1, height: 1 },
          layerType: "raster",
          blendMode: "normal",
          locked,
          isBackground: undefined,
          lockPosition: undefined,
          lockRotation: undefined,
          lockTransparency: undefined,
          width: w,
          height: h,
          shapeParams: undefined,
          textData: undefined,
        };
        let highestPos = emuLayers.length;
        for (let i = 0; i < emuLayers.length; i++) {
          if (ids.includes(emuLayers[i].id)) {
            highestPos = i;
            break;
          }
        }
        const _e = beginEmu("Merge Selected Layers", [...selected.map((l) => l.id), mergedId]);
        const removed: Array<{ id: string; resourceId: number }> = [];
        for (let i = emuLayers.length - 1; i >= 0; i--) {
          if (ids.includes(emuLayers[i].id)) {
            removed.push({ id: emuLayers[i].id, resourceId: emuLayers[i].resourceId });
            emuLayers.splice(i, 1);
          }
        }
        emuLayers.splice(highestPos, 0, merged);
        finishEmu(_e);
        changes = [
          ...removed.map((r) => ({ kind: "remove" as const, id: r.id, resourceId: r.resourceId })),
          ...emuLayers.map((l) => ({ kind: "upsert" as const, layer: l })),
        ];
      }
    }
  } else if (cmd.type === "flatten") {
    // Mirror Rust apply_flatten: a single "Background" raster node, not locked, but
    // carrying the background flag + position/rotation locks. A single-layer doc is
    // a silent no-op. Dims from the emu doc-dims hook.
    const mergedId = cmd.mergedId as string;
    if (!mergedId || mergedId.length === 0) {
      throw new Error("E_INVALID: flatten mergedId must not be empty");
    }
    if (emuLayers.some((l) => l.id === mergedId)) {
      throw new Error(`E_INVALID: flatten mergedId already present: ${mergedId}`);
    }
    if (emuLayers.length <= 1) {
      // no-op
    } else {
      if (!(emuDocWidth > 0 && emuDocHeight > 0)) {
        throw new Error(
          "E_INVALID: flatten requires document dims (call setEmuDocumentDims)",
        );
      }
      const w = emuDocWidth;
      const h = emuDocHeight;
      const rid = emuNextResource;
      emuNextResource += 1;
      const merged: any = {
        id: mergedId,
        name: "Background",
        visible: true,
        opacity: 1,
        resourceId: rid,
        x: 0,
        y: 0,
        scaleX: 1,
        scaleY: 1,
        rotation: 0,
        dirtyRect: { x: 0, y: 0, width: 1, height: 1 },
        layerType: "raster",
        blendMode: "normal",
        locked: false,
        isBackground: true,
        lockPosition: true,
        lockRotation: true,
        lockTransparency: undefined,
        width: w,
        height: h,
        shapeParams: undefined,
        textData: undefined,
      };
      const removed: Array<{ id: string; resourceId: number }> = emuLayers.map((l) => ({
        id: l.id,
        resourceId: l.resourceId,
      }));
      const _e = beginEmu("Flatten Image", [...removed.map((r) => r.id), mergedId]);
      emuLayers = [merged];
      finishEmu(_e);
      changes = [
        ...removed.map((r) => ({ kind: "remove" as const, id: r.id, resourceId: r.resourceId })),
        { kind: "upsert", layer: merged },
      ];
    }
  } else if (cmd.type === "rasterizeLayer") {
    // Mirror shapeLayerToRaster / textLayerToRaster: drop the shape/text params and
    // become a plain raster layer; keep bitmap dims/transform/adjustments. A
    // non-parametric layer is a silent no-op (the oracle's type guard returns
    // silently). Unknown id is a silent no-op.
    const id = cmd.id as string;
    const idx = emuLayers.findIndex((l) => l.id === id);
    if (idx < 0) {
      // unknown id no-op
    } else {
      const l = emuLayers[idx];
      const isParametric = l.layerType === "shape" || l.layerType === "text";
      if (!isParametric) {
        // no-op: raster/other layers stay as-is
      } else {
        const nl: any = { ...l };
        nl.layerType = "raster";
        if (l.layerType === "shape") nl.shapeParams = undefined;
        else nl.textData = undefined;
        nl.dirtyRect = { x: 0, y: 0, width: 1, height: 1 };
        const _e = beginEmu("Rasterize Layer", [id]);
        emuLayers[idx] = nl;
        finishEmu(_e);
        changes = [{ kind: "upsert", layer: nl }];
      }
    }
  } else if (cmd.type === "cropCanvas") {
    // Mirror Rust apply_crop_canvas (command.rs CropCanvas): offset every unlocked
    // layer by (-x, -y) and set the document size to (width, height). Locked layers
    // (RenderLayer.locked === true) are untouched; selection is cleared. Trust
    // boundary: non-finite inputs reject E_INVALID before mutation (stricter than
    // the host, which lets NaN slip through its <= 0 comparisons). Non-positive
    // width/height is a SILENT no-op (no entry; DV bumps below).
    const x = cmd.x as number;
    const y = cmd.y as number;
    const width = cmd.width as number;
    const height = cmd.height as number;
    if (![x, y, width, height].every((v) => Number.isFinite(v))) {
      throw { code: "E_INVALID", message: "cropCanvas requires finite x/y/width/height" };
    }
    if (width <= 0 || height <= 0) {
      // silent no-op: no entry, snapshot unchanged
    } else {
      const _e = beginEmu("Crop Canvas", emuLayers.map((l) => l.id));
      for (let i = 0; i < emuLayers.length; i++) {
        const l = emuLayers[i];
        if (l.locked === true) continue;
        const nl = { ...l, x: l.x - x, y: l.y - y, dirtyRect: { x: 0, y: 0, width: 1, height: 1 } };
        emuLayers[i] = nl;
      }
      emuDocWidth = width;
      emuDocHeight = height;
      emuSelection = null;
      finishEmu(_e);
      changes = emuLayers.map((l) => ({ kind: "upsert" as const, layer: l }));
    }
  } else if (cmd.type === "applyCrop") {
    // Mirror Rust apply_apply_crop (command.rs ApplyCrop) non-destructive branch:
    // recenter + optionally rotate/scale every unlocked layer into the crop region
    // and set the document size to the (optional) target size. Trust boundary:
    // reject non-finite x/y/width/height/rotation with E_INVALID; a half
    // target-size pair (one of targetWidth/targetHeight without the other) rejects
    // E_INVALID; non-positive width/height is a silent no-op.
    const x = cmd.x as number;
    const y = cmd.y as number;
    const width = cmd.width as number;
    const height = cmd.height as number;
    const rot = (cmd.rotation as number | undefined) ?? 0;
    const tw = cmd.targetWidth as number | undefined;
    const th = cmd.targetHeight as number | undefined;
    if (![x, y, width, height, rot].every((v) => Number.isFinite(v))) {
      throw { code: "E_INVALID", message: "applyCrop requires finite x/y/width/height/rotation" };
    }
    if ((tw === undefined) !== (th === undefined)) {
      throw { code: "E_INVALID", message: "applyCrop target size requires both targetWidth and targetHeight" };
    }
    if (width <= 0 || height <= 0) {
      // silent no-op
    } else {
      const finalW = tw ?? width;
      const finalH = th ?? height;
      const cropCenterX = x + width / 2;
      const cropCenterY = y + height / 2;
      const r = (-rot * Math.PI) / 180;
      const cos = Math.cos(r);
      const sin = Math.sin(r);
      const exportSx = finalW / width;
      const exportSy = finalH / height;
      const _e = beginEmu("Crop Canvas", emuLayers.map((l) => l.id));
      for (let i = 0; i < emuLayers.length; i++) {
        const l = emuLayers[i];
        if (l.locked === true) continue;
        const lw = l.width ?? 0;
        const lh = l.height ?? 0;
        const lsx = l.scaleX ?? 1;
        const lsy = l.scaleY ?? 1;
        const lcx = l.x + (lw * Math.abs(lsx)) / 2;
        const lcy = l.y + (lh * Math.abs(lsy)) / 2;
        const vx = lcx - cropCenterX;
        const vy = lcy - cropCenterY;
        const rvx = vx * cos - vy * sin;
        const rvy = vx * sin + vy * cos;
        const nlcx = width / 2 + rvx;
        const nlcy = height / 2 + rvy;
        const finalCx = nlcx * exportSx;
        const finalCy = nlcy * exportSy;
        const finalSx = lsx * exportSx;
        const finalSy = lsy * exportSy;
        const finalRot = normalizeRotation((l.rotation ?? 0) - rot);
        const newX = finalCx - (lw * Math.abs(finalSx)) / 2;
        const newY = finalCy - (lh * Math.abs(finalSy)) / 2;
        const nl = {
          ...l,
          x: newX,
          y: newY,
          scaleX: finalSx,
          scaleY: finalSy,
          rotation: finalRot,
          dirtyRect: { x: 0, y: 0, width: 1, height: 1 },
        };
        emuLayers[i] = nl;
      }
      emuDocWidth = finalW;
      emuDocHeight = finalH;
      emuSelection = null;
      finishEmu(_e);
      changes = emuLayers.map((l) => ({ kind: "upsert" as const, layer: l }));
    }
  } else if (cmd.type === "resizeCanvas") {
    // Mirror Rust apply_resize_canvas (command.rs ResizeCanvas): only the document
    // size changes, no layer is touched. Trust boundary: non-finite dims reject
    // E_INVALID before mutation; non-positive dims are a silent no-op. Emits an
    // empty layer delta (the document size rides the snapshot); the entry records
    // the size so undo/redo restores it.
    const width = cmd.width as number;
    const height = cmd.height as number;
    if (![width, height].every((v) => Number.isFinite(v))) {
      throw { code: "E_INVALID", message: "resizeCanvas requires finite width/height" };
    }
    if (width <= 0 || height <= 0) {
      // silent no-op: no entry
    } else {
      const _e = beginEmu("Resize Canvas", []);
      emuDocWidth = width;
      emuDocHeight = height;
      finishEmu(_e);
      changes = [];
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
        emuDocWidth = e.docWBefore;
        emuDocHeight = e.docHBefore;
        emuCursor -= 1;
      } else {
        externalSeq = e.seq; // host handoff - no cursor move, no DV bump here
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
        emuDocWidth = e.docWAfter;
        emuDocHeight = e.docHAfter;
        emuCursor += 1;
      } else {
        externalSeq = e.seq;
        emuPendingExternal = { seq: e.seq, direction: "redo" };
      }
    }
  } else {
    // Exhaustiveness guard: every Command variant is handled above. An unknown
    // command type must fail loud, never silently fall through to an empty delta
    // (data-loss class per the protocol design).
    throw new Error(`E_UNKNOWN_COMMAND: ${String((cmd as { type?: string }).type)}`);
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
