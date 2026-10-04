// SPDX-License-Identifier: AGPL-3.0-or-later

/**
 * Snapshot-token payloads and the token re-attach that consumes them.
 *
 * Extracted from `history.ts` so the payload DTOs, the identity gates and the
 * re-attach's failure reporting sit in one file instead of two hundred lines of
 * the history class. `CommandHistory` still OWNS the payloads it builds
 * (`buildSnapshotPayload`); it imports the DTOs from here so the two ends cannot
 * disagree about a field.
 */
import { syncFacadeVersionFromPixel } from "@/lib/protocol/facadeRegistry";
import { getSnapshot } from "@/lib/protocol/bridge";
import type { BitmapField } from "./bitmapStore";
import { historyBridgeEnabled } from "./historyBridgeGate";

// ── Snapshot-token payloads (mirror the rust core DocumentSnapshot/LayerSnapshot
//    DTOs so the Tauri commands round-trip 1:1, camelCase on the wire) ──

/** One layer's metadata + opaque bitmap token in a document snapshot. */
export interface SnapshotLayerMeta {
  layerId: string;
  width: number;
  height: number;
  bitmapToken: string | null;
  epoch: number;
  pixelVersion: number;
}

/** A document's atomic metadata + bitmap-token snapshot (no pixels). */
export interface SnapshotPayload {
  docId: string;
  version: number;
  layers: SnapshotLayerMeta[];
}

/**
 * The restored model's per-layer identity facts the re-attach verifies against
 * BEFORE applying any bitmap. Supplied (bridge-ON, snapshot-typed) by the
 * undo/redo caller from the snapshot being restored. `epoch`/`pixelVersion`
 * mirror what `buildSnapshotPayload` recorded at record time; the bitmaps are
 * the exact objects the restored model currently holds, so a payload whose
 * token resolves to a DIFFERENT object is provably not this step's snapshot.
 */
export interface SnapshotLayerIdentity {
  /** Expected epoch for this layer (the restored model's bitmapEpoch ?? 0). */
  epoch?: number;
  /** Expected pixelVersion for this layer (falls back to epoch when absent). */
  pixelVersion?: number;
  /** The restored model's current primary imageBitmap for this layer. */
  imageBitmap?: ImageBitmap | null;
  /** The restored model's current baseImageBitmap for this layer. */
  baseImageBitmap?: ImageBitmap | null;
  /** The restored model's current layer width in px (size gate). */
  width?: number;
  /** The restored model's current layer height in px (size gate). */
  height?: number;
}

/**
 * Re-attach the layer ImageBitmap(s) for a snapshot undo/redo step by token,
 * WITHOUT ever detaching a still-valid bitmap. This is the UX-critical
 * re-attach: it is called (bridge ON) after `engine.restore(snapshot)`, and it
 * asks the Rust snapshot cursor for the `before` (undo) or `after` (redo)
 * snapshot, then resolves each layer's `bitmapToken` from the BitmapStore and
 * re-assigns the SAME ImageBitmap object to the live layer.
 *
 * No-detach contract:
 *   - a token that resolves to a bitmap -> the EXACT same object is re-attached;
 *   - a token that resolves to null -> the existing bitmap is left untouched and
 *     the miss is surfaced (console.warn), never a silent detach;
 *   - the Rust command returning None (Ok) or rejecting (Err) -> the bridge did
 *     not own this entry (tip is not a Snapshot), so this returns false and the
 *     caller proceeds with the existing Model-A restore.
 *
 * Returns true when this helper re-attached at least one bitmap (caller may then
 * skip redundant work, though re-uploading is harmless).
 *
 * Identity gate: the payload is verified against this doc's restored model
 * before any bitmap is applied. If the returned snapshot's `docId` differs, or
 * (when `currentLayerIds` is supplied) its layer-id set no longer matches the
 * restored model, the re-attach is SKIPPED (returns false) and the Model-A
 * restore stays authoritative — never a wrong-step bitmap.
 *
 * Finding-B hardening (per-layer epoch/pixelVersion/token identity): a future
 * Snapshot producer that MUTATES a surviving layer's bitmap (e.g. an adjustment
 * bake) can emit step snapshots whose layer-id sets coincide with a different
 * step's payload. `currentLayerIds` alone cannot then tell them apart. When
 * `currentLayerMeta` is supplied, each payload layer's `epoch`/`pixelVersion`
 * MUST match the restored model's expected values, AND each token must resolve
 * to the EXACT bitmap object the restored model currently holds for that layer's
 * field. A payload that fails either check is SKIPPED (returns false + warn),
 * so a coincidentally-matching layer set can never silently attach the wrong
 * bitmap. The contiguous Delete Layer case is unaffected: its surviving layers
 * keep stable, consistent epochs and SAME bitmap objects across steps.
 *
 * Size gate: when `currentLayerMeta` supplies the restored model's layer
 * `width`/`height`, a payload bitmap whose pixel size disagrees with those
 * model-owned dims is SKIPPED for that layer (warn + continue) - a
 * size-mismatched bitmap would render against dims it does not match. Only the
 * swap is skipped; dims are never written here (dims stay model-owned). A null
 * token resolution (missing path) keeps its existing warn-and-keep behavior.
 */
export async function restoreSnapshotBitmapsByToken(
  direction: "undo" | "redo",
  docId: string,
  resolve: (token: string) => ImageBitmap | null,
  set: (layerId: string, bitmap: ImageBitmap, field?: BitmapField) => boolean,
  fieldFor?: (token: string) => BitmapField | null,
  currentLayerIds?: () => Iterable<string>,
  currentLayerMeta?: (layerId: string) => SnapshotLayerIdentity | null,
): Promise<boolean> {
  // Gated on `historyBridgeEnabled()` ALONE, where `commit` / `recordSnapshotHistory`
  // gate on `bridgeRecordsFor()` (flag AND a doc-id getter). Tighter would be WRONG
  // here: a document's `Snapshot` entries can come from the NATIVE WALKER rather than
  // this history, so a getterless history can still own a Snapshot step to undo. Nor
  // can the looser gate over-step: with no Snapshot at the cursor the command answers
  // `Ok(None)` without moving (`rust_pixels_undo_snapshot`,
  // document_snapshot_cmds.rs:224-225), and the identity gates below drop a payload
  // this step did not produce.
  if (!historyBridgeEnabled()) return false;
  let snapshot: SnapshotPayload | null = null;
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    const snap = (await invoke<SnapshotPayload | null>(
      direction === "undo" ? "rust_pixels_undo_snapshot" : "rust_pixels_redo_snapshot",
      { docId },
    )) as SnapshotPayload | null;
    snapshot = snap;
    // Native-authority version sync: the snapshot cursor advances the native
    // engine document version; push it into the facade so a later facade command
    // is not rejected with E_VERSION_MISMATCH. Gated no-op unless native active.
    // The snapshot cursor's returned payload carries its STORED `version` field
    // (TS always records version:0), NOT the engine document version, so reading
    // `snap.version` here would be a dead up-only no-op. Source the authoritative
    // engine DV from getSnapshot() (protocol_snapshot_native returns the live
    // engine documentVersion) instead.
    if (snap) {
      const engSnap = await getSnapshot(docId);
      syncFacadeVersionFromPixel(docId, engSnap.version);
    }
  } catch (err) {
    // Tauri v2 invoke REJECTS with an error-envelope object on a Rust Err, and the
    // snapshot cursor is best-effort - a failure must never break undo/redo. But
    // best-effort is not silent: this is the snapshot counterpart of the pixel step,
    // and a step that never ran is what `[history-cursor-step] the Rust cursor step
    // was rejected` now reports for its sibling. Reachable under facade+bridge via
    // `external_barrier_check` (crates/core/src/history.rs:386, :412).
    console.warn("[history-cursor-step] the Rust SNAPSHOT cursor step was rejected", {
      docId,
      direction,
      error: String(err),
    });
    return false;
  }
  if (!snapshot || !snapshot.layers || snapshot.layers.length === 0) return false;
  // Finding 2 (identity assertion): the Rust snapshot cursor can, under cursor
  // drift (or a mixed stream), return a DIFFERENT Snapshot's payload for this
  // doc. Re-attaching that payload would overwrite the Model-A restore with the
  // wrong-step bitmap. Before applying, verify the payload is actually for THIS
  // doc's restored model: the docId must match, and (when the live layer-id set
  // is supplied) the payload's layer-id set must match the restored model's.
  // On any mismatch we SKIP the re-attach (return false) so the Model-A restore
  // stays authoritative — never fail-open into a wrong-step bitmap.
  if (snapshot.docId !== docId) {
    console.warn("[history] snapshot payload docId mismatch; skipping re-attach", {
      expected: docId,
      actual: snapshot.docId,
    });
    return false;
  }
  if (currentLayerIds) {
    const modelLayerIds = new Set(currentLayerIds());
    const snapLayerIds = new Set(snapshot.layers.map((l) => l.layerId));
    const sameLayerSet =
      modelLayerIds.size === snapLayerIds.size &&
      [...snapLayerIds].every((id) => modelLayerIds.has(id));
    if (!sameLayerSet) {
      console.warn("[history] snapshot layer-set mismatch; skipping re-attach", {
        docId,
        payload: [...snapLayerIds],
        model: [...modelLayerIds],
      });
      return false;
    }
  }
  // Finding-B per-layer identity gate. When `currentLayerMeta` is supplied, the
  // restored model's per-layer epoch/pixelVersion (and current bitmap objects)
  // are the ground truth for THIS step. A payload built from a DIFFERENT step of
  // a future bitmap-mutating producer can share the docId AND the layer-id set,
  // but its per-layer epoch/pixelVersion (and the bitmap its token resolves to)
  // will differ. Verify every payload layer before any set — fail-closed.
  let metaById: Map<string, SnapshotLayerIdentity | null> | null = null;
  if (currentLayerMeta) {
    metaById = new Map();
    for (const layer of snapshot.layers) {
      const meta = currentLayerMeta(layer.layerId);
      if (!meta) {
        console.warn("[history] snapshot layer absent from restored model; skipping re-attach", {
          docId,
          layerId: layer.layerId,
        });
        return false;
      }
      const expEpoch = meta.epoch ?? 0;
      const expPixel = meta.pixelVersion ?? expEpoch;
      if (layer.epoch !== expEpoch || layer.pixelVersion !== expPixel) {
        console.warn("[history] snapshot layer epoch/pixelVersion mismatch; skipping re-attach", {
          docId,
          layerId: layer.layerId,
          payloadEpoch: layer.epoch,
          payloadPixelVersion: layer.pixelVersion,
          expectedEpoch: expEpoch,
          expectedPixelVersion: expPixel,
        });
        return false;
      }
      metaById.set(layer.layerId, meta);
    }
  }
  let applied = false;
  for (const layer of snapshot.layers) {
    if (!layer.bitmapToken) continue;
    const bitmap = resolve(layer.bitmapToken);
    if (bitmap) {
      // Hazard #3: re-attach to the SAME layer field the token was registered
      // from (imageBitmap vs baseImageBitmap). When no fieldFor resolver is
      // supplied (the isolated-helper unit call) the set callback receives the
      // legacy 2-arg form, so existing consumers/tests are unaffected.
      const field = layer.bitmapToken && fieldFor ? fieldFor(layer.bitmapToken) : null;
      // Finding-B token→bitmap identity: the token must resolve to the EXACT
      // bitmap the restored model currently holds for this layer's field. A
      // coincidental layer-set + epoch match cannot make a wrong-step payload
      // resolve to the model's own bitmap object unless it truly is the model's
      // bitmap — so a mismatch (including the model holding null while the
      // payload claims a bitmap) is provably the wrong step. Skip fail-closed.
      if (metaById) {
        const meta = metaById.get(layer.layerId);
        const modelBitmap = meta
          ? field === "baseImageBitmap"
            ? meta.baseImageBitmap ?? null
            : meta.imageBitmap ?? null
          : null;
        if (bitmap !== modelBitmap) {
          console.warn("[history] snapshot token resolves to a different bitmap than restored model; skipping re-attach", {
            docId,
            layerId: layer.layerId,
            field: field ?? "imageBitmap",
          });
          return false;
        }
        // Size gate: the payload bitmap's pixel size must agree with the
        // restored model's layer dims. On mismatch SKIP only this layer's swap
        // (warn + continue) - the restored state stays intact and dims are
        // never written. Dims absent (older meta) -> no gate, parity kept.
        const metaW = meta?.width;
        const metaH = meta?.height;
        if (
          typeof metaW === "number" &&
          typeof metaH === "number" &&
          (bitmap.width !== metaW || bitmap.height !== metaH)
        ) {
          console.warn("[history] snapshot bitmap size mismatches restored model dims; skipping layer re-attach", {
            docId,
            layerId: layer.layerId,
            payload: { width: bitmap.width, height: bitmap.height },
            model: { width: metaW, height: metaH },
          });
          continue;
        }
      }
      applied = field ? set(layer.layerId, bitmap, field) || applied : set(layer.layerId, bitmap) || applied;
    } else {
      // Token present but unresolvable: never silently detach the existing one.
      console.warn(
        "[history] snapshot bitmap token unresolved; keeping existing bitmap",
        { docId, layerId: layer.layerId, token: layer.bitmapToken },
      );
    }
  }
  return applied;
}