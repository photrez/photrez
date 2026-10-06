// SPDX-License-Identifier: AGPL-3.0-or-later

/**
 * A legacy rebuild predicate must consult the SAME dropped-id ledger its sibling
 * re-add path consults.
 *
 * THE DEFECT. `syncLayersFromRust` rebuilds the model under facade ownership from
 * the Rust graph mirror. An id the mirror no longer lists is re-added by
 * `survives()`, which admits it when the facade owns it and the engine projected
 * it in its last snapshot. Its sibling - the mirror-absent, model-absent INSERT
 * path a few lines below - is guarded with `everDroppedIds` for exactly the
 * reason the comment there gives: without it, a removed id comes back as a
 * pixel-less ghost. The ledger is consulted on ONE of the TWO re-add paths.
 *
 * So every intended disappearance - legacy delete, merge down, merge selected,
 * flatten, facade delete, undone add - records its node into `everDroppedIds`
 * BEFORE the mutation (that is what `recordDroppedNode` does, and the merge/flatten
 * arms call it before dispatching to the mirror), and `survives()` then ignores
 * the ledger and resurrects any victim that is facade-owned. Measured in the real
 * app at the shipped origin: merging a facade-OWNED layer with a host-CREATED one
 * left the owned layer alive, `["Paint + Background", "Background"]`, n=2.
 *
 * PRE-EXISTING: `survives()` arrives from commit `84701cf` (2026-09-14), an
 * ancestor of `1a34940`. This is not a regression from the unpushed commits.
 *
 * COVERAGE BOUNDARY. These cases drive the REAL `DocumentEngine` with the REAL
 * wasm graph mirror (`USE_RUST_SSOT` builds `rustEngine` from the loaded module),
 * so `syncLayersFromRust` and its `survives()` predicate run for real. No
 * hand-written mirror double - a double is exactly what let this hide.
 *
 * NOT IN SCOPE. `resolveSelectionRoute` declining a mixed owned/unowned selection
 * is a USABILITY gap with a correct result (the legacy arm runs and produces the
 * right vector), not this correctness defect. Left alone on purpose.
 */
import { describe, it, expect, vi, beforeAll, beforeEach, afterEach, type Mock } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import { WorkspaceManager } from "@/engine/workspace";
import { isFacadeOwnedLayer, hasFacadeOwnedLayers } from "@/engine/document";
import { CommandHistory } from "@/engine/history";
import { mergeActiveLayerDown } from "@/components/editor/layers/layerOperations";
import { runFacadeExternalHandoff } from "@/components/editor/facadeHistoryHandoff";
import type { EditorContextValue } from "@/components/editor/shell/EditorContext";
import { getWasmExportModule } from "@/components/editor/wasmExport";
import {
  __resetFacadeRegistryForTests,
  seedFacadeFromEngine,
  installFacadeCommitShim,
  getFacade,
} from "@/lib/protocol/facadeRegistry";
import { __resetNativeAuthorityForTests, awaitNativeSeed } from "@/lib/protocol/bridge";
import { __resetCanonicalRepushForTests } from "@/lib/protocol/canonicalSeed";
import {
  createRustStoreEmulator,
  type RustStoreEmulator,
} from "@/lib/paint/__tests__/rustStoreEmulator";
import { installFaithfulCanvas } from "@/__tests__/faithfulOffscreenCanvas";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
const invokeMock = invoke as unknown as Mock<
  (cmd: string, args: Record<string, unknown>) => Promise<unknown>
>;

type WasmModule = {
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
let store: RustStoreEmulator;
let restoreCanvas: (() => void) | undefined;

const TAURI_KEY = "__TAURI_INTERNALS__";
const NS = "native::";
// UNIQUE per test. `DocumentEngine` builds its graph mirror from the loaded wasm
// module, and the mirror is keyed by document id - so reusing one id across
// cases lets one case's layers bleed into the next one's mirror and makes a
// green result untrustworthy.
let DOC = "syncResurrection-0";
const SIZE = 64;

function nativeRejection(e: unknown): string {
  const raw = e instanceof Error ? e.message : String(e);
  try {
    const parsed = JSON.parse(raw) as { code?: unknown; message?: unknown };
    if (typeof parsed?.code === "string" && typeof parsed?.message === "string") {
      return `${parsed.code}: ${parsed.message}`;
    }
  } catch {
    /* not a JSON envelope */
  }
  return raw;
}

function installTransport(): void {
  const opened = new Set<string>();
  invokeMock.mockImplementation(async (cmd: string, args: Record<string, unknown> = {}, options?: any) => {
    if (cmd.startsWith("rust_pixels_")) return store.invoke(cmd, args, options);
    const rawDocId = (args.docId as string) ?? "";
    const key = `${NS}${rawDocId === "" ? "default" : rawDocId}`;
    switch (cmd) {
      case "rust_pixels_open_document":
        if (!opened.has(key)) {
          opened.add(key);
          wasmMod.protocol_reset(key);
        }
        return undefined;
      case "protocol_seed_native":
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
      default:
        throw `E_UNKNOWN_COMMAND: ${cmd}`;
    }
  });
}

type Engine = ReturnType<WorkspaceManager["getActiveEngine"]> & object;

function makeRenderer(): never {
  return {
    uploadImage: vi.fn(),
    uploadSurfaceTiles: vi.fn(),
    destroyTexture: vi.fn(),
    invalidatePaintSurface: vi.fn(),
  } as never;
}

function makeEditor(engine: Engine, history: CommandHistory): EditorContextValue {
  return {
    workspace: {
      getActiveEngine: () => engine,
      getActiveHistory: () => history,
      getActiveDocumentId: () => engine.getId(),
      notifyVisualChange: vi.fn(),
    },
    renderer: { uploadImage: vi.fn() },
    scheduler: { requestRender: vi.fn() },
  } as unknown as EditorContextValue;
}

/**
 * ONE undo press, replicated verbatim from `useEditorCommands.restoreHistorySnapshot`:
 * the `hasFacadeOwnedLayers` gate, the production handoff unit, and the TS-history
 * fall-through. The hook closure itself is not exported, so the branch is copied -
 * the same approach `mixedHistory.test.ts` documents.
 */
async function pressUndo(engine: Engine, history: CommandHistory): Promise<"facade" | "ts" | "none"> {
  const editor = makeEditor(engine, history);
  if (hasFacadeOwnedLayers()) {
    if (await runFacadeExternalHandoff(editor, "undo")) return "facade";
  }
  const popped = history.undo(engine.snapshot());
  if (!popped) return "none";
  try {
    engine.restore(popped as never);
    return "ts";
  } catch {
    // E_FACADE_OWNED while owned ids exist: the documented blocked fall-through.
    return "none";
  }
}

/** The facade's cursor as the real engine reports it: undo depth, entry count. */
function cursorReading(): string {
  const q = JSON.parse(
    wasmMod.protocol_history_query_json(`${NS}${DOC}`),
  ) as { cursor?: number; entries?: unknown[]; pendingExternal?: unknown };
  return `cursor=${q.cursor} entries=${q.entries?.length ?? -1} pending=${q.pendingExternal ? 1 : 0}`;
}

function ids(engine: Engine): string[] {
  return engine.getLayers().map((l) => l.id);
}

function names(engine: Engine): string[] {
  return engine.getLayers().map((l) => l.name);
}

function giveRaster(engine: Engine, layerId: string, fill: string): void {
  const layer = engine.getLayer(layerId);
  const canvas = new OffscreenCanvas(layer!.width, layer!.height);
  const ctx = canvas.getContext("2d") as unknown as CanvasRenderingContext2D;
  ctx.fillStyle = fill;
  ctx.fillRect(0, 0, layer!.width, layer!.height);
  engine.setLayerImageBitmap(layerId, canvas.transferToImageBitmap());
}

async function openSeededDocument(): Promise<Engine> {
  const wm = new WorkspaceManager();
  const session = WorkspaceManager.createBlankDocument(DOC, "Sync", SIZE, SIZE);
  wm.addDocument(session);
  const engine = session.engine as never as Engine;
  liveEngine = engine as never;
  await seedFacadeFromEngine(engine as never, getFacade(DOC));
  await awaitNativeSeed(DOC);
  return engine;
}

/** Add a FACADE-OWNED layer: minted by the facade and projected, which is the
 *  only way ownership is granted (`applyFacadeSnapshot` -> `markFacadeOwned`). */
async function addOwnedLayer(engine: Engine, name: string): Promise<string> {
  const facade = getFacade(DOC);
  const before = new Set(ids(engine));
  await facade.addLayer(name, SIZE, SIZE);
  await engine.applyFacadeSnapshot(facade.snapshot as never);
  const added = ids(engine).filter((id) => !before.has(id));
  expect(added.length, `${name} was added through the facade`).toBe(1);
  giveRaster(engine, added[0], "#3366cc");
  return added[0];
}

/** Add a HOST-CREATED layer: NOT facade-owned, so the routed arm declines a mixed
 *  selection and the legacy arm is what runs - the shipped shape of the defect. */
function addHostLayer(engine: Engine, name: string): string {
  const created = engine.addLayer(name, SIZE, SIZE);
  giveRaster(engine, created.id, "#cc6633");
  return created.id;
}

/**
 * `[Background(owned), Paint(unowned)]` adjacent, one above the other.
 *
 * The blank document's own layer BECOMES the owned Background (a projection marks
 * it facade-owned), and the unowned Paint is added host-side on top of it. No
 * delete is used to get there: deleting a facade-owned layer is itself one of the
 * paths this defect corrupts, so the setup refuses to depend on it.
 *
 * `addLayer` appends below the stack, so which id ends up on top is not what is
 * under test. What IS under test is that a merge down makes BOTH participants
 * victims and that one of them is facade-owned - so the caller is handed whichever
 * id sits on top.
 */
async function openMixedPair(): Promise<{
  engine: Engine;
  ownedId: string;
  hostId: string;
  topId: string;
}> {
  const engine = await openSeededDocument();
  const facade = getFacade(DOC);
  await engine.applyFacadeSnapshot(facade.snapshot as never);
  const ownedId = ids(engine)[0];
  expect(
    ids(engine).length,
    `premise: exactly one seeded layer, got ${JSON.stringify(names(engine))}`,
  ).toBe(1);
  giveRaster(engine, ownedId, "#3366cc");
  expect(
    isFacadeOwnedLayer(ownedId),
    "premise: the Background layer is FACADE-OWNED",
  ).toBe(true);

  const hostId = addHostLayer(engine, "Paint");
  expect(isFacadeOwnedLayer(hostId), "premise: Paint is NOT facade-owned").toBe(false);

  const order = ids(engine);
  expect(order.length, "premise: exactly the two layers under test").toBe(2);
  expect(new Set(order), "premise: both ids are live").toEqual(new Set([ownedId, hostId]));
  return { engine, ownedId, hostId, topId: order[0] };
}

// The commit shim resolves the engine at COMMIT time, so the holder is per test
// while the shim install is per FILE (it is module-sticky, like EditorShell boot).
let liveEngine: { getId(): string; getLayers(): Array<{ id: string }> } | null = null;

beforeAll(async () => {
  const mod = (await getWasmExportModule()) as WasmModule | null;
  expect(mod, "the REAL wasm pkg must load").not.toBeNull();
  wasmMod = mod!;
  (globalThis as Record<string, unknown>)[TAURI_KEY] = { invoke: invokeMock };
  // Production installs this once at EditorShell boot. WITHOUT it a legacy
  // `history.commit` records nothing in the facade, so the undo silently takes the
  // TypeScript path instead of the facade handoff - which is exactly the arm the
  // shipped default takes and the reason this harness must have it.
  installFacadeCommitShim({
    getEngine: () => liveEngine,
    getDocId: () => liveEngine?.getId() ?? "default",
  });
});

let docSeq = 0;

beforeEach(() => {
  localStorage.clear();
  localStorage.setItem("photrez.facade", "1");
  localStorage.setItem("photrez.facadeAuthority", "native");
  DOC = `syncResurrection-${(docSeq += 1)}`;
  restoreCanvas = installFaithfulCanvas();
  invokeMock.mockReset();
  store = createRustStoreEmulator();
  __resetNativeAuthorityForTests();
  __resetCanonicalRepushForTests();
  __resetFacadeRegistryForTests();
  installTransport();
  wasmMod.protocol_reset(`${NS}${DOC}`);
});

afterEach(() => {
  store.dispose();
  localStorage.clear();
  restoreCanvas?.();
  restoreCanvas = undefined;
  __resetNativeAuthorityForTests();
  __resetCanonicalRepushForTests();
  __resetFacadeRegistryForTests();
  vi.restoreAllMocks();
});

describe("FALSIFIER: which write resurrects the victim", () => {
  it("samples the layer vector right after mergeDown, after a microtask, and after the next sync", async () => {
    const { engine, ownedId, hostId, topId } = await openMixedPair();

    // (1) immediately after `mergeDown` returns.
    engine.mergeDown(topId);
    const sample1 = { ids: ids(engine), names: names(engine) };

    // (2) after one microtask, so any fire-and-forget seeding has settled.
    await new Promise((r) => setTimeout(r, 0));
    const sample2 = { ids: ids(engine), names: names(engine) };

    // (3) after the next sync: a legacy add calls notifyChange, which re-pushes the
    // model and runs `syncLayersFromRust` again. (A legacy metadata mutator is
    // NOT usable here - every one of them refuses a facade-owned layer.)
    addHostLayer(engine, "Probe");
    const sample3 = { ids: ids(engine), names: names(engine) };

    const report =
      `sample1=${JSON.stringify(sample1)} sample2=${JSON.stringify(sample2)} ` +
      `sample3=${JSON.stringify(sample3)} owned=${ownedId} host=${hostId}`;

    // The merge must have destroyed BOTH participants. Assert the real id list: a
    // count would also pass with the wrong single survivor.
    expect(sample1.ids, report).not.toContain(ownedId);
    expect(sample1.ids, report).not.toContain(hostId);
    expect(sample1.ids.length, report).toBe(1);
    expect(sample2.ids, report).toEqual(sample1.ids);
    expect(sample3.ids, report).not.toContain(ownedId);
    expect(sample3.ids, report).not.toContain(hostId);
  });
});

describe("REGRESSION: the post-op vector is exactly the merged layer", () => {
  it("mergeDown with a facade-owned victim destroys that victim", async () => {
    const { engine, ownedId, hostId, topId } = await openMixedPair();
    engine.mergeDown(topId);
    const after = ids(engine);
    expect(after, "the owned victim is gone").not.toContain(ownedId);
    expect(after, "the host victim is gone").not.toContain(hostId);
    expect(after.length, "exactly one layer survives: the merged one").toBe(1);
    expect(names(engine)[0], "and it is the merged node").toMatch(/ \+ /);
  });

  it("mergeSelectedLayers with a facade-owned victim destroys that victim", async () => {
    const { engine, ownedId, hostId } = await openMixedPair();
    engine.mergeSelectedLayers([hostId, ownedId]);
    const after = ids(engine);
    expect(after, "the owned victim is gone").not.toContain(ownedId);
    expect(after, "the host victim is gone").not.toContain(hostId);
    expect(after.length, "exactly one layer survives: the merged one").toBe(1);
  });

  it("flattenLayers with a facade-owned victim destroys that victim", async () => {
    const { engine, ownedId, hostId } = await openMixedPair();
    engine.flattenLayers();
    const after = ids(engine);
    expect(after, "the owned victim is gone").not.toContain(ownedId);
    expect(after, "the host victim is gone").not.toContain(hostId);
    expect(after.length, "exactly one layer survives: the flattened one").toBe(1);
  });

  it("legacy delete CANNOT reach the owned-victim shape, so it needs no resurrection coverage", async () => {
    // Assessed, not assumed: a facade-owned layer cannot be legacy-deleted at
    // all - `deleteLayer` is one of the eleven mutators that refuse an
    // E_FACADE_OWNED id. The owned-victim shape this defect needs is therefore
    // unreachable through legacy delete, and the guard (which this fix must not
    // touch) is the correct result. This case pins that refusal so the reasoning
    // cannot rot: if delete ever stops refusing, this goes red and the
    // resurrection coverage it stood in for has to be written.
    const { engine, ownedId } = await openMixedPair();
    const before = ids(engine);
    expect(() => engine.deleteLayer(ownedId)).toThrow(/E_FACADE_OWNED/);
    expect(ids(engine), "a refused delete leaves the vector untouched").toEqual(before);
  });

  it("legacy delete of a HOST-created layer is not resurrected by the next sync", async () => {
    const { engine } = await openMixedPair();
    const doomed = addHostLayer(engine, "Doomed");
    engine.deleteLayer(doomed);
    const afterDelete = ids(engine);
    expect(afterDelete, "premise: the host-created layer is gone").not.toContain(doomed);
    // Force another sync.
    addHostLayer(engine, "Probe");
    const after = ids(engine);
    expect(after, "the deleted id stays deleted").not.toContain(doomed);
    expect(after.length, "only live ids remain").toBe(afterDelete.length + 1);
  });
});

describe("MIXED OWNERSHIP: the undo of a legacy merge restores the vector exactly", () => {
  it("ONE undo of a MIXED-ownership merge restores exactly the two pre-op ids", async () => {
    const { engine, ownedId, hostId, topId } = await openMixedPair();
    const history = new CommandHistory();
    history.attachDocIdGetter(() => engine.getId());
    const preOpIds = ids(engine).sort();
    expect(preOpIds, "premise: a mixed-ownership pair").toEqual([ownedId, hostId].sort());
    expect(isFacadeOwnedLayer(ownedId), "premise: one layer is owned").toBe(true);
    expect(isFacadeOwnedLayer(hostId), "premise: one layer is NOT owned").toBe(false);

    // The production path for a mixed selection: `resolveSelectionRoute` returns
    // "legacy", so `layerOperations.mergeActiveLayerDown` runs - `history.commit`
    // (mirrored into the facade as an External transition by the commit shim) and
    // then the engine arm.
    expect(
      mergeActiveLayerDown(engine as never, history, makeRenderer(), topId),
      "premise: the legacy merge-down ran",
    ).toBe(true);

    const postOp = ids(engine);
    const mergedId = postOp[0];
    expect(postOp, "premise: the merge left exactly the merged layer").toEqual([mergedId]);
    expect(hasFacadeOwnedLayers(), "the facade gate is open (owned ids persist)").toBe(true);
    // The commit shim records the transition FIRE-AND-FORGET, so settle before
    // reading the wire and before the undo - the undo steps the entry this creates.
    for (let i = 0; i < 8; i += 1) await new Promise((r) => setTimeout(r, 0));
    // The signal must actually reach the wire, or the undo has nothing to act on.
    const recordEnv = invokeMock.mock.calls
      .map((c) => String(c[1]?.envelopeJson ?? ""))
      .filter((j) => j.includes("recordExternalTransition"))
      .map(
        (j) =>
          JSON.parse(j).command as {
            minted_layer_ids?: string[];
            mintedLayerIds?: string[];
          },
      );
    expect(recordEnv.length, "an External transition was recorded").toBeGreaterThan(0);
    expect(
      // The wire key is snake_case: `toRustEnvelope` maps the command fields, and a
      // dropped key is swallowed by Rust's serde default with no error.
      recordEnv[recordEnv.length - 1].minted_layer_ids,
      `the recorded transition declares the minted destination ${mergedId}`,
    ).toEqual([mergedId]);

    await pressUndo(engine, history);

    // The LIST, never a count: "n=2" would also pass with the wrong two survivors.
    expect(
      ids(engine).sort(),
      `after one undo [${names(engine).join(",")}]`,
    ).toEqual(preOpIds);
  });

  it("a second merge/undo cycle on the SAME document is identical, with no destination carried over", async () => {
    const { engine, ownedId, hostId, topId } = await openMixedPair();
    const history = new CommandHistory();
    history.attachDocIdGetter(() => engine.getId());
    const preOpIds = ids(engine).sort();

    const cycles: string[][] = [];
    const destinationIds: string[] = [];
    const trace: string[] = [];
    for (let cycle = 0; cycle < 2; cycle += 1) {
      // The armed layer is the current top of the stack each time.
      const armed = ids(engine)[0];
      expect(
        mergeActiveLayerDown(engine as never, history, makeRenderer(), armed),
        `cycle ${cycle}: the legacy merge-down ran`,
      ).toBe(true);
      const merged = ids(engine);
      expect(merged.length, `cycle ${cycle}: the merge left exactly the merged layer`).toBe(1);
      destinationIds.push(merged[0]);
      // Settle BEFORE reading the declaration: the commit shim records fire-and-forget,
      // so the declared id is not on the wire yet. The vector was read first, above,
      // which is what the production caller does.
      for (let i = 0; i < 8; i += 1) await new Promise((r) => setTimeout(r, 0));
      // What the arm ACTUALLY declared, read off the serialized envelope rather than
      // assumed - so the assertion compares two observed facts, not an expectation
      // about one of them.
      const declared = (
        JSON.parse(
          String(
            invokeMock.mock.calls
              .map((c) => String(c[1]?.envelopeJson ?? ""))
              .filter((j) => j.includes("recordExternalTransition"))
              .slice(-1)[0] ?? "{}",
          ),
        ) as { command: { minted_layer_ids?: string[] } }
      ).command.minted_layer_ids;
      expect(
        declared?.[0],
        `cycle ${cycle}: the commit declared exactly one minted destination`,
      ).toBeTruthy();
      // THE ASSERTION THAT CATCHES IT AT CYCLE 0, not two steps downstream at the undo.
      // A declaration the engine ignored is worse than no declaration: it silently
      // disables the External undo's minted-layer exclusion, so the destination
      // survives its own undo. The tell is the id SHAPE - the engine's internal
      // fallback mints `layer-<uuid>` while the arm declares `layer-<base36>`.
      expect(
        merged[0],
        `cycle ${cycle}: the engine created the id the arm declared ` +
          `(declared ${declared?.[0]}, created ${merged[0]})`,
      ).toBe(declared?.[0]);
      trace.push(
        `cycle ${cycle} BEFORE undo: beforeMerge=[${armed}] dest=${merged[0]} ` +
          `tsCanUndo=${history.canUndo()} ${await cursorReading()}`,
      );
      const branch = await pressUndo(engine, history);
      const restored = ids(engine).sort();
      cycles.push(restored);
      trace.push(
        `cycle ${cycle} AFTER  undo: branch=${branch} restored=[${restored.join(",")}] ` +
          `tsCanUndo=${history.canUndo()} ${await cursorReading()}`,
      );
    }
    const diagnosis = trace.join("\n");

    // THE CONTRACT, per cycle: ONE undo restores the pre-merge set FOR THAT MERGE.
    // Stated per cycle rather than as "both cycles equal each other", because the
    // history state legitimately differs between them - the second merge is recorded
    // on top of the first entry the first undo already stepped, so the streams are not
    // the same depth. What must hold is that each undo restores ITS OWN pre-merge set.
    expect(cycles[0], `cycle 1 restores its own pre-merge set\n${diagnosis}`).toEqual(preOpIds);
    expect(cycles[1], `cycle 2 restores its own pre-merge set\n${diagnosis}`).toEqual(preOpIds);
    for (const dest of destinationIds) {
      expect(cycles[1], `destination ${dest} is gone after the undo`).not.toContain(dest);
    }
    expect(
      destinationIds[0] === destinationIds[1],
      `no destination id is carried into cycle 2: ${diagnosis}`,
    ).toBe(false);
    void ownedId;
    void hostId;
  });
});

describe("CONFIRMED UNAFFECTED: an op that destroys nothing records no victim", () => {
  it("duplicate adds a layer and resurrects nothing", async () => {
    const { engine, ownedId, hostId } = await openMixedPair();
    const before = ids(engine);
    const clone = engine.duplicateLayer(ownedId);
    const after = ids(engine);
    expect(after.length, "duplicate adds exactly one").toBe(before.length + 1);
    expect(after).toContain(ownedId);
    expect(after).toContain(hostId);
    expect(after, "the clone is the new id").toContain(clone.id);
  });

  it("reorder cannot resurrect a removed id", async () => {
    // Only the resurrection claim is under test. The exact post-reorder vector is
    // NOT asserted: an UNROUTED reorder under facade authority is a documented
    // pre-existing divergence (document.ts `reorderLayer`), unrelated to this
    // defect. What must hold is that the id a prior op removed does not come back.
    const { engine } = await openMixedPair();
    const doomed = addHostLayer(engine, "Doomed");
    engine.deleteLayer(doomed);
    expect(ids(engine), "premise: the deleted id is gone").not.toContain(doomed);
    engine.reorderLayer(0, 1);
    expect(ids(engine), "reorder does not resurrect the removed id").not.toContain(doomed);
  });
});
