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
import { getWasmExportModule } from "@/components/editor/wasmExport";
import { invoke } from "@tauri-apps/api/core";
import { nativeProtocol } from "./nativeClient";

type WasmProtocol = {
  protocol_contract_version: () => number;
  protocol_apply_command: (json: string, docId: string) => string;
  protocol_snapshot_json: (docId: string) => string;
  protocol_reset?: (docId: string) => void;
  // ADR 0008 H0 exports (optional so an older pkg degrades to emulation):
  protocol_register_payload_adapter?: (id: string, docId: string) => void;
  protocol_history_query_json?: (docId: string) => string;
  protocol_history_cursor_commit?: (json: string, docId: string) => string;
};

let wasm: WasmProtocol | null = null;

export function setProtocolWasm(mod: WasmProtocol): void {
  // Stale-pkg arity guard (document-scoped engine): the per-document protocol
  // requires a 2-arg protocol_apply_command(json, docId). An older shared-engine
  // pkg exports a 1-arg wrapper and JS silently drops the extra docId, so per-doc
  // isolation would collapse into a single "default" engine while every test
  // stayed green - the "green-but-wrong" anti-pattern this guard stops.
  //
  // HONEST SCOPE: this guard throws at arm time, so a DIRECT setProtocolWasm call
  // with a stale pkg fails the suite loudly (the direct test path). In the
  // PRODUCTION path the pkg is armed through wasmExport.ts getWasmExportModule(),
  // whose catch-all downgrades any arm-time throw (including this one) to
  // console.warn + null. So the real E_STALE_WASM cause lands only in a console
  // line; the user-visible symptom for a stale pkg under photrez.facade=1 is
  // E_FACADE_NOT_READY (ensureFacadeReady sees wasm===null), and under flag OFF
  // it is silent TS emulation. The guard is therefore suite-focused (it also
  // fails the real-wasm tests via their not.toBeNull / length>=2 guards) and
  // documents the stale-pkg class at arm time. (CONTRACT_VERSION is deliberately
  // NOT bumped: this arity probe is the guard, so a stale pkg fails HERE rather
  // than in every later assertion.)
  if (mod && typeof mod.protocol_apply_command === "function" && mod.protocol_apply_command.length < 2) {
    throw new Error(
      "E_STALE_WASM: protocol_apply_command takes 1 arg -- this wasm pkg predates document-scoped engine routing; rebuild the wasm pkg",
    );
  }
  wasm = mod;
}

// ── Facade readiness (load-order safety) ─────────────────────────────────
// When photrez.facade=1 the facade MUST be Rust-backed. Before the wasm is
// armed (setProtocolWasm) a facade command must NOT silently fall through to
// the TS emulator — the emulator starts at version 0 and is discarded once the
// real engine arms, so an emu->Rust straddle diverges state/version (there is
// no atomic reconciliation). The gate below makes that divergence LOUD under
// flag ON, and only under flag ON. Flag OFF (default) keeps the emulator as the
// legacy authority path, byte-identical to before.
const FACADE_NOT_READY = "E_FACADE_NOT_READY";

// Single source of truth for the facade flag. Reads localStorage directly (not
// via facadeRegistry) to avoid a bridge->facadeRegistry import cycle; facade
// registry re-exports this so existing consumers are unchanged.
export function isFacadeEnabled(): boolean {
  try {
    return typeof localStorage !== "undefined" && localStorage.getItem("photrez.facade") === "1";
  } catch {
    return false;
  }
}

// Native-authority dispatch selection: chooses which engine backs the protocol
// command path. Defaults to wasm so production keeps using the wasm engine; when
// native authority is active the predicate is read by the dispatch branches in
// applyCommand, getSnapshot, getHistoryQuery, and historyCursorCommit.
const FACADE_AUTHORITY_KEY = "photrez.facadeAuthority";

export function isNativeAuthority(): boolean {
  try {
    return typeof localStorage !== "undefined" && localStorage.getItem(FACADE_AUTHORITY_KEY) === "native";
  } catch {
    return false;
  }
}

export function isFacadeArmed(): boolean {
  return wasm !== null;
}

export function facadeReadinessError(): Error {
  return new Error(
    `${FACADE_NOT_READY}: facade protocol is enabled (photrez.facade=1) but the wasm engine is not armed yet; wait for ensureFacadeReady()`,
  );
}

// Awaits the production wasm loader (which arms the bridge via setProtocolWasm
// on resolve) and returns the armed module, or throws E_FACADE_NOT_READY if the
// wasm fails to arm. Idempotent: once armed it returns immediately. Callers
// should await this at app boot (flag ON) so no facade command runs before the
// real Rust engine is wired.
export async function ensureFacadeReady(): Promise<WasmProtocol> {
  if (wasm) return wasm;
  await getWasmExportModule();
  if (!wasm) throw facadeReadinessError();
  return wasm;
}

export function getContractVersion(): number {
  if (wasm) return wasm.protocol_contract_version();
  return CONTRACT_VERSION;
}

// ── Native-authority dispatch (gated, OFF by default) ────────────────────────
// When native authority is active, the protocol command path is rerouted to the
// per-document native ProtocolEngine in the Rust process-global REGISTRY (reached
// via rust_pixels_open_document). Production (native authority off) keeps using
// the wasm engine, byte-identical; no dispatch branch below runs in that case.
// The single authoritative seed promise per doc. Created exactly ONCE, at the
// document-open path (workspace.addDocument) with the REAL layers + starting
// version, so the native engine is never seeded empty. Every bridge command
// awaits this promise instead of seeding itself; a competing empty seed (the
// previous behavior) would clobber the TS model with zero layers and lose data.
const nativeSeedPromiseByDoc = new Map<string, Promise<void>>();

// Native-authority adapter-registration promises (gated, OFF by default).
// registerPayloadAdapter creates one per (doc, adapterId) chained after the
// authoritative seed; applyCommand awaits it so the adapter is registered on the
// native engine BEFORE the command that references it (prevents E_UNKNOWN_ADAPTER).
const nativeAdapterRegByDoc = new Map<string, Promise<void>>();

// Native-authority cutover seed: drop a doc's seed + adapter state on close so
// reopening the SAME id re-seeds the engine with the restored model (a stale
// resolved promise would otherwise skip the re-seed). No-op unless native active.
export function clearNativeSeed(docId: string): void {
  if (!isNativeAuthority()) return;
  const key = docId === "" ? "default" : docId;
  nativeSeedPromiseByDoc.delete(key);
  for (const k of nativeAdapterRegByDoc.keys()) {
    if (k === key || k.startsWith(`${key}::`)) nativeAdapterRegByDoc.delete(k);
  }
}

// Authoritative per-doc seed. Created with REAL layers + version at the
// document-open path before any command runs. Idempotent per doc: the first
// call's (version, layers) win; later calls return the existing promise (the
// Rust seed is only-when-empty as well).
export function createNativeSeed(docId: string, version: number, layers: RenderLayer[]): Promise<void> {
  if (!isNativeAuthority()) return Promise.resolve();
  const key = docId === "" ? "default" : docId;
  const existing = nativeSeedPromiseByDoc.get(key);
  if (existing) return existing;
  const p = (async () => {
    await invoke("rust_pixels_open_document", { docId: key });
    await nativeProtocol.protocol_seed_native(JSON.stringify({ version, layers }), key);
  })();
  nativeSeedPromiseByDoc.set(key, p);
  return p;
}

// Awaits the authoritative seed for a doc. The document-open path is responsible
// for creating it (with real layers) before any command fires; this never seeds
// empty. If no seed exists the open path did not run first - resolve without
// seeding so the subsequent native command surfaces that, never a silent
// zero-layer clobber.
function awaitNativeSeed(docId: string): Promise<void> {
  if (!isNativeAuthority()) return Promise.resolve();
  const key = docId === "" ? "default" : docId;
  return nativeSeedPromiseByDoc.get(key) ?? Promise.resolve();
}

// Seed entry point used by seedFacadeFromEngine. Delegates to the single
// authoritative seed so it can no longer be pre-empted by an empty bridge seed.
export async function ensureNativeEngineSeeded(
  docId: string,
  version: number,
  layers: RenderLayer[],
): Promise<void> {
  if (!isNativeAuthority()) return;
  await createNativeSeed(docId, version, layers);
}

// Native-authority external-transition barrier (ADR 0014): the legacy history
// mirror (recordExternalTransitionFor) bumps the native engine documentVersion
// outside the facade's own command envelope, and it is fire-and-forget. A facade
// command issued while that mirror is still in flight can read a stale
// renderedVersion and be rejected with E_VERSION_MISMATCH (the fill -> setOpacity
// interleave). Track the in-flight mirror so a facade command can await it and
// then read the authoritative version. Gated: only the native path registers and
// flushes; the wasm default path never touches this map, so flush is a no-op
// there and behavior is unchanged.
const externalTransitionPendingByDoc = new Map<string, Promise<void>>();

export function setExternalTransitionPending(docId: string, p: Promise<void>): void {
  if (!isNativeAuthority()) return;
  const key = docId === "" ? "default" : docId;
  const prev = externalTransitionPendingByDoc.get(key);
  externalTransitionPendingByDoc.set(
    key,
    (prev ?? Promise.resolve())
      .then(() => p)
      .catch(() => {}),
  );
}

export async function flushExternalTransitions(docId: string): Promise<void> {
  if (!isNativeAuthority()) return;
  const key = docId === "" ? "default" : docId;
  const p = externalTransitionPendingByDoc.get(key);
  if (p) {
    externalTransitionPendingByDoc.delete(key);
    await p;
  }
}

// Surface a protocol error uniformly as `CODE: message`. The wasm path rejects
// with a JSON `{code,message}` envelope; the native path rejects with the bare
// `"CODE: message"` string (Tauri v2 invoke rejects with that string on a Rust
// Err(String)). Both normalize to the same Error shape, so a rejection surfaces
// consistently, never as an unhandled rejection or a mis-parsed `{ok:false}`.
export function normalizeProtocolError(e: unknown): Error {
  const msg = e instanceof Error ? e.message : String(e);
  try {
    const parsed = JSON.parse(msg) as { code: string; message: string };
    return new Error(`${parsed.code}: ${parsed.message}`);
  } catch {
    return new Error(msg);
  }
}

// Test seam: clears native-authority cutover state so tests covering both the
// default (wasm) and native paths stay isolated. Mirrors __resetEmulatedForTests.
export function __resetNativeAuthorityForTests(): void {
  nativeSeedPromiseByDoc.clear();
  nativeAdapterRegByDoc.clear();
}

export async function applyCommand(envelope: CommandEnvelope): Promise<CommandResult> {
  if (envelope.contractVersion !== CONTRACT_VERSION) {
    throw new Error(
      `E_CONTRACT_VERSION: expected ${CONTRACT_VERSION} got ${envelope.contractVersion}`,
    );
  }
  const rustEnvelope = toRustEnvelope(envelope);
  const json = JSON.stringify(rustEnvelope);
  const docId = envelope.docId ?? "default";
  if (isNativeAuthority()) {
    await awaitNativeSeed(docId);
    // Guarantee the adapter referenced by this command is registered on the
    // native engine before applying. The legacy mirror registers it via
    // registerPayloadAdapter (which resolves this promise after the seed), so
    // awaiting it prevents Rust record_external from rejecting E_UNKNOWN_ADAPTER.
    const key = docId === "" ? "default" : docId;
    const adapterId = (JSON.parse(json) as { command?: { adapter_id?: string } }).command?.adapter_id;
    if (adapterId) {
      const reg = nativeAdapterRegByDoc.get(`${key}::${adapterId}`);
      if (reg) await reg;
    }
    try {
      const outJson = await nativeProtocol.protocol_apply_command_native(json, docId);
      return JSON.parse(outJson) as CommandResult;
    } catch (e) {
      throw normalizeProtocolError(e);
    }
  }
  if (!wasm) {
    // Under photrez.facade=1 the facade must be Rust-backed — never
    // silently emulate a facade command while the wasm is unarmed (that
    // emu->Rust straddle diverges state/version). Under flag OFF (default)
    // the emulator is the legacy authority and behaves exactly as before.
    if (isFacadeEnabled()) {
      throw facadeReadinessError();
    }
    return emulateApply(envelope, docId);
  }
  try {
    const outJson = wasm.protocol_apply_command(json, docId);
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

export async function getSnapshot(docId = "default"): Promise<RenderSnapshot> {
  if (isNativeAuthority()) {
    await awaitNativeSeed(docId);
    try {
      const j = await nativeProtocol.protocol_snapshot_native(docId);
      return JSON.parse(j) as RenderSnapshot;
    } catch (e) {
      throw normalizeProtocolError(e);
    }
  }
  if (!wasm) return { version: 0, layers: [] };
  const j = wasm.protocol_snapshot_json(docId);
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

function emulateApply(env: CommandEnvelope, _docId?: string): CommandResult {
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
