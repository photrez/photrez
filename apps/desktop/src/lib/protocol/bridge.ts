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

export let wasm: WasmProtocol | null = null;

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
export const nativeSeedPromiseByDoc = new Map<string, Promise<void>>();

// Native-authority adapter-registration promises (gated, OFF by default).
// registerPayloadAdapter creates one per (doc, adapterId) chained after the
// authoritative seed; applyCommand awaits it so the adapter is registered on the
// native engine BEFORE the command that references it (prevents E_UNKNOWN_ADAPTER).
export const nativeAdapterRegByDoc = new Map<string, Promise<void>>();

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
  // Mirror the layer/adapter eviction: a stale canonical-seed promise would otherwise
  // skip the re-seed on reopen and keep a divergent shadow alive.
  nativeCanonicalSeedPromiseByDoc.delete(key);
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

// Native-authority canonical-document seed promises (gated, OFF by default).
// Like nativeSeedPromiseByDoc but for the full canonical-document shadow seed
// (seedNativeCanonical). applyCommand awaits BOTH the layer seed and this seed,
// so a facade command cannot apply natively before the canonical open-seed lands
// and (if it arrived late) overwrite doc_size with open-time dims.
export const nativeCanonicalSeedPromiseByDoc = new Map<string, Promise<void>>();

// Seed the full canonical-document shadow into the native engine for a doc. Runs
// AFTER the authoritative open-path layer seed (it awaits that seed's promise) so
// the doc is already open when the canonical copy is pushed. Gated: a no-op under
// the default (wasm) authority. If the open-path seed was never created, we still
// invoke so the command surfaces the missing-doc ordering bug rather than silently
// skipping a seed. The seed's promise is recorded in nativeCanonicalSeedPromiseByDoc
// so applyCommand's seed barrier can await it.
export function seedNativeCanonical(docId: string, canonicalJson: string): Promise<void> {
  if (!isNativeAuthority()) return Promise.resolve();
  const key = docId === "" ? "default" : docId;
  const p = (async () => {
    await (nativeSeedPromiseByDoc.get(key) ?? Promise.resolve());
    await nativeProtocol.protocol_seed_canonical_native(canonicalJson, key);
  })();
  nativeCanonicalSeedPromiseByDoc.set(key, p);
  return p;
}

// Awaits the authoritative seed for a doc. The document-open path is responsible
// for creating it (with real layers) before any command fires; this never seeds
// empty. If no seed exists the open path did not run first - resolve without
// seeding so the subsequent native command surfaces that, never a silent
// zero-layer clobber. Awaits BOTH the layer seed AND the canonical-document seed,
// so a late canonical open-seed can never overwrite doc_size after a command has
// already applied natively.
export async function awaitNativeSeed(docId: string): Promise<void> {
  if (!isNativeAuthority()) return Promise.resolve();
  const key = docId === "" ? "default" : docId;
  const layer = nativeSeedPromiseByDoc.get(key) ?? Promise.resolve();
  const canonical = nativeCanonicalSeedPromiseByDoc.get(key) ?? Promise.resolve();
  await Promise.all([layer, canonical]);
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
  nativeCanonicalSeedPromiseByDoc.clear();
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
  if (!wasm) return emulateGetSnapshot();
  const j = wasm.protocol_snapshot_json(docId);
  return JSON.parse(j) as RenderSnapshot;
}

// Lightweight native-authority version read (carry-forward optimization): the
// facade's syncFromEngine path only needs the engine's u64 version, never the
// full snapshot. Under native authority read it directly via
// protocol_version_native (no snapshot serialization); fall back to parsing the
// snapshot's version on the wasm path as a defensive fallback (the only
// production caller, syncFromEngine, is native-gated, so the wasm branch is a
// safety net that is rarely hit).
export async function getVersion(docId = "default"): Promise<number> {
  if (isNativeAuthority()) {
    await awaitNativeSeed(docId);
    try {
      return await nativeProtocol.protocol_version_native(docId);
    } catch (e) {
      throw normalizeProtocolError(e);
    }
  }
  if (!wasm) return 0;
  const j = wasm.protocol_snapshot_json(docId);
  return (JSON.parse(j) as RenderSnapshot).version;
}

function toRustEnvelope(env: CommandEnvelope): unknown {
  const c = env.command;
  let rustCmd: unknown;
  switch (c.type) {
    case "noop":
      rustCmd = { type: "noop" };
      break;
    case "ping":
      rustCmd = { type: "ping", echo: c.echo };
      break;
    case "addLayer":
      rustCmd = {
        type: "addLayer",
        id: c.id,
        name: c.name,
        width: c.width,
        height: c.height,
        index: c.index,
        // Command variant fields serialize snake_case on the wire (the enum
        // rename_all only renames variant names, not their fields — matching the
        // existing brushStroke layer_id convention). Translate the TS camelCase
        // command fields to the wire keys.
        layer_type: c.layerType,
        shape_params: c.shapeParams,
        text_data: c.textData,
      };
      break;
    case "deleteLayer":
      rustCmd = { type: "deleteLayer", id: c.id };
      break;
    case "transformLayer":
      rustCmd = { type: "transformLayer", id: c.id, transform: c.transform };
      break;
    case "setVisible":
      rustCmd = { type: "setVisible", id: c.id, visible: c.visible };
      break;
    case "setLocked":
      rustCmd = { type: "setLocked", id: c.id, kind: c.kind, locked: c.locked };
      break;
    case "rename":
      rustCmd = { type: "rename", id: c.id, name: c.name };
      break;
    case "reorder":
      rustCmd = { type: "reorder", id: c.id, to: c.to };
      break;
    case "setBackgroundFlag":
      rustCmd = { type: "setBackgroundFlag", id: c.id };
      break;
    case "setBlendMode":
      rustCmd = { type: "setBlendMode", id: c.id, mode: c.mode };
      break;
    case "setLayerParams":
      rustCmd = {
        type: "setLayerParams",
        id: c.id,
        shape_params: c.shapeParams,
        text_data: c.textData,
      };
      break;
    case "setAdjustment":
      rustCmd = { type: "setAdjustment", id: c.id, adjustment: c.adjustment };
      break;
    case "setOpacity":
      rustCmd = { type: "setOpacity", id: c.id, opacity: c.opacity };
      break;
    case "brushStroke":
      rustCmd = { type: "brushStroke", layer_id: c.layerId, points: c.points, settings: c.settings };
      break;
    case "undo":
      rustCmd = { type: "undo" };
      break;
    case "redo":
      rustCmd = { type: "redo" };
      break;
    case "recordExternalTransition":
      rustCmd = {
        type: "recordExternalTransition",
        label: c.label,
        affected_layer_ids: c.affectedLayerIds,
        adapter_id: c.adapterId,
        token: c.token,
        memory_cost_bytes: c.memoryCostBytes,
      };
      break;
    // Selection arms: selection is engine-local; the nested SelectionState is a
    // camelCase struct (its serde rename_all matches the TS field names), so it
    // passes through unchanged. No snake_case field remap needed.
    case "setSelection":
      rustCmd = { type: "setSelection", selection: c.selection };
      break;
    case "clearSelection":
      rustCmd = { type: "clearSelection" };
      break;
    case "selectAll":
      rustCmd = { type: "selectAll" };
      break;
    case "invertSelection":
      rustCmd = { type: "invertSelection" };
      break;
    // Structural arms: the wasm command enum is camelCase (serde rename_all), so
    // the variant type stays camelCase; only the nested fields serialize snake_case
    // on the wire (enum rename_all does not touch field names), so translate the TS
    // camelCase fields to the wire keys.
    case "duplicateLayer":
      rustCmd = { type: "duplicateLayer", id: c.id, new_id: c.newId };
      break;
    case "mergeDown":
      rustCmd = { type: "mergeDown", id: c.id, merged_id: c.mergedId };
      break;
    case "mergeSelected":
      rustCmd = { type: "mergeSelected", ids: c.ids, merged_id: c.mergedId };
      break;
    case "flatten":
      rustCmd = { type: "flatten", merged_id: c.mergedId };
      break;
    case "rasterizeLayer":
      rustCmd = { type: "rasterizeLayer", id: c.id };
      break;
    // Canvas-size arms: the wasm command enum is camelCase (serde rename_all),
    // so the variant type stays camelCase; only the optional nested fields
    // serialize snake_case on the wire (matching target_width/target_height on the
    // Rust ApplyCrop variant), so translate the TS camelCase command fields.
    case "cropCanvas":
      rustCmd = { type: "cropCanvas", x: c.x, y: c.y, width: c.width, height: c.height };
      break;
    case "applyCrop":
      rustCmd = {
        type: "applyCrop",
        x: c.x,
        y: c.y,
        width: c.width,
        height: c.height,
        rotation: c.rotation,
        target_width: c.targetWidth,
        target_height: c.targetHeight,
      };
      break;
    case "resizeCanvas":
      rustCmd = { type: "resizeCanvas", width: c.width, height: c.height };
      break;
    default: {
      // Exhaustiveness guard: every Command variant is handled above. A new
      // variant that forgets its wire mapping fails the type-check here instead
      // of silently dropping a mutation (data-loss class).
      const _exhaustive: never = c;
      throw new Error(`E_UNKNOWN_COMMAND: ${JSON.stringify(_exhaustive)}`);
    }
  }
  return { contractVersion: env.contractVersion, expectedVersion: env.expectedVersion, command: rustCmd };
}


// The in-memory emulator now lives in bridge_emu.ts to keep this module under the
// 1000-line guard. applyCommand dispatches to it when the wasm runtime is unarmed;
// every emulator symbol is re-exported so existing import paths stay valid.
import { emulateApply, emulateGetSnapshot } from "./bridge_emu";
export * from "./bridge_emu";
