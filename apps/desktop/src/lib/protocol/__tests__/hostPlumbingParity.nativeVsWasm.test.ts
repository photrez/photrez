// Host-plumbing parity: the native-authority dispatch arm vs the wasm-authority
// dispatch arm of `bridge.applyCommand`.
//
// WHY THIS TEST IS WORTH HAVING
//
// The two arms share ONE engine implementation (a single command-envelope
// apply reached from both transports), so comparing engine state across the
// arms cannot detect an engine bug. What CAN diverge is
// the host plumbing that runs around the engine, and that is what this file
// compares:
//
//   * dispatch selection          bridge.ts:284-316
//   * docId resolution            bridge.ts:286 + nativeClient.ts:37-39
//   * the seeding barrier         bridge.ts:200-206, awaited at :285
//   * the adapter barrier         bridge.ts:287-295
//   * error normalization         bridge.ts:257-268
//   * response parsing            bridge.ts:298, :315
//
// Both arms are driven against the REAL Rust engine, so no part of the engine's
// behaviour is faked. The native arm reaches it through a Tauri-transport shim
// that translates each `invoke` name into its real wasm export and converts the
// wasm rejection shape into the bare `"CODE: message"` string the native command
// produces (protocol_native_cmds.rs:59) - which is also what Tauri v2 `invoke()`
// rejects with. The shim translates each invoke name to its real wasm export;
// the single command with no wasm counterpart is documented in THE GAP below.
//
// THE GAP, STATED UP FRONT: the wasm build exports no layer/version seed, so
// `protocol_seed_native` has no counterpart to compare against. Both arms are
// therefore seeded through the one seeding channel both surfaces DO share,
// `protocol_seed_canonical` / `protocol_seed_canonical_native`
// (crates/core/src/document_core.rs:173 up-projects the pushed layer vector into
// the engine's LayerSet). The seed's ORDERING contract stays covered by
// nativeAuthorityReroute.test.ts. Seed payload semantics are NOT covered here.
//
// THE PRECONDITION, STATED JUST AS PLAINLY: this file compares the two arms FROM A
// COMMON PRECONDITION, and production does not hand them one. Production seeds the
// native engine at open with the real layers + the live version (workspace.ts:55-75,
// facadeRegistry.ts:684-712) and seeds the wasm engine with nothing (every seed call
// site in src/ is isNativeAuthority()-gated). So this file proves "given the same
// starting state, the host plumbing is parity". It does NOT prove "the two authority
// modes start their engines in the same state" - that difference is the design, not
// a defect, and it is outside what this file can measure. Seeding-strategy
// divergence is therefore NOT covered here either.

import { describe, it, expect, vi, beforeAll, beforeEach, afterEach, type Mock } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { getWasmExportModule } from "@/components/editor/wasmExport";
import {
  applyCommand,
  getSnapshot,
  getVersion,
  createNativeSeed,
  seedNativeCanonical,
  setProtocolWasm,
  __resetNativeAuthorityForTests,
} from "../bridge";
import { CONTRACT_VERSION, type Command, type RenderSnapshot } from "../types";
import { WorkspaceManager } from "@/engine/workspace";
import { seedFacadeFromEngine, getFacade, __resetFacadeRegistryForTests } from "../facadeRegistry";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const invokeMock = invoke as unknown as Mock<
  (cmd: string, args: Record<string, unknown>) => Promise<unknown>
>;

// The native engine lives in a separate process registry, so the shim parks its
// per-doc engines in a distinct namespace. Sharing one namespace with the wasm
// arm would compare one engine against itself.
const NATIVE_NS = "native::";

const NATIVE_DOC = "plumbing-parity-native";
const WASM_DOC = "plumbing-parity-wasm";
const EMPTY_DOC = "";

type WasmModule = {
  protocol_contract_version: () => number;
  protocol_reset: (docId: string) => void;
  protocol_apply_command: (json: string, docId: string) => string;
  protocol_snapshot_json: (docId: string) => string;
  protocol_version: (docId: string) => bigint;
  protocol_history_query_json: (docId: string) => string;
  protocol_history_cursor_commit: (json: string, docId: string) => string;
  protocol_register_payload_adapter: (adapterId: string, docId: string) => void;
  protocol_seed_canonical: (json: string, docId: string) => string;
};

let wasmMod: WasmModule;
let nativeSeedInvocations: string[] = [];

// The wasm bridge rejects with a JSON `{code,message}` string
// (document_core.rs:711); the native command rejects with the bare
// `"CODE: message"` string (protocol_native_cmds.rs:59). Translate the first
// into the second so the host sees the same rejection shape it sees in
// production.
function nativeRejection(e: unknown): string {
  const raw = e instanceof Error ? e.message : String(e);
  try {
    const parsed = JSON.parse(raw) as { code?: unknown; message?: unknown };
    if (typeof parsed?.code === "string" && typeof parsed?.message === "string") {
      return `${parsed.code}: ${parsed.message}`;
    }
  } catch {
    // Not a JSON envelope: surface the raw value.
  }
  return raw;
}

function installNativeTransport(): void {
  nativeSeedInvocations = [];
  // Docs this transport has already "opened". Kept per install (so each test
  // starts clean) to model the real command's IDEMPOTENCE.
  const openedDocs = new Set<string>();
  invokeMock.mockImplementation(async (cmd: string, args: Record<string, unknown> = {}) => {
    // Faithful to the Rust side: EVERY native command resolves an empty doc_id to
    // the reserved "default" key before it touches the registry
    // (protocol_native_cmds.rs:25-31, applied at :48). The wasm bridge does the
    // same (document_core.rs:675-681, applied at :705). Modelling it here matters:
    // omitting it would make the host's OWN normalization load-bearing, which it
    // is not, and would manufacture a divergence production cannot have.
    const rawDocId = (args.docId as string) ?? "";
    const docId = rawDocId === "" ? "default" : rawDocId;
    const key = `${NATIVE_NS}${docId}`;
    switch (cmd) {
      case "rust_pixels_open_document":
        // IDEMPOTENT, like production: `pixel_store.rs:327-329`
        // (`self.docs.entry(doc_id).or_default()`) KEEPS an already-open doc's
        // engine; it does not replace it. An earlier version of this shim mapped
        // the command to `protocol_reset` unconditionally, which is more
        // destructive than reality: `workspace.notifyRustPixelDoc` fires the open
        // through a dynamic `import()` (workspace.ts:33-35), so a DUPLICATE open
        // can land AFTER the seeds and wipe the engine the seeds just populated.
        // Production cannot lose state that way - so the shim must not either.
        if (!openedDocs.has(key)) {
          openedDocs.add(key);
          wasmMod.protocol_reset(key);
        }
        return undefined;
      case "protocol_seed_native":
        // NO wasm counterpart (see the module header). Recorded so the ordering
        // barrier can be asserted; the layer state arrives through the canonical
        // seed below, exactly as workspace.ts pushes it in production.
        nativeSeedInvocations.push(docId);
        return JSON.stringify({ version: 0, layers: [] });
      case "protocol_seed_canonical_native":
        return wasmMod.protocol_seed_canonical(args.payloadJson as string, key);
      case "protocol_apply_command_native":
        try {
          return wasmMod.protocol_apply_command(args.envelopeJson as string, key);
        } catch (e) {
          throw nativeRejection(e);
        }
      case "protocol_snapshot_native":
        return wasmMod.protocol_snapshot_json(key);
      case "protocol_version_native":
        return Number(wasmMod.protocol_version(key));
      case "protocol_history_query_native":
        return wasmMod.protocol_history_query_json(key);
      case "protocol_history_cursor_commit_native":
        try {
          return wasmMod.protocol_history_cursor_commit(
            JSON.stringify({ seq: args.seq, direction: args.direction }),
            key,
          );
        } catch (e) {
          throw nativeRejection(e);
        }
      case "protocol_register_adapter_native":
        wasmMod.protocol_register_payload_adapter(args.adapterId as string, key);
        return null;
      case "protocol_canonical_native":
        // Native-only read-back: the wasm build exports no counterpart, so this
        // surface is deliberately not compared (report Part 2).
        throw `E_NOT_BACKED: ${cmd}`;
      default:
        // An unrouted command name must surface, never resolve undefined.
        throw `E_UNKNOWN_COMMAND: ${cmd}`;
    }
  });
}

// A three-layer document. Both arms are seeded from this same canonical payload
// through the shared canonical channel, so the comparison starts from a common
// precondition rather than from two different seeds.
const CANONICAL_JSON = JSON.stringify({
  id: "plumbing-parity",
  name: "Plumbing",
  width: 800,
  height: 600,
  layers: [
    {
      id: "l-top",
      name: "Top",
      type: "raster",
      visible: true,
      opacity: 1,
      locked: false,
      blendMode: "normal",
      transform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
      width: 300,
      height: 200,
    },
    {
      id: "l-mid",
      name: "Mid",
      type: "raster",
      visible: true,
      opacity: 1,
      locked: false,
      blendMode: "normal",
      transform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
      width: 200,
      height: 150,
    },
    {
      id: "l-bot",
      name: "Bot",
      type: "raster",
      visible: true,
      opacity: 1,
      locked: false,
      blendMode: "normal",
      transform: { x: 0, y: 0, scaleX: 1, scaleY: 1, rotation: 0, flipH: false, flipV: false },
      width: 200,
      height: 150,
    },
  ],
});

// Deterministic scripted sequence over the routed command set, with undo/redo
// walk-backs interleaved. Fixed ids and fixed values, so a divergence is
// reproducible.
const SCRIPT: ReadonlyArray<{ label: string; command: Command }> = [
  { label: "addLayer n1", command: { type: "addLayer", id: "n1", name: "New", width: 100, height: 80, index: 0 } },
  { label: "setOpacity n1 0.5", command: { type: "setOpacity", id: "n1", opacity: 0.5 } },
  { label: "undo", command: { type: "undo" } },
  { label: "redo", command: { type: "redo" } },
  { label: "rename n1", command: { type: "rename", id: "n1", name: "Renamed" } },
  { label: "setVisible n1 false", command: { type: "setVisible", id: "n1", visible: false } },
  { label: "setLocked n1", command: { type: "setLocked", id: "n1", kind: "base", locked: true } },
  { label: "setBlendMode n1 multiply", command: { type: "setBlendMode", id: "n1", mode: "multiply" } },
  { label: "reorder n1 to 1", command: { type: "reorder", id: "n1", to: 1 } },
  { label: "undo", command: { type: "undo" } },
  { label: "duplicateLayer n1", command: { type: "duplicateLayer", id: "n1", newId: "n1-dup" } },
  { label: "deleteLayer n1-dup", command: { type: "deleteLayer", id: "n1-dup" } },
  { label: "setSelection", command: { type: "setSelection", selection: { x: 1, y: 2, width: 50, height: 40, angle: 0, shape: "rect", inverted: false } } },
  { label: "clearSelection", command: { type: "clearSelection" } },
  { label: "selectAll", command: { type: "selectAll" } },
  { label: "invertSelection", command: { type: "invertSelection" } },
  { label: "undo", command: { type: "undo" } },
  { label: "redo", command: { type: "redo" } },
  { label: "setBackgroundFlag l-bot", command: { type: "setBackgroundFlag", id: "l-bot" } },
  { label: "transformLayer l-mid", command: { type: "transformLayer", id: "l-mid", transform: { x: 10, y: 20, scaleX: 2, scaleY: 3, rotation: 45, flipH: true, flipV: false } } },
];

// Canonical projection of an observed snapshot. `dirtyRect` is a transient render
// hint (the Rust digest excludes it for the same reason); everything else is
// compared, including `version` and `resourceId`, because a version or resource
// desync between the arms is exactly the class of host bug this test exists for.
//
// `includeVersion` is false for exactly one caller: the non-degeneracy guard. The
// engine bumps `version` on every ACCEPTED command - the selection arms and a
// no-op undo included (document_core_apply.rs:916, :735) - so counting distinct
// version-INCLUSIVE digests would only re-measure "the commands were accepted",
// which the guard already asserts. Excluding `version` makes the guard measure
// what it claims: that the steps moved layer/selection/dims state.
function projectionOf(snapshot: RenderSnapshot, includeVersion: boolean): string {
  const layers = (snapshot.layers ?? []).map((layer) => {
    const rec = layer as unknown as Record<string, unknown>;
    const out: Record<string, unknown> = {};
    for (const key of Object.keys(rec).sort()) {
      if (key === "dirtyRect") continue;
      out[key] = rec[key];
    }
    return out;
  });
  const out: Record<string, unknown> = {
    layers,
    selection: snapshot.selection ?? null,
    width: snapshot.width ?? null,
    height: snapshot.height ?? null,
  };
  if (includeVersion) out.version = snapshot.version;
  return JSON.stringify(out);
}

// One step's observation, in two projections: `digest` (version-inclusive) is what
// the arms are compared on; `state` (version-free) is what the non-degeneracy
// guard measures. A rejected step records the rejection in BOTH, so an arm that
// rejects what the other accepts is reported at that step.
type Observation = { digest: string; state: string };

function useNativeAuthority(on: boolean): void {
  if (on) localStorage.setItem("photrez.facadeAuthority", "native");
  else localStorage.removeItem("photrez.facadeAuthority");
}

/// Seed the native arm through the production host path: the authoritative
/// layer seed (barrier only - no wasm counterpart) followed by the canonical
/// push that the real wasm module applies.
async function seedNativeArm(docId: string): Promise<void> {
  useNativeAuthority(true);
  await createNativeSeed(docId, 0, []);
  await seedNativeCanonical(docId, CANONICAL_JSON);
}

/// Seed the wasm arm's engine through the same canonical channel. Production
/// under wasm authority never seeds the wasm engine (canonicalSeed.ts: the
/// default path never calls the seed), so this is test setup that gives the
/// comparison a common precondition - it is not a production behaviour claim.
function seedWasmArm(docId: string): void {
  wasmMod.protocol_seed_canonical(CANONICAL_JSON, docId);
}

/// Drive the scripted sequence through `applyCommand` on one arm and record the
/// post-step observation. `expectedVersion` is read from that arm's own engine,
/// so a version desync surfaces as a divergence rather than as a spurious
/// rejection. A rejection is recorded at its step (rather than thrown) so an
/// arm that REJECTS a command the other arm accepts is reported as a divergence
/// at that step, with both observations, instead of as a bare stack trace.
async function runArm(docId: string, native: boolean): Promise<Observation[]> {
  useNativeAuthority(native);
  const observed: Observation[] = [];
  for (const step of SCRIPT) {
    try {
      const expectedVersion = await getVersion(docId);
      await applyCommand({
        contractVersion: CONTRACT_VERSION,
        expectedVersion,
        docId,
        command: step.command,
      });
      const snapshot = await getSnapshot(docId);
      observed.push({
        digest: projectionOf(snapshot, true),
        state: projectionOf(snapshot, false),
      });
    } catch (e) {
      const rejected = `<rejected: ${e instanceof Error ? e.message : String(e)}>`;
      observed.push({ digest: rejected, state: rejected });
    }
  }
  return observed;
}

function expectStepParity(nativeObserved: Observation[], wasmObserved: Observation[]): void {
  expect(nativeObserved.length).toBe(SCRIPT.length);
  expect(wasmObserved.length).toBe(SCRIPT.length);
  const wasmStates = wasmObserved.filter((o) => !o.state.startsWith("<rejected:"));
  expect(wasmStates.length).toBe(SCRIPT.length);

  // Non-degeneracy, on the VERSION-FREE projection (see projectionOf). Two
  // independent guards, because a single one is easy to satisfy vacuously:
  //
  //  1. No step may leave the state unchanged. A script of accepted no-ops would
  //     leave every state identical and redden on the first step. Walk-backs
  //     revisit EARLIER states, which is legitimate - only a step that changes
  //     nothing is a failure.
  //  2. The states must not collapse onto a tiny cycle: more than half the steps
  //     must produce a state never seen before. Measured at the time of writing:
  //     14 distinct states across the 20 steps (the undo/redo walk-backs revisit
  //     earlier states by design).
  for (let i = 1; i < wasmStates.length; i++) {
    expect(
      wasmStates[i].state,
      `step ${i + 1}/${SCRIPT.length} (${SCRIPT[i].label}) did not move state, so the parity comparison is vacuous at that step`,
    ).not.toBe(wasmStates[i - 1].state);
  }
  const distinctStates = new Set(wasmStates.map((o) => o.state)).size;
  expect(
    distinctStates * 2,
    `only ${distinctStates} distinct states across ${wasmStates.length} steps`,
  ).toBeGreaterThan(wasmStates.length);

  for (let i = 0; i < SCRIPT.length; i++) {
    expect(
      nativeObserved[i].digest,
      `host plumbing diverged at step ${i + 1}/${SCRIPT.length} (${SCRIPT[i].label})`,
    ).toBe(wasmObserved[i].digest);
  }
}

beforeAll(async () => {
  const mod = (await getWasmExportModule()) as WasmModule | null;
  expect(mod).not.toBeNull();
  expect(typeof mod!.protocol_apply_command).toBe("function");
  // Guard the mechanism this file depends on: a 1-arg export would mean the pkg
  // predates document-scoped engines and the per-doc namespacing below would
  // silently collapse into one engine.
  expect(mod!.protocol_apply_command.length).toBe(2);
  wasmMod = mod as WasmModule;
});

beforeEach(() => {
  localStorage.clear();
  invokeMock.mockReset();
  __resetNativeAuthorityForTests();
  setProtocolWasm(wasmMod);
  installNativeTransport();
  // Fresh engines for every case so a rerun cannot observe a previous arm.
  wasmMod.protocol_reset(`${NATIVE_NS}default`);
  wasmMod.protocol_reset(NATIVE_DOC);
  wasmMod.protocol_reset(WASM_DOC);
  wasmMod.protocol_reset(`${NATIVE_NS}${NATIVE_DOC}`);
  wasmMod.protocol_reset(`${NATIVE_NS}${WASM_DOC}`);
});

afterEach(() => {
  localStorage.clear();
  __resetNativeAuthorityForTests();
});

describe("native arm vs wasm arm: per-step state parity", () => {
  it("both dispatch arms reach the same state at every step", async () => {
    await seedNativeArm(NATIVE_DOC);
    seedWasmArm(WASM_DOC);

    const nativeObserved = await runArm(NATIVE_DOC, true);
    const wasmObserved = await runArm(WASM_DOC, false);

    expectStepParity(nativeObserved, wasmObserved);
  });

  it("an empty docId reaches the same engine on both arms", async () => {
    // bridge.ts resolves `envelope.docId ?? "default"` and nativeClient.ts:37-39
    // normalizes a raw "" again before the transport - but neither is what makes
    // this hold: BOTH engines normalize "" themselves (protocol_native_cmds.rs:25-31
    // for the native registry, document_core.rs:675-681 for the wasm one, which is
    // also why the transport shim models it). So this is an end-to-end CONTRACT pin
    // ("" addresses the reserved "default" document on either arm), NOT a detector
    // for a host-side normalization bug: removing the host's own normalization does
    // not change production behaviour.
    await seedNativeArm(EMPTY_DOC);
    seedWasmArm("default");

    const nativeObserved = await runArm(EMPTY_DOC, true);
    const wasmObserved = await runArm("default", false);

    expectStepParity(nativeObserved, wasmObserved);
  });
});

describe("native arm vs wasm arm: dispatch and barrier contracts", () => {
  it("the native arm dispatches through the Tauri transport and the wasm arm never does", async () => {
    await seedNativeArm(NATIVE_DOC);
    seedWasmArm(WASM_DOC);

    invokeMock.mockClear();
    await runArm(NATIVE_DOC, true);
    const nativeCalls = invokeMock.mock.calls.map((c) => c[0]);
    expect(nativeCalls.filter((c) => c === "protocol_apply_command_native").length).toBe(SCRIPT.length);

    invokeMock.mockClear();
    await runArm(WASM_DOC, false);
    expect(invokeMock.mock.calls.filter((c) => c[0] === "protocol_apply_command_native").length).toBe(0);
  });

  it("the native arm's seed barrier orders a command issued while the seed is in flight", async () => {
    // Deliberately do NOT await the seeds. Fire both seed calls and then the first
    // command back to back, so the command RACES two in-flight seeds: applyCommand
    // must await the barrier itself (bridge.ts:285) or its invoke would be recorded
    // before the seeds'. Awaiting the seeds first would make this assertion a
    // restatement of the call order this test just wrote - which is what it used to
    // be. The real barrier race is otherwise covered by
    // canonicalSeedBarrier.wiring.test.ts; this exercises it through applyCommand.
    useNativeAuthority(true);
    const layerSeed = createNativeSeed(NATIVE_DOC, 0, []);
    const canonicalSeed = seedNativeCanonical(NATIVE_DOC, CANONICAL_JSON);

    // expectedVersion is 0: neither seed bumps the document version. It must NOT be
    // read via getVersion, which awaits the same barrier and would hide the race.
    await applyCommand({
      contractVersion: CONTRACT_VERSION,
      expectedVersion: 0,
      docId: NATIVE_DOC,
      command: SCRIPT[0].command,
    });
    await Promise.all([layerSeed, canonicalSeed]);

    const order = invokeMock.mock.calls.map((c) => c[0]);
    const openIdx = order.indexOf("rust_pixels_open_document");
    const layerSeedIdx = order.indexOf("protocol_seed_native");
    const canonicalSeedIdx = order.indexOf("protocol_seed_canonical_native");
    const applyIdx = order.indexOf("protocol_apply_command_native");
    expect(openIdx).toBeGreaterThanOrEqual(0);
    expect(layerSeedIdx).toBeGreaterThan(openIdx);
    expect(canonicalSeedIdx).toBeGreaterThan(openIdx);
    expect(nativeSeedInvocations).toEqual([NATIVE_DOC]);
    // The barrier itself: the command landed after BOTH seeds even though it was
    // issued while they were still in flight.
    expect(applyIdx).toBeGreaterThan(layerSeedIdx);
    expect(applyIdx).toBeGreaterThan(canonicalSeedIdx);
  });

  // DOCUMENTS A DEFECT, NOT A CONTRACT. The two arms surface DIFFERENT error
  // strings for the same rejection:
  //   native arm -> "CODE: message"            (normalizeProtocolError, bridge.ts:300)
  //   wasm arm   -> the raw JSON envelope      (bridge.ts:317-325)
  // The wasm arm's normalization is dead code: the `throw` at bridge.ts:321 sits
  // inside the `try` whose bare `catch` at :322 immediately catches it, so :323
  // always re-throws the un-normalized message. Reported as a finding; NOT fixed
  // here (production change).
  //
  // NOT A NEW DISCOVERY: bridgeErrorEnvelope.test.ts:10-18 documents the same
  // mechanism and pins the raw-envelope message at :63-65 and :81. If
  // bridge.ts:317-325 is ever fixed, the FIRST failure here is the JSON.parse of
  // the wasm message below (a normalized message is no longer JSON), not the final
  // inequality - and bridgeErrorEnvelope.test.ts:63/:81 redden in the same run. All
  // of them must be flipped together.
  it("documents the raw error-shape divergence between the arms", async () => {
    await seedNativeArm(NATIVE_DOC);
    seedWasmArm(WASM_DOC);

    const mismatch: Command = { type: "addLayer", id: "bad", name: "Bad", width: 10, height: 10, index: 0 };
    const catchMessage = async (docId: string, native: boolean): Promise<string> => {
      useNativeAuthority(native);
      try {
        await applyCommand({ contractVersion: CONTRACT_VERSION, expectedVersion: 99, docId, command: mismatch });
        return "<resolved>";
      } catch (e) {
        return e instanceof Error ? e.message : String(e);
      }
    };

    const nativeMessage = await catchMessage(NATIVE_DOC, true);
    const wasmMessage = await catchMessage(WASM_DOC, false);

    expect(nativeMessage).toBe("E_VERSION_MISMATCH: expected version 99 got 0");
    const wasmEnvelope = JSON.parse(wasmMessage) as { code: string; message: string };
    expect(wasmEnvelope).toEqual({
      code: "E_VERSION_MISMATCH",
      message: "expected version 99 got 0",
    });
    expect(nativeMessage).not.toBe(wasmMessage);
  });
});

// ---- Part 3: the REAL document-open path, under both authority values --------
//
// Parts 1-2 seeded both engines by hand through ONE shared canonical channel, and
// that equalization is exactly what hides the production difference. Part 3 drives
// the app's own open path instead - `WorkspaceManager.addDocument`
// (workspace.ts:39-75) followed by `seedFacadeFromEngine` (facadeRegistry.ts:666-713)
// - once per authority value, then reads the state back through the app's own
// protocol read path (`bridge.getSnapshot`).
//
// MEASURED RESULT: the two authorities do NOT present the same document to the
// ENGINE, and they are not supposed to.
//
//   * native authority: the engine is the single canonical authority (ADR 0013
//     D-A, protocol_native_cmds.rs:37-40) and the open path seeds it with the
//     document's real layers (workspace.ts:68, `createNativeSeed`) plus the full
//     canonical shadow (workspace.ts:72, `seedNativeCanonical`). The engine holds
//     the document.
//   * wasm authority: NOTHING seeds the engine. Every seed call site is
//     `isNativeAuthority()`-gated (bridge.ts:156, :183, :201, :215), so the engine
//     holds only the layers the facade itself created - the TS model stays the
//     authority for legacy layers.
//
// That asymmetry is deliberate and documented in the codebase, not an oversight:
// facadeRegistry.ts:313-322 ("under wasm authority the engine only ever knows
// facade-created layers (legacy TS layers are never seeded into it), so the command
// would reorder a PARTIAL set") and :338-341 ("a merge/flatten would operate on a
// PARTIAL set and drop layers (data-loss class)"). The structural commit funnels
// compensate by refusing to route under wasm authority (:322, :351, :368).
//
// So Part 3 pins the ASYMMETRY, not a parity. It is the gate the counter said was
// missing: delete the native open-path seed and the native arm stops holding the
// document, which reddens here. What it does NOT claim is that the user sees a
// different document - the user-visible state is the TS/facade model, and that is
// the same in both modes (see the report's Part 3 for why that is a separate claim
// this file does not measure).

const OPEN_NATIVE_DOC = "open-path-native";
const OPEN_WASM_DOC = "open-path-wasm";

// What the app's own read path reports for one arm after its real open. Every
// field is captured WHILE THAT ARM'S AUTHORITY IS ACTIVE: `isNativeAuthority()`
// reads a process-global localStorage key (bridge.ts:83-89), so opening the second
// document flips the flag for the first document's reads too. Capturing later would
// read the wasm engine for both docs and the comparison would be meaningless.
type OpenObservation = {
  documentLayerIds: string[];
  documentLayerNames: string[];
  facadeLayerIds: string[];
  facadeLayerNames: string[];
  targetId: string;
  snapshot: RenderSnapshot;
  version: number;
};

/// Drive the app's REAL document-open path for one authority value, then read the
/// engine back through the bridge before the authority can change.
async function observeRealOpen(docId: string, native: boolean): Promise<OpenObservation> {
  useNativeAuthority(native);
  const wm = new WorkspaceManager();
  const session = WorkspaceManager.createBlankDocument(docId, "Open Path", 800, 600);
  wm.addDocument(session);
  // addDocument fires its native seeds without awaiting them (workspace.ts:68,
  // :72). seedFacadeFromEngine awaits the same per-doc seed barrier, so awaiting
  // it here is what makes both seeds land before the first read.
  await seedFacadeFromEngine(session.engine as never, getFacade(docId));
  const layers = session.engine.getLayers();
  // The USER-visible document: the facade projection the app renders from.
  // Compared by NAME across arms, not by id: createBlankDocument mints a random
  // layer id per document (workspace.ts:266), so the two arms' documents are
  // different documents with the same content.
  const facadeLayers = getFacade(docId).snapshot.layers ?? [];
  return {
    documentLayerIds: layers.map((l) => l.id).sort(),
    documentLayerNames: layers.map((l) => l.name).sort(),
    facadeLayerIds: facadeLayers.map((l) => l.id).sort(),
    facadeLayerNames: facadeLayers.map((l) => l.name).sort(),
    targetId: layers[0].id,
    snapshot: await getSnapshot(docId),
    version: await getVersion(docId),
  };
}

/// Issue one command on one arm, under that arm's authority, and read the engine
/// back the same way. Returns the outcome rather than throwing, so a rejection is
/// reported as a value.
async function commandUnderAuthority(
  docId: string,
  native: boolean,
  expectedVersion: number,
  command: Command,
): Promise<{ outcome: string; snapshot: RenderSnapshot }> {
  useNativeAuthority(native);
  let outcome: string;
  try {
    await applyCommand({ contractVersion: CONTRACT_VERSION, expectedVersion, docId, command });
    outcome = "<resolved>";
  } catch (e) {
    outcome = `<rejected: ${e instanceof Error ? e.message : String(e)}>`;
  }
  return { outcome, snapshot: await getSnapshot(docId) };
}

describe("Part 3: real document-open path under both authority values", () => {
  const TAURI_KEY = "__TAURI_INTERNALS__";
  const priorTauri = (globalThis as Record<string, unknown>)[TAURI_KEY];

  beforeEach(() => {
    // workspace.notifyRustPixelDoc is guarded on __TAURI_INTERNALS__
    // (workspace.ts:32) and returns silently without it. Without this line the
    // "real open path" would never reach the transport and Part 3 would test
    // nothing while staying green.
    (globalThis as Record<string, unknown>)[TAURI_KEY] = { invoke: invokeMock };
    __resetFacadeRegistryForTests();
    for (const id of [OPEN_NATIVE_DOC, OPEN_WASM_DOC]) {
      wasmMod.protocol_reset(id);
      wasmMod.protocol_reset(`${NATIVE_NS}${id}`);
    }
  });

  afterEach(() => {
    if (priorTauri === undefined) delete (globalThis as Record<string, unknown>)[TAURI_KEY];
    else (globalThis as Record<string, unknown>)[TAURI_KEY] = priorTauri;
  });

  it("the real open path seeds the native engine with the document and leaves the wasm engine empty", async () => {
    const nativeArm = await observeRealOpen(OPEN_NATIVE_DOC, true);
    const wasmArm = await observeRealOpen(OPEN_WASM_DOC, false);

    // The real document has layers; if this is 0 the test is vacuous.
    expect(nativeArm.documentLayerIds.length).toBeGreaterThan(0);

    // Native authority: the engine IS the document (layers + dims seeded at open).
    expect((nativeArm.snapshot.layers ?? []).map((l) => l.id).sort()).toEqual(
      nativeArm.documentLayerIds,
    );
    expect([nativeArm.snapshot.width, nativeArm.snapshot.height]).toEqual([800, 600]);

    // Wasm authority: the engine holds nothing. Deliberate - see the block comment
    // above and facadeRegistry.ts:313-322. Note the dims are OMITTED rather than
    // null: `RenderSnapshot.width/height` are `skip_serializing_if = "Option::is_none"`
    // (projection.rs:30-33), whose own comment names "an unseeded wasm engine running
    // only layer arms" as the case that produces this.
    expect(wasmArm.snapshot.layers ?? []).toEqual([]);
    expect([wasmArm.snapshot.width ?? null, wasmArm.snapshot.height ?? null]).toEqual([null, null]);

    // Same document, same open path, two different engine states. This is the
    // asymmetry Part 2's equalized seed could not see.
    expect(nativeArm.snapshot).not.toEqual(wasmArm.snapshot);

    // ...but the document the USER sees is the same, and it is the real document.
    // The user-visible state is the facade projection (facadeProjection.ts), which
    // the open path seeds from the TS engine in BOTH modes (facadeRegistry.ts:673-678),
    // not from the protocol engine. So the engine asymmetry does not reach the user
    // - and if a future change made the facade seed from the ENGINE instead, these
    // assertions are what would catch the user-visible consequence.
    expect(nativeArm.facadeLayerIds).toEqual(nativeArm.documentLayerIds);
    expect(wasmArm.facadeLayerIds).toEqual(wasmArm.documentLayerIds);
    expect(nativeArm.facadeLayerNames).toEqual(nativeArm.documentLayerNames);
    expect(wasmArm.facadeLayerNames).toEqual(nativeArm.facadeLayerNames);
  });

  it("the first command lands on the native arm and is a SILENT no-op on the wasm arm", async () => {
    const nativeArm = await observeRealOpen(OPEN_NATIVE_DOC, true);
    const wasmArm = await observeRealOpen(OPEN_WASM_DOC, false);

    const opacityOf = (snapshot: RenderSnapshot, id: string): number | null =>
      (snapshot.layers ?? []).find((l) => l.id === id)?.opacity ?? null;
    const setOpacity: Command = { type: "setOpacity", id: nativeArm.targetId, opacity: 0.5 };

    const nativeRun = await commandUnderAuthority(
      OPEN_NATIVE_DOC,
      true,
      nativeArm.version,
      setOpacity,
    );
    const wasmRun = await commandUnderAuthority(OPEN_WASM_DOC, false, wasmArm.version, setOpacity);

    // BOTH resolve. A metadata command naming a layer the engine does not know is
    // not an error: the SetOpacity arm falls through to an empty delta and the
    // version still bumps (document_core_apply.rs:610-612, then :916). So the wasm
    // arm does not fail loudly - it silently does nothing.
    expect(nativeRun.outcome).toBe("<resolved>");
    expect(wasmRun.outcome).toBe("<resolved>");

    // The edit landed on the authority that holds the document...
    expect(opacityOf(nativeRun.snapshot, nativeArm.targetId)).toBe(0.5);
    // ...and on the authority that does not, the layer is still absent entirely.
    expect(opacityOf(wasmRun.snapshot, nativeArm.targetId)).toBeNull();
    expect(wasmRun.snapshot.layers ?? []).toEqual([]);
  });
});
