# Architecture Checkpoint — 2026-08-24
**Purpose:** Freeze locked decisions + migration state before Brush / Pixel-Data design review. Protects against model/session drift. Design-only checkpoint — no production code change.

## LOCKED ADRs

**ADR 0007 — Hybrid Command-Snapshot (Accepted 2026-08-23, amended)**
- Pattern C: `SolidJS owns UI/interaction/viewport` + `Rust photrez-core owns persistent editor truth (Document, Layers, Selection, Transform, History, Image data, Import/Export, Serialization)` via typed `CommandEnvelope {contractVersion, expectedVersion?, command}` → `CommandResult {documentVersion, delta}` → versioned `RenderSnapshot`/`RenderDelta` projection.
- Invariant: `TS may hold transient interaction state; only Rust may hold persistent editor state.` No TS Document mirror.
- Guardrails: `photrez-core` is Tauri-independent (adapter pattern); `RenderSnapshot` is stale-able cache never second truth; `resourceId`/`dirtyRect` describe resources, renderer owns GPU handles; brush sends stroke semantics on `pointerup`, not bitmap.
- Files: `docs/decisions/adr/0007-hybrid-command-snapshot.md`, `crates/core/src/protocol.rs`, `apps/desktop/src/lib/protocol/{types.ts,bridge.ts,editorFacade.ts}`

**ADR 0008 — History Stream / Versioning / Payload Adapters (Accepted 2026-08-24)**
- C1 Three independent domains: `DocumentVersion (u64 monotonic transition stamp, ONLY currency for expectedVersion/delta.baseVersion/renderedVersion)` ≠ `HistorySeq (immutable entry identity, dense append-only)` ≠ `HistoryCursor (position [-1..lastSeq])`. No bijection. Entry `versionBefore/After` are diagnostic stamps; walker applies stored inverses.
- C2 Model B: `RecordExternalTransition {label, affectedLayerIds, adapterId, token, memoryCostBytes}` is a REAL Rust transition (validates `E_UNKNOWN_ADAPTER`, truncates redo, appends `External`, `DV+=1`). Two-step `TS mutate → RecordExternalTransition` is labeled **transitional crash-consistency limitation** with failure state `UNRECORDED_EXTERNAL_TRANSITION` + `CURSOR_COMMIT_FAILED`, barrier `E_EXTERNAL_PENDING`, `E_CURSOR_MISMATCH`.
- C3 Transitional ownership: `Rust owns canonical history metadata/index/cursor authority; external payloads temporarily TS-side behind PayloadAdapter`. Permanent model `HistoryEntry {seq, groupId, origin: Native|External(adapterId), payloadRef}` — no permanent `kind=mirror`. `PayloadAdapter` registration; `Native{before,after}` is transitional metadata-only, must not become pixel history; conservative `affectedLayerIds` superset is H0-only.
- Files: `docs/decisions/adr/0008-history-stream-versioning-adapters.md`, `crates/core/src/protocol.rs` (entries/cursor/next_seq/adapters/pending_external + begin_forward/finish_forward + walker), `apps/desktop/src/lib/protocol/{bridge.ts,facadeRegistry.ts}` (emu parity, `tsPayloadStore`, `pendingRecord/pendingConfirm`, `historyDegraded`, `getHistoryProjection`, `syncAuthoritativeVersion`)

**ADR 0009 — Mixed Selection Policy (Accepted 2026-08-24, transitional)**
- Classification per batch op: `[]→{mode:"empty"}` silent no-op (never facade; deduped) | all NOT owned→`{mode:"legacy"}` | all owned→`{mode:"facade", ownedIds: string[]}` (unique, first-occurrence order) | mixed→`{mode:"mixed-rejected"}` atomic rejection: zero mutation/history/protocol + one shared `MIXED_OWNERSHIP_MESSAGE` (`errors.mixedOwnershipSelection`).
- Single-target ops cannot be mixed by construction; read-only ops exempt; no exception for non-destructive mutating ops (consistency over convenience). Retires at full cutover (mixed branch becomes dead code).
- Shared pure helper `resolveSelectionRoute(ids: string[]): SelectionRoute` in `facadeRegistry.ts` is single source of truth; `isFacadeEnabled()` flag-aware; dedup via `Set`.
- Files: `docs/decisions/adr/0009-mixed-selection-policy.md`, `apps/desktop/src/lib/protocol/facadeRegistry.ts`, `apps/desktop/src/lib/protocol/__tests__/resolveSelectionRoute.test.ts` (7 tests)

## LOCKED INVARIANTS

- **Rust = persistent editor SSOT** — every accepted command mutates Rust, pushes `HistoryEntry`, bumps `DocumentVersion`, returns `RenderDelta`. TS never holds persistent truth. [ADR 0007 + ADR 0008]
- **TS = UI + transient interaction + renderer orchestration** — `transformPreview` / `opacityPreview` Solid signals merged by `EditorShell.buildState() → applyFacadePreviews(RenderState)` into outgoing `RenderState` (renderer file untouched). `pointermove` never enters Rust/IPC path (transient only, spy-proven). [VERIFIED: `facadeRegistry.ts:transformPreview/opacityPreview`, `EditorShell.tsx:buildState`, `benchOpacity.ts`/`benchTransform.ts`, `facadeOpacity.test.ts` 0-IPC ticks]
- **GPU resources = renderer-owned derived state** — `WebGL2Backend` owns `Map<string, TextureRef>` keyed by `layerId`; `RenderSnapshot`/`RenderDelta` carry `resourceId` + `dirtyRect` (logical), never GPU handles. Empty layer (`imageBitmap==null`) creates no texture, skipped via `visibleLayers` filter. [VERIFIED: `apps/desktop/src/renderer/webgl2.ts:100,73,523`]
- **DocumentVersion ≠ HistorySeq ≠ HistoryCursor** — three domains, no coupling invariant. [ADR 0008 C1]
- **expectedVersion uses DocumentVersion only** — sourced from engine-authoritative `DocumentVersion` via `facade.renderedVersion` refresh after every result; mismatch → `E_VERSION_MISMATCH`. Never derived from `HistorySeq`/cursor. [VERIFIED: `editorFacade.ts`, `protocol.rs:CommandEnvelope`, `facadeOpacity.test.ts` stale test]
- **pointermove must not enter Rust command/IPC path** — transient preview only; commit on semantic boundary (`pointerup` / `finishOpacityEdit`). [VERIFIED: drag + slider tests]
- **snapshots/deltas are projections, not second SSOT** — `RenderSnapshot {version, layers: RenderLayer[]}` and `RenderDelta {baseVersion, version, changes: RenderLayerChange[]}`; renderer applies delta only if `delta.baseVersion === renderedVersion`, else requests full snapshot. [ADR 0007 + `protocol.rs`, `bridge.ts`]
- **No pixel transport design is finalized yet** — brush pixels still owned by TS `ImageBitmap` per layer; Rust pixel ownership / `ResourceRegistry` / `TileStore` / zero-copy / WebGPU renderer are NOT implemented and have no finalized data-path. [VERIFIED: absence in `protocol.rs` RenderSnapshot/Delta (metadata-only, no pixel bytes), `document.ts:ImageBitmap`]

## COMPLETED MIGRATIONS (behind `photrez.facade=1`, legacy path byte-identical when OFF)

- **AddLayer** — `handleAddLayer` facade branch via `seedFacadeFromEngine` → `EditorFacade.seedSnapshot()` (bypasses monotonic guard), WASM lifetime consolidated to module-level `ENGINE`, Gate A guards. [VERIFIED: `useLayerActions.ts:handleAddLayer`, `facadeRegistry.ts:seedFacadeFromEngine`, `protocol.rs` module-level thread_local]
- **Transform** — `useSelectionTransformDrag.applyDragTransform` (facade transient `updateTransform` + preview signal, 0 IPC per move) + numeric `facadeCommitNumericTransform` via shared funnel; `EditorShell` scheduler merges preview; `useEditorCommands` undo/redo `hasFacadeOwnedLayers()` → `facade.undo/redo()` with `lastHistoryDeltaWasEmpty` fallback. [VERIFIED: `facadeTransformWiring.test.ts` 6/6, `facadeNumericTransform.test.ts` 4/4, `mixedHistory.test.ts` 2/2]
- **DeleteLayer** — `Remove {id, resourceId}` (resourceId captured pre-removal), `facadeProjectedIds` reconciliation in `applyFacadeSnapshot` (unmarks disappeared ids), `handleDeleteActiveLayer` single+multi via `resolveSelectionRoute` (mixed → atomic rejection with shared message). [VERIFIED: `facadeDelete.test.ts` 3/3, `deleteMixedSelection.test.ts` 3/3, `protocol.rs:Remove`]
- **Opacity** — `opacityPreview` signal + `applyFacadePreviews` pure merge + `commitFacadeOpacity` funnel; `PropertiesPanel.handleOpacityChange` (transient) / `finishOpacityEdit` (ONE `SetOpacity` per owned id with `expectedVersion` + projection, no-op guard). [VERIFIED: `facadeOpacity.test.ts` 8/8, `PropertiesPanel.tsx`, `benchOpacity.ts:__benchO0`]
- **Shared `resolveSelectionRoute()`** — pure helper in `facadeRegistry.ts` (dedup, empty→silent, flag-aware). [VERIFIED: `resolveSelectionRoute.test.ts` 7/7]
- **H0 history infrastructure** — `entries/cursor/next_seq/adapters/pending_external`, walker (native apply / external handoff `status:"external"`), `RecordExternalTransition`, `register/query/cursor_commit` exports, `tsPayloadStore`, `pendingMarkers`, `historyDegraded(UNRECORDED_EXTERNAL_TRANSITION/CURSOR_COMMIT_FAILED)`, `getHistoryProjection`, `syncAuthoritativeVersion`, `installFacadeCommitShim` (zero cost OFF). [VERIFIED: `cargo test -p photrez-core` 82 passed incl. 7 H0 barrier tests, `h0Stream.test.ts` 12/12]

## CURRENT STATE

- Rust pixel ownership: **NOT implemented** — pixels remain `ImageBitmap` per layer in `DocumentEngine` (`apps/desktop/src/engine/document.ts`), rendered by `WebGL2Backend` (`apps/desktop/src/renderer/webgl2.ts`). `photrez-core` RenderSnapshot/Delta are metadata-only today.
- Brush migration: **NOT implemented** — brush rasterization stays in `apps/desktop/src/components/editor/useBrushOverlay.ts` (Canvas2D GPU scratch + `PaintTileSurface` `PaintTileSurface.ts` tile patches for undo), not in Rust. Flag `photrez.tileCommit` / `photrez.rustDabs` are prior experiments, not the Hybrid migration.
- ResourceRegistry: **NOT implemented** — `resourceId` exists as stable logical id in `RenderLayer`/`RenderDelta` but no registry lifecycle (create/update/destroy mapping) is implemented.
- TileStore: **NOT implemented** — prior `PaintTileSurface` (256×256) is a TS tile optimization for brush undo, not the Rust TileStore; Rust tile storage does not exist.
- zero-copy / shared-memory: **NOT implemented** — no `SharedArrayBuffer` transport, no `rgba_buffer_view` pixel path for brush; `bench-rust-vs-ts` shows naive per-pixel WASM slower (0.69×), GPU WGSL wins for invert/adjust only.
- WebGPU renderer: **NOT implemented** — renderer remains WebGL2; WebGPU compute (`gpuCompute.ts` WGSL invert/adjust) is an interactive compute layer for filters, not the renderer. No WebGPU canvas ownership.

## BENCHMARK INTERPRETATION (locked)

- **B0** (`docs/performance/baseline-ticket2.json`, commit `824584d`, seed 42, 100 iters, `performance.memory` unavailable → `process.memoryUsage` fallback) = **headless protocol baseline** for the facade protocol. Frozen, never compared to real runtime.
- **R0/R1/O1/D1/T1 etc.** (`__benchR0/__benchT0/__benchD0/__benchO0` via `agent-browser` + `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9223` on live Tauri/WebGL2) = **real runtime measurements** (same machine, same bytes, real `createImageBitmap`/WebGL2).
- **Do not claim "Rust is faster" from addLayer** — R0 already uses Rust per-doc `DocumentEngine` (`USE_RUST_SSOT=true`); `~8-10ms → ~0.1ms` reduction = removal of `syncLayersFromRust` + `pushModelToRust` double JSON roundtrips (documented in `AI_HISTORY [2026-08-23]` + `AI_CURRENT_TASK`).
- **commit latency ≠ firstVisible latency (both retained):**
  - `commit` = serialization→wasm→Rust guard/mutation/history/version bump→delta creation→parse→facade merge→projection. Excludes following `requestRender`/rAF. Measures correctness/ownership cost.
  - `firstVisible = commit → first painted frame` (includes `requestRender` + rAF + `WebGL2Backend.render()`). Measures user-perceived responsiveness.
  - Retained both because commit can regress within frame budget without visible jank, and firstVisible can stay fast while commit grows; `previewStep` uses same split (`TIME-TO-NEXT-VISIBLE-FRAME` ≈ rAF-bound ~16.7ms @60Hz, not compute latency). Documented in `benchTransform.ts` + `benchOpacity.ts` header/`metricSemantics`.
- **preview latency is `TIME-TO-NEXT-VISIBLE-FRAME`, not raw compute latency** — both legacy and facade previews are rAF-bound in measured fixtures (e.g., Opacity O1 16.5 vs O0 16.7).

## References

- `docs/decisions/adr/0007-hybrid-command-snapshot.md`
- `docs/decisions/adr/0008-history-stream-versioning-adapters.md`
- `docs/decisions/adr/0009-mixed-selection-policy.md`
- `docs/AI_CURRENT_TASK.md` + `docs/AI_HISTORY.md` (migration evidence + benchmark tables)
- `crates/core/src/protocol.rs`, `apps/desktop/src/lib/protocol/*`, `apps/desktop/src/renderer/webgl2.ts`, `apps/desktop/src/engine/document.ts`

---

## [2026-08-24] Production Brush Baseline — LOCKED (T-BRUSH-TILECOMMIT-GRAD APPROVED/CLOSED)

Production Brush contract is now the locked baseline: Brush semantics (DabProducer, per-stroke settings snapshot) · Batched working surface (GPU scratch, single crossing, merged readRect) · PaintTileSurface as persistent pixel owner · HistoryTilePatches (entry-owned per-tile mementos) · uploadSurfaceTiles + requestRender · metadata-only RenderDelta. Event contract: pointercancel/Escape(active)=discard, lostpointercapture=commit, pointerup=commit; pre-history failure restores all before-patches with no entry. photrez.tileCommit default-ON (rollback "0"). Full contract: docs/plans/2026-08-24-brush-ux-production-path-design.md §7. Future Rust pixel ownership work must benchmark against bench-final-matrix-v2 and preserve this UX contract unchanged.
