// SPDX-License-Identifier: AGPL-3.0-or-later

/**
 * The native protocol boundary, emulated for jsdom component tests.
 *
 * WHY THIS EXISTS. A document factory (blank canvas, preset, new tab, plain
 * image) seeds the native engine through its background-flag commit, and the
 * open now REFUSES to install a document whose seed was refused - so a test
 * that drives a real open affordance has to answer that boundary or it is
 * exercising "the seed was refused" rather than the affordance.
 *
 * Every rule here is the real command's observable contract, not a convenience:
 *  - `protocol_seed_native` stores exactly the layer ids it was given, so a
 *    later `protocol_layer_ids_native` answers with those and nothing else. A
 *    stub answering "no layers" made the ACCEPTING path fail for a reason the
 *    product does not have, which is how a mock hid a real defect once already.
 *  - `protocol_apply_command_native` bumps documentVersion on every accepted
 *    apply, rejects a stale `expectedVersion`, flags a KNOWN id
 *    (isBackground + position/rotation locks) and accepts an UNKNOWN id with an
 *    empty delta - the native arm has no unknown-layer error branch, which is
 *    why the host probes membership separately.
 *  - an unseeded/unopened document rejects every command, the way the Rust
 *    registry does.
 *  - `refuseSeed()` makes the seed REJECT with the bare string a Rust
 *    `Err(String)` produces through Tauri v2 `invoke`. It never resolves an
 *    error-shaped value: that wrong contract is what let the seed-refusal hole
 *    sit behind a green suite.
 *
 * Test-only, lives under `__tests__`, so the static writer census skips it.
 */
export interface NativeProtocolEmulator {
  /** Drop the `nativeProtocol` object shape `@/lib/protocol/nativeClient` exports. */
  nativeProtocol: Record<string, (...args: never[]) => Promise<unknown>>;
  /** Forget every seeded document. Call between cases. */
  reset(): void;
  /** Make every later seed reject with the bare string a Rust `Err` produces. */
  refuseSeed(message?: string): void;
  /** Accept seeds again (the default after `reset`). */
  acceptSeed(): void;
  /** The layer ids the emulated native engine currently holds for a document. */
  layerIds(docId: string): string[];
}

export function createNativeProtocolEmulator(): NativeProtocolEmulator {
  interface Doc {
    version: number;
    layers: Map<string, Record<string, unknown>>;
  }
  const docs = new Map<string, Doc>();
  let refusing = false;

  const key = (docId: string) => (docId === "" ? "default" : docId);
  const docOf = (docId: string): Doc | undefined => docs.get(key(docId));
  const cmdResult = (dv: number, changes: unknown[] = []) =>
    JSON.stringify({
      documentVersion: dv,
      delta: { baseVersion: Math.max(0, dv - 1), version: dv, changes },
      status: "ok",
    });

  const nativeProtocol = {
    protocol_seed_native: async (payloadJson: string, docId: string) => {
      if (refusing) throw "E_SEED: canonical seed refused";
      const payload = JSON.parse(payloadJson) as {
        version: number;
        layers: Array<Record<string, unknown>>;
      };
      docs.set(key(docId), {
        version: payload.version,
        layers: new Map(payload.layers.map((l) => [String(l.id), { ...l }])),
      });
      return JSON.stringify({ version: payload.version });
    },
    protocol_apply_command_native: async (envelopeJson: string, docId: string) => {
      const doc = docOf(docId);
      if (!doc) throw `document not open: ${docId}`;
      const env = JSON.parse(envelopeJson) as {
        expectedVersion?: number;
        command?: { type?: string; id?: string };
      };
      if (env.expectedVersion !== undefined && env.expectedVersion !== doc.version) {
        throw `E_VERSION_MISMATCH: expected version ${env.expectedVersion} got ${doc.version}`;
      }
      const command = env.command ?? {};
      doc.version += 1;
      if (command.type !== "setBackgroundFlag") return cmdResult(doc.version);
      const layer = command.id ? doc.layers.get(command.id) : undefined;
      if (!layer) return cmdResult(doc.version);
      Object.assign(layer, { isBackground: true, lockPosition: true, lockRotation: true });
      return cmdResult(doc.version, [{ kind: "upsert", layer: { ...layer } }]);
    },
    protocol_layer_ids_native: async (docId: string) => {
      const doc = docOf(docId);
      if (!doc) throw `document not open: ${docId}`;
      return JSON.stringify([...doc.layers.keys()]);
    },
    protocol_snapshot_native: async (docId: string) => {
      const doc = docOf(docId);
      if (!doc) throw `document not open: ${docId}`;
      return JSON.stringify({ version: doc.version, layers: [...doc.layers.values()] });
    },
    protocol_version_native: async (docId: string) => {
      const doc = docOf(docId);
      if (!doc) throw `document not open: ${docId}`;
      return doc.version;
    },
    protocol_seed_canonical_native: async () => JSON.stringify({ ok: true }),
    protocol_canonical_native: async () => JSON.stringify({ ok: true }),
    protocol_register_adapter_native: async () => JSON.stringify({ ok: true }),
    protocol_history_query_native: async () => JSON.stringify({ cursor: 0, entries: [] }),
    protocol_history_cursor_commit_native: async () => JSON.stringify({ ok: true }),
  };

  return {
    nativeProtocol: nativeProtocol as unknown as NativeProtocolEmulator["nativeProtocol"],
    reset: () => {
      docs.clear();
      refusing = false;
    },
    refuseSeed: (message = "E_SEED: canonical seed refused") => {
      refusing = true;
      void message;
    },
    acceptSeed: () => {
      refusing = false;
    },
    layerIds: (docId) => [...(docOf(docId)?.layers.keys() ?? [])],
  };
}
