// EditorClient: migrated-op delegation with the DUAL-READ invariant.
//
// The facade (Rust canonical via the Command/delta contract) is authoritative for
// a migrated operation. This client is the single seam between the TS editor
// surface and the facade. It holds NO persistent document state of its own: it
// only projects the facade's RenderSnapshot into the TS engine (a read-only
// view). The TS engine's own mutation path is never entered for a migrated op.
//
// For a migrated op (Delete Layer pilot) the flow is:
//   1. route to the facade command (expectedVersion enforced inside EditorFacade),
//   2. apply the resulting RenderDelta to the facade's snapshot (projection), and
//   3. project the facade snapshot into the TS engine via applyFacadeSnapshot.
// The legacy TS path is NOT entered for a migrated op; it is only the flag-OFF /
// non-owned fallback (byte-identical).

import { EditorFacade } from "./editorFacade";
import type { RenderSnapshot } from "./types";
import { isFacadeEnabled } from "./facadeRegistry";
import { isFacadeOwnedLayer } from "@/engine/document";

export type MigratedDeleteStatus = "facade" | "legacy" | "blocked";

export type DeleteRouteResult = {
  status: MigratedDeleteStatus;
  snapshot: RenderSnapshot | null;
  /** Present on a fail-closed COMMAND throw (status==="blocked" — Rust Err /
   *  version conflict; the caller must surface it and NOT mutate) OR on a
   *  PROJECTION throw AFTER a successful command (status==="facade" — the facade
   *  already mutated; the caller must still destroy the texture). */
  error?: string;
};

/** Routing predicates, injectable so tests can drive flag/ownership
 *  deterministically without global localStorage side effects. */
export type DeleteRouting = {
  isFacadeEnabled(): boolean;
  isFacadeOwnedLayer(id: string): boolean;
};

const realRouting: DeleteRouting = {
  isFacadeEnabled,
  isFacadeOwnedLayer,
};

export interface EditorClientEngine {
  applyFacadeSnapshot(s: unknown): void;
}

export class EditorClient {
  constructor(
    private readonly engine: EditorClientEngine,
    private readonly facade: EditorFacade,
    private readonly routing: DeleteRouting = realRouting,
  ) {}

  /**
   * DUAL-READ boundary. Delegates the delete to the facade (Rust command ->
   * delta -> snapshot) and projects the resulting snapshot into the TS engine.
   * Returns {status:"legacy"} so the caller falls through to the byte-identical
   * legacy TS path when the migrated op does not apply (flag OFF / non-owned).
   * On a thrown command it FAILS CLOSED: no projection, no mutation, no history.
   */
  deleteLayer(id: string): DeleteRouteResult {
    if (!this.routing.isFacadeEnabled() || !this.routing.isFacadeOwnedLayer(id)) {
      return { status: "legacy", snapshot: null };
    }
    // Separate the command from the projection: a command throw is a genuine
    // failure (fail-closed), but a PROJECTION throw after a SUCCESSFUL command
    // must NOT be misreported as "blocked" (split-brain — the facade already
    // mutated). The facade command is authoritative; a projection compile/render
    // failure only means the TS view couldn't be updated, not that the delete
    // failed.
    let snap: RenderSnapshot;
    try {
      snap = this.facade.deleteLayer(id);
    } catch (e) {
      return {
        status: "blocked",
        snapshot: null,
        error: e instanceof Error ? e.message : String(e),
      };
    }
    // Ghost-layer guard: a facade command against an id Rust/emulation does NOT
    // know (e.g. a legacy-created layer that got seeded as facade-owned) returns
    // a no-op (empty delta) and the victim STAYS in the snapshot. Reporting
    // "facade" would make the caller destroy the texture of a still-present
    // layer. Surface it as blocked instead so the caller does NOT destroy.
    if (snap && snap.layers.some((l) => l.id === id)) {
      return {
        status: "blocked",
        snapshot: null,
        error: "E_NOOP_DELETE: facade left the layer present (unknown id)",
      };
    }
    let projectionError: string | undefined;
    try {
      if (snap) this.engine.applyFacadeSnapshot(snap as never);
    } catch (e) {
      projectionError = e instanceof Error ? e.message : String(e);
    }
    return { status: "facade", snapshot: snap, error: projectionError };
  }
}

export function createEditorClient(
  engine: EditorClientEngine,
  facade: EditorFacade,
): EditorClient {
  return new EditorClient(engine, facade);
}
