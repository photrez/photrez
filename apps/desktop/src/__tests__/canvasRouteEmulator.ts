// Shared Tauri-invoke emulator for the canvas-route tests. It mirrors the native
// ProtocolEngine's document-size ownership: resizeCanvas and applyCrop move the
// stored document size and push a history entry; undo/redo restore the size and
// report it on the delta width/height, exactly like the walker in
// crates/core/src/document_core_apply.rs. Layer geometry is deliberately NOT
// modelled (the real-wasm parity matrix owns that); these tests pin the size
// plumbing only.
import type { Mock } from "vitest";

type InvokeMock = Mock<(cmd: string, args?: Record<string, unknown>) => Promise<unknown>>;
type DocSize = { w: number; h: number } | null;

export interface CanvasRouteEmulator {
  /** Every apply-command envelope as it arrived on the wire (snake_case fields). */
  wireEnvelopes: Array<{ command: Record<string, unknown> }>;
  reset(): void;
}

export function installCanvasRouteEmulator(invokeMock: InvokeMock): CanvasRouteEmulator {
  const open = new Set<string>();
  const version = new Map<string, number>();
  const docSize = new Map<string, DocSize>();
  const layers = new Map<string, Array<Record<string, unknown>>>();
  const history = new Map<string, Array<{ layers: Array<Record<string, unknown>>; size: DocSize }>>();
  const redo = new Map<string, Array<{ layers: Array<Record<string, unknown>>; size: DocSize }>>();
  const wireEnvelopes: Array<{ command: Record<string, unknown> }> = [];

  const result = (dv: number, changes: unknown[], size?: DocSize): string =>
    JSON.stringify({
      documentVersion: dv,
      delta: {
        baseVersion: Math.max(0, dv - 1),
        version: dv,
        changes,
        ...(size ? { width: size.w, height: size.h } : {}),
      },
      status: "ok",
    });

  invokeMock.mockImplementation(async (cmd, args = {}) => {
    const docId = (args.docId as string) ?? "default";
    const cur = () => version.get(docId) ?? 0;
    const size = () => docSize.get(docId) ?? null;
    const snapshotNodes = () => (layers.get(docId) ?? []).map((l) => ({ ...l }));
    const pushHistory = () => {
      if (!history.has(docId)) history.set(docId, []);
      history.get(docId)!.push({ layers: snapshotNodes(), size: size() });
    };
    switch (cmd) {
      case "rust_pixels_open_document":
        open.add(docId);
        return undefined;
      case "protocol_seed_native": {
        if (!open.has(docId)) throw `document not open: ${docId}`;
        const payload = JSON.parse((args.payloadJson as string) ?? "{}") as {
          version?: number;
          layers?: Array<Record<string, unknown>>;
        };
        version.set(docId, Number(payload.version ?? 0));
        layers.set(docId, (payload.layers ?? []).map((l) => ({ ...l })));
        history.set(docId, []);
        redo.set(docId, []);
        return JSON.stringify({ version: cur(), layers: layers.get(docId) });
      }
      case "protocol_seed_canonical_native":
      case "protocol_register_adapter_native":
        return null;
      case "protocol_apply_command_native": {
        if (!open.has(docId)) throw `document not open: ${docId}`;
        const env = JSON.parse((args.envelopeJson as string) ?? "{}") as {
          expectedVersion?: number;
          command?: Record<string, unknown>;
        };
        wireEnvelopes.push({ command: env.command ?? {} });
        const c = env.command ?? {};
        if (env.expectedVersion !== undefined && env.expectedVersion !== cur()) {
          throw `E_VERSION_MISMATCH: expected version ${env.expectedVersion} got ${cur()}`;
        }
        if (c.type === "resizeCanvas") {
          const w = Number(c.width);
          const h = Number(c.height);
          if (!(w > 0) || !(h > 0)) {
            version.set(docId, cur() + 1);
            return result(cur(), []);
          }
          pushHistory();
          docSize.set(docId, { w, h });
          version.set(docId, cur() + 1);
          return result(cur(), [], { w, h });
        }
        if (c.type === "applyCrop") {
          const w = Number(c.width);
          const h = Number(c.height);
          if (!(w > 0) || !(h > 0)) {
            version.set(docId, cur() + 1);
            return result(cur(), []);
          }
          const fw = typeof c.target_width === "number" ? c.target_width : w;
          const fh = typeof c.target_height === "number" ? c.target_height : h;
          pushHistory();
          docSize.set(docId, { w: fw, h: fh });
          version.set(docId, cur() + 1);
          return result(cur(), [], { w: fw, h: fh });
        }
        if (c.type === "undo" || c.type === "redo") {
          const from = c.type === "undo" ? history : redo;
          const to = c.type === "undo" ? redo : history;
          const stack = from.get(docId) ?? [];
          let dims: DocSize | undefined;
          if (stack.length > 0) {
            const prev = stack.pop()!;
            if (!to.has(docId)) to.set(docId, []);
            to.get(docId)!.push({ layers: snapshotNodes(), size: size() });
            layers.set(docId, prev.layers.map((l) => ({ ...l })));
            const before = size();
            docSize.set(docId, prev.size);
            if (JSON.stringify(before) !== JSON.stringify(prev.size)) dims = prev.size;
          }
          version.set(docId, cur() + 1);
          return result(cur(), [], dims);
        }
        version.set(docId, cur() + 1);
        return result(cur(), []);
      }
      case "protocol_snapshot_native": {
        if (!open.has(docId)) throw `document not open: ${docId}`;
        const s = size();
        return JSON.stringify({
          version: cur(),
          layers: snapshotNodes(),
          ...(s ? { width: s.w, height: s.h } : {}),
        });
      }
      case "protocol_version_native":
        if (!open.has(docId)) throw `document not open: ${docId}`;
        return cur();
      case "protocol_history_query_native":
        return JSON.stringify({ cursor: 0, lastSeq: 0, degradedHint: false, entries: [] });
      case "protocol_history_cursor_commit_native":
        version.set(docId, cur() + 1);
        return result(cur(), []);
      default:
        throw `E_UNKNOWN_COMMAND: ${cmd}`;
    }
  });

  return {
    wireEnvelopes,
    reset() {
      open.clear();
      version.clear();
      docSize.clear();
      layers.clear();
      history.clear();
      redo.clear();
      wireEnvelopes.length = 0;
    },
  };
}
