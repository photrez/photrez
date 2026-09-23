// Native-authority history determinism contract.
//
// Drives the production native path end to end:
//   EditorFacade -> bridge.applyCommand -> nativeProtocol -> Tauri invoke ->
//   protocol_apply_command_native / protocol_history_query_native /
//   protocol_history_cursor_commit_native
// and pins the invariants that make undo/redo deterministic:
//   1. Every committed transition (forward command, undo, redo, external cursor
//      commit) advances DocumentVersion by EXACTLY 1. In Rust: each history
//      entry arm does `version += 1` once (crates/core/src/history.rs
//      record_external/record_snapshot), apply() runs the single trailing
//      `self.version += 1` (crates/core/src/document_core_apply.rs), and an
//      external handoff returns EARLY without that bump - the deferred bump
//      happens only in history_cursor_commit (crates/core/src/history.rs).
//   2. A new forward command truncates the redo region before appending
//      (crates/core/src/document_core.rs begin_forward: entries.truncate).
//   3. A stale expectedVersion REJECTS. Tauri v2 invoke() rejects with a bare
//      "CODE: message" string on a Rust Err(String) - it never resolves with an
//      { ok: false } envelope (see nativeClient.ts header). The invoke mock
//      below reproduces that transport contract exactly; a mock that resolved
//      envelopes instead would fail the rejection assertions here.
//
// The Rust-side behavior the mock mirrors is pinned by cargo tests
// (native_apply_command_undo_redo_round_trip in protocol_native_cmds.rs,
// redo_region_truncated_by_new_forward_command in crates/core/src/history_h0_tests.rs).

import { describe, it, expect, vi, beforeEach, afterEach, type Mock } from "vitest";
import { invoke } from "@tauri-apps/api/core";
import {
  __resetNativeAuthorityForTests,
  applyCommand,
  createNativeSeed,
  getVersion,
} from "../bridge";
import {
  __resetFacadeRegistryForTests,
  confirmExternalCursor,
  getFacade,
  getHistoryProjection,
  historyDegraded,
  recordExternalTransitionFor,
} from "../facadeRegistry";
import { nativeProtocol } from "../nativeClient";
import { CONTRACT_VERSION, type Command, type RenderLayer } from "../types";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));

type InvokeArgs = Record<string, unknown>;
const invokeMock = invoke as unknown as Mock<(cmd: string, args?: InvokeArgs) => Promise<unknown>>;

type WireEnvelope = {
  contractVersion: number;
  expectedVersion?: number;
  command: { type: string } & Record<string, unknown>;
};

type HarnessEntry = {
  seq: number;
  origin: "native" | "external";
  label: string;
  versionBefore: number;
  versionAfter: number;
  before: RenderLayer[];
  after: RenderLayer[];
};

type HarnessDoc = {
  version: number;
  layers: RenderLayer[];
  entries: HarnessEntry[];
  cursor: number;
  nextSeq: number;
  pendingExternal: { seq: number; direction: string } | null;
  adapters: Set<string>;
};

type DeltaChange = { kind: "upsert"; layer: RenderLayer } | { kind: "remove"; id: string; resourceId: number };

// Test-only stand-in for the native command surface. State transitions follow
// the Rust engine's documented semantics (see header). Errors are thrown as
// bare strings so the mock rejects exactly the way Tauri v2 invoke() rejects on
// a Rust Err(String).
function installNativeHistoryHarness(): void {
  const docs = new Map<string, HarnessDoc>();

  const resolveKey = (docId: unknown): string =>
    typeof docId === "string" && docId !== "" ? docId : "default";

  const docOf = (docId: unknown): HarnessDoc => {
    const key = resolveKey(docId);
    const doc = docs.get(key);
    if (!doc) throw `document not open: ${key}`;
    return doc;
  };

  // Mirror of Rust diff_walker (crates/core/src/document_core.rs): Remove every
  // layer present in old but gone from new, then Upsert EVERY layer of new in
  // new's order, so the consumer rebuilds membership and order from the delta.
  const walkerDelta = (old: RenderLayer[], next: RenderLayer[]): DeltaChange[] => {
    const nextIds = new Set(next.map((l) => l.id));
    const changes: DeltaChange[] = [];
    for (const layer of old) {
      if (!nextIds.has(layer.id)) {
        changes.push({ kind: "remove", id: layer.id, resourceId: layer.resourceId });
      }
    }
    for (const layer of next) {
      changes.push({ kind: "upsert", layer });
    }
    return changes;
  };

  const resultOf = (
    doc: HarnessDoc,
    base: number,
    changes: DeltaChange[],
    status?: string,
    externalSeq?: number,
  ): string =>
    JSON.stringify({
      documentVersion: doc.version,
      delta: { baseVersion: base, version: doc.version, changes },
      ...(status !== undefined ? { status } : {}),
      ...(externalSeq !== undefined ? { externalSeq } : {}),
    });

  // New forward transition: truncate the redo region, append, advance version
  // exactly once (document_core.rs begin_forward + the trailing apply bump).
  const beginForward = (doc: HarnessDoc, label: string): HarnessEntry => {
    doc.entries = doc.entries.slice(0, doc.cursor);
    const seq = doc.nextSeq;
    doc.nextSeq += 1;
    const entry: HarnessEntry = {
      seq,
      origin: "native",
      label,
      versionBefore: doc.version,
      versionAfter: doc.version + 1,
      before: [...doc.layers],
      after: [],
    };
    doc.entries.push(entry);
    return entry;
  };

  const finishForward = (doc: HarnessDoc, entry: HarnessEntry): void => {
    entry.after = [...doc.layers];
    doc.cursor = doc.entries.length;
    doc.version += 1;
  };

  const barrierCheck = (doc: HarnessDoc): void => {
    if (doc.pendingExternal) {
      throw "E_EXTERNAL_PENDING: external history transition pending; commit cursor first";
    }
  };

  const applyEnvelope = (envelopeJson: string, docId: unknown): string => {
    let env: WireEnvelope;
    try {
      env = JSON.parse(envelopeJson) as WireEnvelope;
    } catch {
      throw "E_ENVELOPE_PARSE: invalid command envelope JSON";
    }
    if (env.contractVersion !== CONTRACT_VERSION) {
      throw `E_CONTRACT_VERSION: expected ${CONTRACT_VERSION} got ${env.contractVersion}`;
    }
    const doc = docOf(docId);
    if (env.expectedVersion !== undefined && env.expectedVersion !== doc.version) {
      throw `E_VERSION_MISMATCH: expected ${doc.version} got ${env.expectedVersion}`;
    }
    const base = doc.version;
    switch (env.command.type) {
      case "addLayer": {
        barrierCheck(doc);
        const c = env.command as unknown as { id: string; name: string; width: number; height: number; index: number };
        const entry = beginForward(doc, `Add ${c.name}`);
        const layer: RenderLayer = {
          id: c.id,
          name: c.name,
          visible: true,
          opacity: 1,
          resourceId: 0,
          x: 0,
          y: 0,
          scaleX: 1,
          scaleY: 1,
          rotation: 0,
          width: c.width,
          height: c.height,
        };
        const at = Math.max(0, Math.min(c.index, doc.layers.length));
        doc.layers.splice(at, 0, layer);
        finishForward(doc, entry);
        return resultOf(doc, base, [{ kind: "upsert", layer }]);
      }
      case "recordExternalTransition": {
        barrierCheck(doc);
        // Nested command fields travel snake_case on the wire (bridge.toRustEnvelope
        // maps adapterId -> adapter_id).
        const c = env.command as unknown as { label: string; adapter_id: string };
        if (c.adapter_id !== "native" && !doc.adapters.has(c.adapter_id)) {
          throw `E_UNKNOWN_ADAPTER: adapter '${c.adapter_id}' is not registered`;
        }
        doc.entries = doc.entries.slice(0, doc.cursor);
        const seq = doc.nextSeq;
        doc.nextSeq += 1;
        doc.entries.push({
          seq,
          origin: "external",
          label: c.label,
          versionBefore: base,
          versionAfter: base + 1,
          before: [...doc.layers],
          after: [],
        });
        doc.cursor = doc.entries.length;
        doc.version += 1;
        return resultOf(doc, base, [], "external-recorded", seq);
      }
      case "undo": {
        barrierCheck(doc);
        const old = doc.layers;
        if (doc.cursor === 0) {
          // Empty-stack undo is a NO-OP success; the version still bumps.
          doc.version += 1;
          return resultOf(doc, base, []);
        }
        const entry = doc.entries[doc.cursor - 1];
        if (entry.origin === "external") {
          // Host handoff: restore the layer vector now, but the cursor AND the
          // version move only when the host confirms via cursor commit.
          doc.pendingExternal = { seq: entry.seq, direction: "undo" };
          doc.layers = [...entry.before];
          return resultOf(doc, base, walkerDelta(old, doc.layers), "external", entry.seq);
        }
        doc.cursor -= 1;
        doc.layers = [...entry.before];
        doc.version += 1;
        return resultOf(doc, base, walkerDelta(old, doc.layers));
      }
      case "redo": {
        barrierCheck(doc);
        const old = doc.layers;
        if (doc.cursor >= doc.entries.length) {
          // Tip redo is a NO-OP success; the version still bumps.
          doc.version += 1;
          return resultOf(doc, base, []);
        }
        const entry = doc.entries[doc.cursor];
        if (entry.origin === "external") {
          // Host handoff: the up-projected side lives in the entry once the
          // host's canonical push recorded it (after: [] until then).
          doc.pendingExternal = { seq: entry.seq, direction: "redo" };
          doc.layers = [...entry.after];
          return resultOf(doc, base, walkerDelta(old, doc.layers), "external", entry.seq);
        }
        doc.cursor += 1;
        doc.layers = [...entry.after];
        doc.version += 1;
        return resultOf(doc, base, walkerDelta(old, doc.layers));
      }
      default:
        throw `E_UNKNOWN_COMMAND: ${env.command.type}`;
    }
  };

  invokeMock.mockImplementation(async (cmd: string, args: InvokeArgs = {}) => {
    const key = resolveKey(args.docId);
    switch (cmd) {
      case "rust_pixels_open_document": {
        docs.set(key, {
          version: 0,
          layers: [],
          entries: [],
          cursor: 0,
          nextSeq: 1,
          pendingExternal: null,
          adapters: new Set<string>(),
        });
        return null;
      }
      case "protocol_seed_native": {
        const doc = docOf(key);
        const payload = JSON.parse(String(args.payloadJson ?? "{}")) as {
          version: number;
          layers: RenderLayer[];
        };
        // Seed is only-when-empty in Rust; the TS caller is idempotent too.
        if (doc.entries.length === 0 && doc.version === 0 && doc.layers.length === 0) {
          doc.version = payload.version;
          doc.layers = [...payload.layers];
        }
        return "null";
      }
      case "protocol_register_adapter_native": {
        const doc = docOf(key);
        const adapterId = String(args.adapterId ?? "");
        if (adapterId !== "native") doc.adapters.add(adapterId);
        return "null";
      }
      case "protocol_version_native":
        return docOf(key).version;
      case "protocol_snapshot_native": {
        const doc = docOf(key);
        return JSON.stringify({ version: doc.version, layers: doc.layers });
      }
      case "protocol_layer_ids_native":
        return JSON.stringify(docOf(key).layers.map((l) => l.id));
      case "protocol_history_query_native": {
        const doc = docOf(key);
        return JSON.stringify({
          cursor: doc.cursor,
          lastSeq: doc.nextSeq - 1,
          degradedHint: false,
          ...(doc.pendingExternal ? { pendingExternal: doc.pendingExternal } : {}),
          entries: doc.entries.map((e) => ({
            seq: e.seq,
            groupId: e.seq,
            origin: e.origin === "native" ? "native" : "external:ts-external",
            label: e.label,
            affectedLayerIds: [] as string[],
            versionBefore: e.versionBefore,
            versionAfter: e.versionAfter,
            memoryCostBytes: 0,
            payloadRef: e.origin === "external" ? "ext-token" : null,
          })),
        });
      }
      case "protocol_apply_command_native":
        return applyEnvelope(String(args.envelopeJson ?? ""), key);
      case "protocol_history_cursor_commit_native": {
        const doc = docOf(key);
        const seq = Number(args.seq);
        const direction = String(args.direction ?? "");
        const pending = doc.pendingExternal;
        if (!pending || pending.seq !== seq || pending.direction !== direction) {
          throw `E_CURSOR_MISMATCH: cursor ${doc.cursor} pending_external ${JSON.stringify(pending)} incompatible with seq ${seq} direction ${direction}`;
        }
        const base = doc.version;
        if (direction === "undo") doc.cursor -= 1;
        else doc.cursor += 1;
        doc.version += 1;
        doc.pendingExternal = null;
        return resultOf(doc, base, [], "external-confirmed");
      }
      default:
        throw `E_UNKNOWN_COMMAND: ${cmd}`;
    }
  });
}

async function seedDoc(docId: string) {
  await createNativeSeed(docId, 0, []);
  return getFacade(docId);
}

describe("native history determinism (idle -> addLayer -> committed -> undo -> redo -> cleanup)", () => {
  beforeEach(() => {
    localStorage.clear();
    localStorage.setItem("photrez.facade", "1");
    localStorage.setItem("photrez.facadeAuthority", "native");
    invokeMock.mockReset();
    __resetNativeAuthorityForTests();
    __resetFacadeRegistryForTests();
    installNativeHistoryHarness();
  });

  afterEach(() => {
    __resetFacadeRegistryForTests();
    __resetNativeAuthorityForTests();
    invokeMock.mockReset();
    localStorage.clear();
  });

  it("advances DocumentVersion by exactly 1 per transition and round-trips the cursor", async () => {
    const docId = "histDeterminism";
    const facade = await seedDoc(docId);

    // idle: nothing committed yet.
    expect(await getVersion(docId)).toBe(0);
    const idle = await getHistoryProjection(docId);
    expect(idle.cursor).toBe(0);
    expect(idle.entries).toHaveLength(0);

    // idle -> addLayer: one forward entry committed at the cursor, version +1.
    const vIdle = await getVersion(docId);
    const snap = await facade.addLayer("Layer A");
    expect(await getVersion(docId)).toBe(vIdle + 1);
    expect(facade.renderedVersion).toBe(vIdle + 1);
    expect(snap.layers.map((l) => l.name)).toEqual(["Layer A"]);
    const committed = await getHistoryProjection(docId);
    expect(committed.cursor).toBe(1);
    expect(committed.entries).toHaveLength(1);
    expect(committed.entries[0].seq).toBe(1);
    expect(committed.entries[0].versionAfter - committed.entries[0].versionBefore).toBe(1);
    expect(committed.lastSeq).toBe(1);

    // committed -> undo: version +1, entry kept so redo can replay it.
    const vCommitted = await getVersion(docId);
    await facade.undo();
    expect(await getVersion(docId)).toBe(vCommitted + 1);
    expect(facade.renderedVersion).toBe(vCommitted + 1);
    const undone = await getHistoryProjection(docId);
    expect(undone.cursor).toBe(0);
    expect(undone.entries).toHaveLength(1);

    // undo -> redo: version +1, cursor returns to the tip, layer restored.
    const vUndone = await getVersion(docId);
    await facade.redo();
    expect(await getVersion(docId)).toBe(vUndone + 1);
    expect(facade.renderedVersion).toBe(vUndone + 1);
    const redone = await getHistoryProjection(docId);
    expect(redone.cursor).toBe(1);
    expect(redone.entries).toHaveLength(1);
    expect(facade.snapshot.layers.map((l) => l.name)).toEqual(["Layer A"]);

    // cleanup: no barrier left open, no degraded markers, no unrecorded tokens.
    expect(redone.pendingExternal ?? null).toBeNull();
    expect(redone.degraded).toBe(false);
    expect(redone.degradedHint).toBe(false);
    expect(redone.unrecordedTokens).toEqual([]);
    expect(historyDegraded()).toBeNull();
    expect(facade.lastExternalHandoff).toBeNull();
  });

  it("drops the redo branch when a new forward command commits", async () => {
    const docId = "histTruncate";
    const facade = await seedDoc(docId);
    await facade.addLayer("Layer A");
    await facade.undo(); // redo pending: entries [A], cursor at 0

    const vBefore = await getVersion(docId);
    await facade.addLayer("Layer B");
    expect(await getVersion(docId)).toBe(vBefore + 1);
    const q = await getHistoryProjection(docId);
    expect(q.entries).toHaveLength(1); // A's redo slot truncated, not appended after
    expect(q.entries[0].seq).toBe(2); // seq keeps advancing, never reused
    expect(q.lastSeq).toBe(2);
    expect(q.cursor).toBe(1);

    // redo is now inert: nothing left to replay, version still advances +1.
    const vTrunc = await getVersion(docId);
    await facade.redo();
    expect(await getVersion(docId)).toBe(vTrunc + 1);
    expect(facade.snapshot.layers.map((l) => l.name)).toEqual(["Layer B"]);
    const after = await getHistoryProjection(docId);
    expect(after.entries).toHaveLength(1);
    expect(after.cursor).toBe(1);
  });

  it("rejects a stale expectedVersion without applying anything", async () => {
    const docId = "histStale";
    const facade = await seedDoc(docId);
    await facade.addLayer("Layer A");

    const beforeVersion = await getVersion(docId);
    const beforeLayers = facade.snapshot.layers.length;
    const beforeEntries = (await getHistoryProjection(docId)).entries.length;

    const stale: Command = { type: "addLayer", id: "stale-layer", name: "stale", width: 10, height: 10, index: 0 };
    await expect(
      applyCommand({ contractVersion: CONTRACT_VERSION, expectedVersion: 999, docId, command: stale }),
    ).rejects.toThrow(/^E_VERSION_MISMATCH/);

    // Rejected, not applied: version, history entries, and projection untouched.
    expect(await getVersion(docId)).toBe(beforeVersion);
    expect((await getHistoryProjection(docId)).entries).toHaveLength(beforeEntries);
    expect(facade.snapshot.layers).toHaveLength(beforeLayers);

    // Not wedged either: the next command with a matching version commits.
    await facade.addLayer("Layer B");
    expect(await getVersion(docId)).toBe(beforeVersion + 1);
  });

  it("surfaces Rust Err(String) as a bare CODE: message rejection, never an ok envelope", async () => {
    const docId = "histTransport";
    await seedDoc(docId);

    // nativeClient returns the RAW invoke promise: the rejection value is the
    // bare string the Rust command produced (nativeClient.ts:3-9).
    const raw = await nativeProtocol
      .protocol_apply_command_native("not-json", docId)
      .then(() => null, (e: unknown) => e);
    expect(typeof raw).toBe("string");
    expect(raw).toMatch(/^E_ENVELOPE_PARSE:/);

    // bridge.applyCommand normalizes that string into an Error carrying the same
    // CODE: message - never "[object Object]", never a JSON envelope round-trip.
    const wrapped = await applyCommand({
      contractVersion: CONTRACT_VERSION,
      expectedVersion: 999,
      docId,
      command: { type: "addLayer", id: "stale", name: "stale", width: 10, height: 10, index: 0 },
    }).then(() => null, (e: unknown) => e);
    expect(wrapped).toBeInstanceOf(Error);
    const message = (wrapped as Error).message;
    expect(message).toMatch(/^E_VERSION_MISMATCH: /);
    expect(message).not.toContain("[object Object]");
    expect(message.startsWith("{")).toBe(false);

    // Success side resolves a JSON STRING (Rust Ok(String)) that applyCommand
    // parses into a CommandResult. A transport that resolved an { ok: false }
    // envelope instead fails both assertions below.
    const ok = await applyCommand({
      contractVersion: CONTRACT_VERSION,
      expectedVersion: 0,
      docId,
      command: { type: "addLayer", id: "layer-ok", name: "ok", width: 10, height: 10, index: 0 },
    });
    expect(ok.documentVersion).toBe(1);
    expect((ok as { ok?: unknown }).ok).toBeUndefined();
  });

  it("confirms an external handoff with exactly one version bump and a round-tripped cursor", async () => {
    const docId = "histExternal";
    const facade = await seedDoc(docId);
    await facade.addLayer("Layer A"); // version 1, cursor 1

    const rec = await recordExternalTransitionFor(docId, {
      label: "Legacy Edit",
      affectedLayerIds: [],
      snapshot: { layers: [] },
    });
    expect(rec.ok).toBe(true);
    expect(rec.seq).toBe(2);
    expect(await getVersion(docId)).toBe(2);

    // undo lands on the external entry: handoff only. apply() returns early with
    // status "external" and does NOT run its trailing version bump; the cursor
    // also stays put until the host confirms.
    await facade.undo();
    expect(facade.lastExternalHandoff).not.toBeNull();
    expect(facade.lastExternalHandoff?.direction).toBe("undo");
    expect(await getVersion(docId)).toBe(2);
    const pending = await getHistoryProjection(docId);
    expect(pending.pendingExternal).toEqual({ seq: rec.seq, direction: "undo" });
    expect(pending.cursor).toBe(2);

    // cursor commit: the ONE bump of this transition, cursor steps back,
    // barrier clears.
    const res = await confirmExternalCursor(docId, rec.seq ?? 0, "undo");
    expect(res.ok).toBe(true);
    expect(await getVersion(docId)).toBe(3);
    expect(facade.renderedVersion).toBe(3);
    const done = await getHistoryProjection(docId);
    expect(done.cursor).toBe(1);
    expect(done.pendingExternal ?? null).toBeNull();
    expect(done.degraded).toBe(false);
    expect(historyDegraded()).toBeNull();
  });
});
