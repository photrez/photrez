// Ticket 1-2 — Typed bridge: CommandEnvelope -> CommandResult{delta} via native JSON transport.
// Control path only (no binary optimization yet). Correctness via Rust expectedVersion check.

import type {
  CommandEnvelope,
  CommandResult,
  RenderLayer,
  RenderSnapshot,
  HistoryQueryResult,
} from "./types";
import { CONTRACT_VERSION } from "./types";
import { pixelInvoke } from "./pixelInvokeCensus";
import { nativeProtocol } from "./nativeClient";

/**
 * Shared pixel-invoke entry. State-changing pixel commands funnel through here
 * so the census registrar is installed before the first pixel invoke and one
 * monotonic order counter covers every call site.
 */
export function invokePixelCommand(command: string, args: Record<string, unknown>): Promise<unknown> {
  return pixelInvoke(command, args);
}

// -- Native dispatch --
// The protocol command path runs on the per-document native DocumentCore in
// the Rust process-global REGISTRY (reached via rust_pixels_open_document).
// The single authoritative seed promise per doc. Created exactly ONCE, at the
// document-open path (workspace.addDocument) with the REAL layers + starting
// version, so the native engine is never seeded empty. Every bridge command
// awaits this promise instead of seeding itself; a competing empty seed (the
// previous behavior) would clobber the TS model with zero layers and lose data.
export const nativeSeedPromiseByDoc = new Map<string, Promise<void>>();

// Native-authority adapter-registration promises (skipped under the wasm opt-out).
// registerPayloadAdapter creates one per (doc, adapterId) chained after the
// authoritative seed; applyCommand awaits it so the adapter is registered on the
// native engine BEFORE the command that references it (prevents E_UNKNOWN_ADAPTER).
export const nativeAdapterRegByDoc = new Map<string, Promise<void>>();

// Native-authority cutover seed: drop a doc's seed + adapter state on close so
// reopening the SAME id re-seeds the engine with the restored model (a stale
// resolved promise would otherwise skip the re-seed).
export function clearNativeSeed(docId: string): void {
  const key = docId === "" ? "default" : docId;
  nativeSeedPromiseByDoc.delete(key);
  for (const k of nativeAdapterRegByDoc.keys()) {
    if (k === key || k.startsWith(`${key}::`)) nativeAdapterRegByDoc.delete(k);
  }
  // Mirror the layer/adapter eviction: a stale canonical-seed promise would otherwise
  // skip the re-seed on reopen and keep a divergent shadow alive.
  nativeCanonicalSeedPromiseByDoc.delete(key);
  canonicalPendingByDoc.delete(key);
  // Evict the payload record with the slot: otherwise a reopened doc id would
  // skip its re-seed as a "duplicate" of the pre-close content.
  canonicalPayloadByDoc.delete(key);
}

// Authoritative per-doc seed. Created with REAL layers + version at the
// document-open path before any command runs. Idempotent per doc: the first
// call's (version, layers) win; later calls return the existing promise (the
// Rust seed is only-when-empty as well).
export function createNativeSeed(docId: string, version: number, layers: RenderLayer[]): Promise<void> {
  const key = docId === "" ? "default" : docId;
  const existing = nativeSeedPromiseByDoc.get(key);
  if (existing) return existing;
  const p = (async () => {
    await invokePixelCommand("rust_pixels_open_document", { docId: key });
    await nativeProtocol.protocol_seed_native(JSON.stringify({ version, layers }), key);
  })();
  nativeSeedPromiseByDoc.set(key, p);
  // A REFUSED seed must not stay recorded as this document's seed. The entry is
  // what every routed command awaits (`awaitNativeSeed`), and a rejected entry is
  // handed to every later caller - so a document opened while the seed was
  // refused would reject every routed command for the rest of the session, with
  // no recovery except closing it. Dropping the entry on rejection returns the
  // document to the state `awaitNativeSeed` already documents for a doc whose
  // open path never ran: resolve without seeding, so the command surfaces the
  // problem instead of inheriting a rejection from the open. Same instrument
  // closing a document uses, scoped to this document id, and guarded by identity
  // so an earlier attempt's late rejection cannot evict a later seed's entry.
  void p.catch(() => {
    if (nativeSeedPromiseByDoc.get(key) === p) nativeSeedPromiseByDoc.delete(key);
  });
  return p;
}

// Native-authority canonical-document seed promises (skipped under the wasm opt-out).
// Like nativeSeedPromiseByDoc but for the full canonical-document shadow seed
// (seedNativeCanonical). applyCommand awaits BOTH the layer seed and this seed,
// so a facade command cannot apply natively before the canonical open-seed lands
// and (if it arrived late) overwrite doc_size with open-time dims.
export const nativeCanonicalSeedPromiseByDoc = new Map<string, Promise<void>>();

// Seed the full canonical-document shadow into the native engine for a doc. Runs
// AFTER the authoritative open-path layer seed (it awaits that seed's promise) so
// the doc is already open when the canonical copy is pushed. If the open-path
// seed was never created, we still
// invoke so the command surfaces the missing-doc ordering bug rather than silently
// skipping a seed. The seed's promise is recorded in nativeCanonicalSeedPromiseByDoc
// so applyCommand's seed barrier can await it.
export function seedNativeCanonical(docId: string, canonicalJson: string): Promise<void> {
  const key = docId === "" ? "default" : docId;
  const p = (async () => {
    await (nativeSeedPromiseByDoc.get(key) ?? Promise.resolve());
    await nativeProtocol.protocol_seed_canonical_native(canonicalJson, key);
  })();
  // Store a GUARDED copy. This slot is an ORDERING barrier, not an error channel:
  // its one job is to make awaitNativeSeed wait for the push. The push's own
  // failure is reported to whoever awaited the returned `p`, and storing the raw
  // promise here as well would make a failed push reject a second time with no
  // handler - an unhandled rejection that fails a test run for a push the caller
  // already saw fail. Measured while adding the pre-op capture push: a document
  // with no reachable transport rejected here, the barrier rejected in
  // awaitNativeSeed, the applyCommand that would have awaited the adapter
  // registration bailed, and the adapter promise surfaced unhandled.
  nativeCanonicalSeedPromiseByDoc.set(key, p.then(() => {}).catch(() => {}));
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
  await createNativeSeed(docId, version, layers);
}

// Native-authority external-transition barrier (ADR 0014): the legacy history
// mirror (recordExternalTransitionFor) bumps the native engine documentVersion
// outside the facade's own command envelope, and it is fire-and-forget. A facade
// command issued while that mirror is still in flight can read a stale
// renderedVersion and be rejected with E_VERSION_MISMATCH (the fill -> setOpacity
// interleave). Track the in-flight mirror so a facade command can await it and
// then read the authoritative version. Only the native path registers and
// flushes.
const externalTransitionPendingByDoc = new Map<string, Promise<void>>();

export function setExternalTransitionPending(docId: string, p: Promise<void>): void {
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
  const key = docId === "" ? "default" : docId;
  const p = externalTransitionPendingByDoc.get(key);
  if (p) {
    externalTransitionPendingByDoc.delete(key);
    await p;
  }
  // Drain the coalesced canonical slot the same way: take the latest re-push,
  // clear the slot, then await it, so the next facade command (via
  // syncFromEngine) still dispatches only after the shadow is complete.
  // Fixed order: version mirrors drain first, then the re-push, so the shadow reflects every bumped version.
  const canon = canonicalPendingByDoc.get(key);
  if (canon) {
    canonicalPendingByDoc.delete(key);
    await canon;
  }
}

// Coalesced canonical re-push slot (last-writer-wins). ONLY repushCanonicalDocument
// writes here; external-mirror version entries stay on the chained barrier above
// because each mirror bumps the engine version and every bump must run.
export const canonicalPendingByDoc = new Map<string, Promise<void>>();

// Last pushed canonical payload per doc. Backs the payload-compare guard in
// repushCanonicalDocument: a byte-identical re-push is a proven duplicate.
// Owned here (not in canonicalSeed) so workspace-close eviction clears it next
// to the slot, and a reopened doc id always re-seeds instead of skipping.
export const canonicalPayloadByDoc = new Map<string, string>();

// Overwrites, never chains: a newer re-push supersedes the pending one because
// every payload is the FULL document, so the latest alone heals the shadow.
// Mirror/version entries must NEVER use this slot (see the map comment above).
export function setCanonicalPending(docId: string, p: Promise<void>): void {
  const key = docId === "" ? "default" : docId;
  // Store a guarded copy: a rejected re-push must never wedge the flush (same
  // stance as the chained barrier). The original still rejects for the
  // re-push's own awaiter, which logs or drains it at the call site.
  canonicalPendingByDoc.set(key, p.then(() => {}).catch(() => {}));
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
// wasm opt-out and native paths stay isolated. Mirrors __resetEmulatedForTests.
export function __resetNativeAuthorityForTests(): void {
  nativeSeedPromiseByDoc.clear();
  nativeAdapterRegByDoc.clear();
  nativeCanonicalSeedPromiseByDoc.clear();
  canonicalPendingByDoc.clear();
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

export async function getSnapshot(docId = "default"): Promise<RenderSnapshot> {
  await awaitNativeSeed(docId);
  try {
    const j = await nativeProtocol.protocol_snapshot_native(docId);
    return JSON.parse(j) as RenderSnapshot;
  } catch (e) {
    throw normalizeProtocolError(e);
  }
}

// Lightweight native version read (carry-forward optimization): the facade's
// syncFromEngine path only needs the engine's u64 version, never the full
// snapshot. Read it directly via protocol_version_native (no snapshot
// serialization).
export async function getVersion(docId = "default"): Promise<number> {
  await awaitNativeSeed(docId);
  try {
    return await nativeProtocol.protocol_version_native(docId);
  } catch (e) {
    throw normalizeProtocolError(e);
  }
}

// Id-only membership probe: the guarded metadata funnels need the engine
// layer-id set, never full layer values. Read it directly via
// protocol_layer_ids_native (no snapshot serialization). A backend that does
// not register the command rejects the invoke, and the probe falls back to the
// full snapshot, so behavior is unchanged there. The fallback also preserves
// the missing-doc error: the snapshot rejects the same way the probe would,
// so no error is masked.
export async function getLayerIds(docId = "default"): Promise<string[]> {
  await awaitNativeSeed(docId);
  try {
    const j = await nativeProtocol.protocol_layer_ids_native(docId);
    return JSON.parse(j) as string[];
  } catch {
    const snap = await getSnapshot(docId);
    return (snap.layers ?? []).map((l) => l.id);
  }
}

// Exported so the wire mapping can be tested directly. This function is where a
// field gets DROPPED: it builds a loosely-typed object that is then stringified
// and shipped, and the exhaustiveness guard below checks the variant TYPE, not
// its fields - so a field the mapping forgets vanishes with no type error, no
// runtime error, and (on the Rust side) no error at all, because `#[serde(default)]`
// turns the absent key into None. Testing through the type-checker cannot see it.
export function toRustEnvelope(env: CommandEnvelope): unknown {
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
      // The command enum's VARIANT names are camelCase (serde rename_all) but its
      // FIELD names are not renamed - hence the snake_case keys below, matching
      // `affected_layer_ids` / `memory_cost_bytes`.
      //
      // doc_size_before / doc_size_after carry the HOST's document-size halves
      // (`Option<(f64, f64)>`, i.e. a [width, height] array or null). They must be
      // mapped here: the envelope is stringified and shipped as-is, and Rust's
      // `#[serde(default)]` turns an ABSENT key into None with no error and no
      // warning - so dropping them silently disarms the crop undo instead of
      // failing. Rust's emit rule needs BOTH halves present and differing, so a
      // size-neutral transition's `null` pair means "changes no size".
      rustCmd = {
        type: "recordExternalTransition",
        label: c.label,
        affected_layer_ids: c.affectedLayerIds,
        adapter_id: c.adapterId,
        token: c.token,
        memory_cost_bytes: c.memoryCostBytes,
        doc_size_before: c.docSizeBefore ?? null,
        doc_size_after: c.docSizeAfter ?? null,
        // Ids this transition minted. Mapped here for the same reason as the size
        // halves: an ABSENT key is swallowed by Rust's `#[serde(default)]` with no
        // error, so dropping it silently disarms the merge-destination undo.
        minted_layer_ids: c.mintedLayerIds ?? [],
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


// The in-memory emulator lives in bridge_emu.ts to keep this module under the
// 1000-line guard. It is test-only instrumentation now: production dispatches to
// the native engine above. Every emulator symbol is re-exported so existing
// import paths stay valid.
export * from "./bridge_emu";
