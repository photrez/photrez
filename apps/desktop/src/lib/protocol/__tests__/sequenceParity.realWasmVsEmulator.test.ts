// Sequence-level parity: the real Rust protocol engine (wasm) vs the TS emulator.
//
// The existing per-op parity matrix measures SINGLE operations. This harness runs
// ONE deterministic, multi-step command sequence through TWO engines and compares
// a canonical state digest after EVERY step, plus undo/redo round-trips:
//
//   Engine A (authoritative): bridge.applyCommand -> wasm.protocol_apply_command
//             (real Rust ProtocolEngine, serde JSON in/out, same .wasm bytes the
//             production facade uses; loaded via the test shim's initSync).
//   Engine B (mirror):        emulateApply / bridge_emu (the TS fallback the
//             bridge uses only when the wasm is unarmed).
//
// WHY THIS MATTERS: the emulator claims to mirror the Rust engine. If it drifts,
// any test that trusts the emulator to predict Rust behavior overstates coverage.
// A digest divergence at a step is a measured finding (step + op + both digests),
// not a test to be loosened.
//
// DISCRIMINATING GUARANTEE
//   * The test asserts bridge.isFacadeArmed() and that bridge.wasm is the exact
//     module object handed back by getWasmExportModule(). If the production
//     setProtocolWasm() wiring is removed, the bridge falls back to the emulator
//     (or throws E_FACADE_NOT_READY while photrez.facade=1) and engine A's digest
//     stops being Rust-derived - the suite goes red. A dedicated falsification
//     case at the bottom proves the gate actually fires when the arm is removed.
//   * Engine B always runs through the emulator explicitly, so engine A can never
//     silently BE engine B: right after engine A runs, the emulator globals must
//     still be EMPTY (they are only populated by engine B's run).
//
// READ-BACK CHANNEL
//   Every engine-A step also reads wasm.protocol_snapshot_json() directly and
//   asserts it matches the bridge's snapshot digest. The native desktop
//   counterpart is protocol_canonical_native / protocol_snapshot_native (reached
//   over Tauri IPC); that surface is not runnable headlessly here and is reserved
//   for the manual native-executable run. It is deliberately NOT stubbed.

import { describe, it, expect, beforeAll, afterEach } from "vitest";
import { getWasmExportModule } from "@/components/editor/wasmExport";
import * as bridge from "@/lib/protocol/bridge";
import {
  emulateApply,
  emulateGetSnapshot,
  __resetEmulatedForTests,
  setEmuDocumentDims,
  getEmuSelection,
  getEmuDocumentDims,
} from "@/lib/protocol/bridge";
import { CONTRACT_VERSION, type Command } from "@/lib/protocol/types";

type WasmModule = {
  protocol_apply_command: (json: string, docId: string) => string;
  protocol_snapshot_json: (docId: string) => string;
  protocol_reset: (docId: string) => void;
  protocol_seed_canonical: (json: string, docId: string) => void;
};

let wasmModule: WasmModule | null = null;

const ENGINE_A_DOC = "sequence-parity-wasm";
const INITIAL_WIDTH = 200;
const INITIAL_HEIGHT = 200;

beforeAll(async () => {
  const m = await getWasmExportModule();
  expect(m).not.toBeNull();
  expect(typeof m.protocol_apply_command).toBe("function");
  wasmModule = m as WasmModule;
});

afterEach(() => {
  // Remove flags so this file cannot leak the facade gate into any later test.
  localStorage.removeItem("photrez.facade");
  localStorage.removeItem("photrez.facadeAuthority");
  __resetEmulatedForTests();
});

// ── Deterministic scripted sequence ──────────────────────────────────────
// Covers the full routed op set: layer lifecycle, transform, opacity, metadata
// (visibility / locks / rename / reorder / background / blend), parametrics
// (typed add + setLayerParams + setAdjustment), structural (duplicate / merge /
// flatten / rasterize), canvas (resize / crop / applyCrop), and selection.
// Fixed ids and fixed values - no randomness, so a divergence is reproducible.

const SHAPE = {
  kind: "star" as const,
  width: 64,
  height: 64,
  radius: 4,
  fill: { kind: "solid" as const, color: "#E15A17" },
  stroke: { enabled: false, color: "#000000", width: 0 },
  arrowHead: false,
};
const TEXT = {
  content: "Hi",
  fontFamily: "Arial",
  fontSize: 32,
  fontWeight: 400,
  fontStyle: "normal" as const,
  color: "#000000",
  align: "left" as const,
  lineHeight: 1.2,
  letterSpacing: 0,
  boxMode: "point" as const,
  boxWidth: 0,
  boxHeight: 0,
  stroke: { width: 0, color: "#000000" },
};

type Step = { op: string; cmd: Command };

const LAYER_LIFECYCLE_AND_METADATA: Step[] = [
  { op: "addLayer l1", cmd: { type: "addLayer", id: "l1", name: "Base", width: 200, height: 200, index: 0 } },
  { op: "addLayer l2", cmd: { type: "addLayer", id: "l2", name: "Mid", width: 100, height: 100, index: 0 } },
  { op: "addLayer l3", cmd: { type: "addLayer", id: "l3", name: "Top", width: 50, height: 50, index: 0 } },
  { op: "rename l2", cmd: { type: "rename", id: "l2", name: "Middle" } },
  { op: "setVisible l3 false", cmd: { type: "setVisible", id: "l3", visible: false } },
  { op: "setVisible l3 true", cmd: { type: "setVisible", id: "l3", visible: true } },
  { op: "setOpacity l2 0.5", cmd: { type: "setOpacity", id: "l2", opacity: 0.5 } },
  {
    op: "transformLayer l2",
    cmd: { type: "transformLayer", id: "l2", transform: { x: 10, y: 20, scaleX: 2, scaleY: 3, rotation: 45, flipH: true } },
  },
  { op: "setLocked l3 base", cmd: { type: "setLocked", id: "l3", kind: "base", locked: true } },
  { op: "setLocked l3 transparency", cmd: { type: "setLocked", id: "l3", kind: "transparency", locked: true } },
  { op: "setLocked l3 position", cmd: { type: "setLocked", id: "l3", kind: "position", locked: true } },
  { op: "setLocked l3 rotation", cmd: { type: "setLocked", id: "l3", kind: "rotation", locked: true } },
  { op: "setBlendMode l2 multiply", cmd: { type: "setBlendMode", id: "l2", mode: "multiply" } },
  { op: "reorder l3 to 2", cmd: { type: "reorder", id: "l3", to: 2 } },
  { op: "setBackgroundFlag l1", cmd: { type: "setBackgroundFlag", id: "l1" } },
  { op: "deleteLayer l3", cmd: { type: "deleteLayer", id: "l3" } },
];

const STRUCTURAL_CANVAS_SELECTION: Step[] = [
  { op: "addLayer s1 shape", cmd: { type: "addLayer", id: "s1", name: "Star", width: 64, height: 64, index: 0, layerType: "shape", shapeParams: SHAPE } },
  { op: "setLayerParams s1", cmd: { type: "setLayerParams", id: "s1", shapeParams: { ...SHAPE, radius: 9 } } },
  { op: "setAdjustment s1", cmd: { type: "setAdjustment", id: "s1", adjustment: { brightness: 10, contrast: -5, saturation: 20 } } },
  { op: "setAdjustment s1 clear", cmd: { type: "setAdjustment", id: "s1" } },
  { op: "rasterizeLayer s1", cmd: { type: "rasterizeLayer", id: "s1" } },
  { op: "addLayer t1 text", cmd: { type: "addLayer", id: "t1", name: "Caption", width: 100, height: 20, index: 0, layerType: "text", textData: TEXT } },
  { op: "rasterizeLayer t1", cmd: { type: "rasterizeLayer", id: "t1" } },
  { op: "duplicateLayer l1", cmd: { type: "duplicateLayer", id: "l1", newId: "d1" } },
  { op: "mergeDown d1", cmd: { type: "mergeDown", id: "d1", mergedId: "m1" } },
  { op: "mergeSelected [s1,t1]", cmd: { type: "mergeSelected", ids: ["s1", "t1"], mergedId: "m2" } },
  { op: "flatten", cmd: { type: "flatten", mergedId: "bg1" } },
  { op: "resizeCanvas 400x300", cmd: { type: "resizeCanvas", width: 400, height: 300 } },
  { op: "cropCanvas 10,20,100,100", cmd: { type: "cropCanvas", x: 10, y: 20, width: 100, height: 100 } },
  {
    op: "applyCrop 0,0,50,50 rot=5 target=100x100",
    cmd: { type: "applyCrop", x: 0, y: 0, width: 50, height: 50, rotation: 5, targetWidth: 100, targetHeight: 100 },
  },
  { op: "setSelection", cmd: { type: "setSelection", selection: { x: 1, y: 2, width: 30, height: 40, angle: 5, shape: "ellipse" } } },
  { op: "invertSelection", cmd: { type: "invertSelection" } },
  { op: "clearSelection", cmd: { type: "clearSelection" } },
  { op: "selectAll", cmd: { type: "selectAll" } },
  { op: "invertSelection", cmd: { type: "invertSelection" } },
];

function undos(n: number, label = "undo"): Step[] {
  return Array.from({ length: n }, (_, i) => ({ op: `${label} ${i + 1}`, cmd: { type: "undo" } as Command }));
}
function redos(n: number, label = "redo"): Step[] {
  return Array.from({ length: n }, (_, i) => ({ op: `${label} ${i + 1}`, cmd: { type: "redo" } as Command }));
}

// ── Canonical digest ─────────────────────────────────────────────────────
// resourceId is the engine's internal pixel-resource handle and dirtyRect is a
// transient render hint; neither is canonical document metadata, so both are
// excluded. Everything else the host projects its model from is compared.

const CANON_LAYER_KEYS = [
  "id",
  "name",
  "visible",
  "opacity",
  "x",
  "y",
  "scaleX",
  "scaleY",
  "rotation",
  "layerType",
  "blendMode",
  "locked",
  "lockTransparency",
  "lockPosition",
  "lockRotation",
  "isBackground",
  "hasAdjustments",
  "flipH",
  "flipV",
  "width",
  "height",
  "shapeParams",
  "textData",
  "basicAdjustment",
] as const;

function canonLayer(layer: Record<string, unknown>): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const key of CANON_LAYER_KEYS) {
    const value = layer?.[key];
    if (value !== undefined) out[key] = value;
  }
  return pruneAbsent(out);
}

// An absent optional is `null` on the Rust wire (serde `Option` without
// skip_serializing_if) and `undefined`/omitted in the TS emulator. Both mean
// "no value", so the semantic digest treats them as equal. The raw
// representation difference is pinned separately (see the textData test) so it
// is documented rather than silently normalized away.
function pruneAbsent(value: unknown): any {
  if (value === null || value === undefined) return undefined;
  if (Array.isArray(value)) return value.map(pruneAbsent);
  if (typeof value === "object") {
    const out: Record<string, unknown> = {};
    for (const key of Object.keys(value as Record<string, unknown>)) {
      const pruned = pruneAbsent((value as Record<string, unknown>)[key]);
      if (pruned !== undefined) out[key] = pruned;
    }
    return out;
  }
  return value;
}

function canonSelection(selection: unknown): Record<string, unknown> | null {
  if (!selection || typeof selection !== "object") return null;
  const s = selection as Record<string, unknown>;
  const out: Record<string, unknown> = {};
  for (const key of ["x", "y", "width", "height", "angle", "shape", "inverted"]) {
    if (s[key] !== undefined && s[key] !== null) out[key] = s[key];
  }
  return out;
}

function stableStringify(value: unknown): string {
  if (value === null) return "null";
  if (Array.isArray(value)) return `[${value.map(stableStringify).join(",")}]`;
  if (typeof value === "object") {
    const obj = value as Record<string, unknown>;
    return `{${Object.keys(obj)
      .sort()
      .map((k) => `${JSON.stringify(k)}:${stableStringify(obj[k])}`)
      .join(",")}}`;
  }
  return JSON.stringify(value);
}

function fnv1a(input: string): string {
  let hash = 0x811c9dc5;
  for (let i = 0; i < input.length; i++) {
    hash ^= input.charCodeAt(i);
    hash = Math.imul(hash, 0x01000193);
  }
  return (hash >>> 0).toString(16).padStart(8, "0");
}

type EngineState = {
  layers: Array<Record<string, unknown>>;
  selection: unknown;
  width: number | null;
  height: number | null;
  version: number;
  // Engine A only: digest of the direct wasm.protocol_snapshot_json() read taken
  // at the same instant as the bridge snapshot, so the observation channel is
  // compared per step (not after the run).
  readBackDigest?: string;
};

function canonicalJson(state: EngineState): string {
  return stableStringify({
    layers: (state.layers ?? []).map(canonLayer),
    selection: canonSelection(state.selection),
    dims: { width: state.width ?? null, height: state.height ?? null },
  });
}

function digestOf(state: EngineState): string {
  return fnv1a(canonicalJson(state));
}

// Compact field-level diff so a divergence is reported precisely (and is not
// truncated by the test reporter the way a full JSON dump is).
function deepDiff(prefix: string, a: unknown, b: unknown, out: string[]): void {
  if (JSON.stringify(a) === JSON.stringify(b)) return;
  if (
    a !== null &&
    b !== null &&
    typeof a === "object" &&
    typeof b === "object" &&
    !Array.isArray(a) &&
    !Array.isArray(b)
  ) {
    const ao = a as Record<string, unknown>;
    const bo = b as Record<string, unknown>;
    const keys = [...new Set([...Object.keys(ao), ...Object.keys(bo)])].sort();
    for (const k of keys) deepDiff(prefix ? `${prefix}.${k}` : k, ao[k], bo[k], out);
    return;
  }
  out.push(`${prefix}: A=${JSON.stringify(a)} B=${JSON.stringify(b)}`);
}

function stateDiff(a: EngineState, b: EngineState): string {
  const parts: string[] = [];
  const al = a.layers ?? [];
  const bl = b.layers ?? [];
  if (al.length !== bl.length) parts.push(`layers.length A=${al.length} B=${bl.length}`);
  const n = Math.max(al.length, bl.length);
  for (let i = 0; i < n; i++) {
    const la = al[i];
    const lb = bl[i];
    if (!la) {
      parts.push(`layer[${i}] A=absent B=${JSON.stringify(canonLayer(lb))}`);
      continue;
    }
    if (!lb) {
      parts.push(`layer[${i}] A=${JSON.stringify(canonLayer(la))} B=absent`);
      continue;
    }
    const ca = canonLayer(la);
    const cb = canonLayer(lb);
    const keys = [...new Set([...Object.keys(ca), ...Object.keys(cb)])].sort();
    const fields: string[] = [];
    for (const k of keys) deepDiff(k, ca[k], cb[k], fields);
    if (fields.length) parts.push(`layer[${i}] ${String(la.id ?? lb.id ?? "?")}: ${fields.join("; ")}`);
  }
  const sa = JSON.stringify(canonSelection(a.selection));
  const sb = JSON.stringify(canonSelection(b.selection));
  if (sa !== sb) parts.push(`selection A=${sa} B=${sb}`);
  const da = JSON.stringify({ w: a.width, h: a.height });
  const db = JSON.stringify({ w: b.width, h: b.height });
  if (da !== db) parts.push(`dims A=${da} B=${db}`);
  return parts.join(" | ");
}

// ── Engine runners ───────────────────────────────────────────────────────

type Runner = { apply: (cmd: Command) => Promise<void>; read: () => Promise<EngineState> };

function engineAWasm(): Runner {
  return {
    apply: async (cmd) => {
      await bridge.applyCommand({ contractVersion: CONTRACT_VERSION, docId: ENGINE_A_DOC, command: cmd });
    },
    read: async () => {
      const snap = await bridge.getSnapshot(ENGINE_A_DOC);
      const raw = JSON.parse(wasmModule!.protocol_snapshot_json(ENGINE_A_DOC)) as {
        layers: Array<Record<string, unknown>>;
        selection?: unknown;
        width?: number;
        height?: number;
        version: number;
      };
      const readBackDigest = digestOf({
        layers: raw.layers,
        selection: raw.selection ?? null,
        width: raw.width ?? null,
        height: raw.height ?? null,
        version: raw.version,
      });
      return {
        layers: snap.layers as unknown as Array<Record<string, unknown>>,
        selection: snap.selection ?? null,
        width: snap.width ?? null,
        height: snap.height ?? null,
        version: snap.version,
        readBackDigest,
      };
    },
  };
}

function engineBEmulator(): Runner {
  return {
    apply: async (cmd) => {
      emulateApply({ contractVersion: CONTRACT_VERSION, command: cmd });
    },
    read: async () => {
      const snap = emulateGetSnapshot();
      const dims = getEmuDocumentDims();
      return {
        layers: snap.layers as unknown as Array<Record<string, unknown>>,
        selection: getEmuSelection(),
        width: dims.width,
        height: dims.height,
        version: snap.version,
      };
    },
  };
}

function resetEngineA(): void {
  wasmModule!.protocol_reset(ENGINE_A_DOC);
  const canon = JSON.stringify({ id: ENGINE_A_DOC, name: "S", width: INITIAL_WIDTH, height: INITIAL_HEIGHT, layers: [] });
  wasmModule!.protocol_seed_canonical(canon, ENGINE_A_DOC);
}

function resetEngineB(): void {
  __resetEmulatedForTests();
  setEmuDocumentDims(INITIAL_WIDTH, INITIAL_HEIGHT);
}

type Recorded = { op: string; digest: string; version: number; state: EngineState };

async function runScript(runner: Runner, script: Step[]): Promise<Recorded[]> {
  const out: Recorded[] = [];
  for (const step of script) {
    await runner.apply(step.cmd);
    const state = await runner.read();
    out.push({ op: step.op, digest: digestOf(state), version: state.version, state });
  }
  return out;
}

// The full scripted sequence, matching the plan's "forward batch, undo N, redo M,
// more forward, undo to base, redo to tip" shape plus a full undo/redo round-trip.
function fullScript(): { steps: Step[]; checkpoints: { tip1: number; tip2: number; base: number } } {
  const steps: Step[] = [];
  const base = 0;
  steps.push(...LAYER_LIFECYCLE_AND_METADATA);
  const tip1 = steps.length; // index of last metadata step
  steps.push(...undos(10, "undo-roundtrip")); // walk back 10
  steps.push(...redos(10, "redo-roundtrip")); // full round trip -> must equal tip1
  steps.push(...undos(10, "undo-partial"));
  steps.push(...redos(4, "redo-partial")); // partial forward
  steps.push(...STRUCTURAL_CANVAS_SELECTION); // more forward (truncates the rest)
  const tip2 = steps.length;
  steps.push(...undos(100, "undo-to-base"));
  steps.push(...redos(100, "redo-to-tip"));
  return { steps, checkpoints: { tip1, tip2, base } };
}

describe("sequence parity: real Rust protocol engine vs TS emulator", () => {
  it("is armed to the real wasm module (not the emulator)", () => {
    expect(bridge.isFacadeArmed()).toBe(true);
    // Identity: the bridge's armed module IS the module getWasmExportModule()
    // returned. If the production setProtocolWasm() wiring is removed, bridge.wasm
    // is null and this fails.
    expect(bridge.wasm).toBe(wasmModule);
  });

  it("agrees on canonical state at every step, including undo/redo walk-backs", async () => {
    // Drive engine A with the facade flag ON: if the wasm were unarmed, the
    // bridge would throw E_FACADE_NOT_READY instead of silently emulating.
    localStorage.setItem("photrez.facade", "1");
    localStorage.setItem("photrez.facadeAuthority", "wasm");

    // Engine A must leave the emulator untouched. Reset the emulator first so the
    // "untouched" claim is measured against a known-empty baseline.
    __resetEmulatedForTests();
    resetEngineA();

    const { steps, checkpoints } = fullScript();
    const engineASteps = await runScript(engineAWasm(), steps);

    // Discriminator: if engine A had fallen back to the emulator, these globals
    // would be populated (the emulator is only driven by engine B below).
    expect(emulateGetSnapshot().layers.length, "engine A must not run the emulator").toBe(0);

    // Read-back channel: every engine-A step's production snapshot read must equal
    // the direct wasm export for the same step (captured at read time).
    for (let i = 0; i < steps.length; i++) {
      expect(engineASteps[i].state.readBackDigest, `read-back mismatch at step ${i} (${steps[i].op})`).toBe(
        engineASteps[i].digest,
      );
    }

    resetEngineB();
    const engineBSteps = await runScript(engineBEmulator(), steps);

    expect(engineBSteps.length).toBe(engineASteps.length);

    // Per-step parity report: collect every divergence with full detail, then
    // fail ONCE with the complete picture (a hidden divergence is the bug class
    // this harness exists to stop).
    const divergences: string[] = [];
    for (let i = 0; i < engineASteps.length; i++) {
      const a = engineASteps[i];
      const b = engineBSteps[i];
      if (a.digest !== b.digest) {
        divergences.push(`step ${i} op="${steps[i].op}" :: ${stateDiff(a.state, b.state)}`);
      }
    }

    // Diagnostic summary so a green run still prints what was covered.
    // eslint-disable-next-line no-console
    console.log(
      `[sequence-parity] steps=${steps.length} parsed=${engineASteps.length} divergences=${divergences.length}`,
    );
    for (const d of divergences) {
      // eslint-disable-next-line no-console
      console.log(`[sequence-parity] DIVERGENCE ${d}`);
    }

    expect(divergences.join("\n"), `emulator != real Rust engine:\n${divergences.join("\n")}`).toBe("");

    // Round-trip 1: undo 10 then redo 10 must return both engines to the tip.
    const tip1A = engineASteps[checkpoints.tip1 - 1];
    const afterRoundtripA = engineASteps[checkpoints.tip1 + 19];
    expect(afterRoundtripA.digest, "engine A undo/redo round-trip").toBe(tip1A.digest);
    const tip1B = engineBSteps[checkpoints.tip1 - 1];
    const afterRoundtripB = engineBSteps[checkpoints.tip1 + 19];
    expect(afterRoundtripB.digest, "engine B undo/redo round-trip").toBe(tip1B.digest);

    // Round-trip 2: undo to base then redo to tip returns both engines to tip2.
    const tip2A = engineASteps[checkpoints.tip2 - 1];
    const afterRedoToTipA = engineASteps[engineASteps.length - 1];
    expect(afterRedoToTipA.digest, "engine A undo-to-base/redo-to-tip").toBe(tip2A.digest);
    const tip2B = engineBSteps[checkpoints.tip2 - 1];
    const afterRedoToTipB = engineBSteps[engineBSteps.length - 1];
    expect(afterRedoToTipB.digest, "engine B undo-to-base/redo-to-tip").toBe(tip2B.digest);

    // Versions must advance identically through the same command sequence.
    const versionMismatch = engineASteps.findIndex((s, i) => s.version !== engineBSteps[i].version);
    expect(
      versionMismatch,
      versionMismatch >= 0
        ? `version mismatch at step ${versionMismatch} (${steps[versionMismatch].op}): ` +
            `wasm=${engineASteps[versionMismatch].version} emu=${engineBSteps[versionMismatch].version}`
        : "",
    ).toBe(-1);
  });

  it("documents the raw textData wire form (Rust null vs emulator omitted)", async () => {
    // The sequence digest above found the ONLY real-vs-emulator difference in this
    // scenario: for an added text layer, the Rust wire form carries explicit null
    // for absent optional TextData fields while the emulator keeps the input object
    // (fields absent). This test pins that representation detail so the semantic
    // normalization is a documented decision, not a hidden pass.
    localStorage.setItem("photrez.facade", "1");
    localStorage.setItem("photrez.facadeAuthority", "wasm");
    resetEngineA();
    await bridge.applyCommand({
      contractVersion: CONTRACT_VERSION,
      docId: ENGINE_A_DOC,
      command: { type: "addLayer", id: "tx", name: "T", width: 10, height: 10, index: 0, layerType: "text", textData: TEXT },
    });
    const raw = JSON.parse(wasmModule!.protocol_snapshot_json(ENGINE_A_DOC)) as {
      layers: Array<Record<string, unknown>>;
    };
    const wasmText = (raw.layers.find((l) => l.id === "tx") as Record<string, any>).textData;

    resetEngineB();
    emulateApply({
      contractVersion: CONTRACT_VERSION,
      command: { type: "addLayer", id: "tx", name: "T", width: 10, height: 10, index: 0, layerType: "text", textData: TEXT },
    });
    const emuLayer = emulateGetSnapshot().layers.find((l) => l.id === "tx") as unknown as Record<string, any>;
    const emuText = emuLayer.textData;

    expect(wasmText.underline).toBeNull();
    expect(wasmText.strikethrough).toBeNull();
    expect(wasmText.uppercase).toBeNull();
    expect(wasmText.stroke.align).toBeNull();
    expect(emuText.underline).toBeUndefined();
    expect(emuText.strikethrough).toBeUndefined();
    expect(emuText.uppercase).toBeUndefined();
    expect(emuText.stroke.align).toBeUndefined();

    // Semantically identical once absent optionals are dropped on both sides.
    expect(pruneAbsent(wasmText)).toEqual(pruneAbsent(emuText));
  });

  it("falsification: removing the wasm arm makes the real-engine path fail loud", async () => {
    // This proves the discriminator above is real: with the bridge unarmed and the
    // facade flag ON, engine A can no longer silently substitute the emulator.
    localStorage.setItem("photrez.facade", "1");
    localStorage.setItem("photrez.facadeAuthority", "wasm");
    bridge.setProtocolWasm(null as unknown as Parameters<typeof bridge.setProtocolWasm>[0]);
    __resetEmulatedForTests();
    expect(bridge.isFacadeArmed()).toBe(false);

    await expect(
      bridge.applyCommand({ contractVersion: CONTRACT_VERSION, docId: ENGINE_A_DOC, command: { type: "addLayer", id: "x", name: "x", width: 1, height: 1, index: 0 } }),
    ).rejects.toThrow(/E_FACADE_NOT_READY/);

    // Restore the real arm for any later test in the file (getWasmExportModule is
    // cached; re-arming uses the captured module object).
    bridge.setProtocolWasm(wasmModule as unknown as Parameters<typeof bridge.setProtocolWasm>[0]);
    expect(bridge.isFacadeArmed()).toBe(true);
  });
});
