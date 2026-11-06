// SPDX-License-Identifier: AGPL-3.0-or-later
// apply() command-arm dispatch for DocumentCore. Kept separate from the engine
// so both stay under the size cap; this is a sibling impl DocumentCore block
// (method visibility is per-impl, not per module).
use super::*;
use crate::document::content::SelectionState;

mod create;
mod metadata;
mod selection;

impl DocumentCore {
    // Selection is engine-local UI state, not an undoable transition. Set the
    // engine field AND mirror it onto the canonical shadow (when seeded) so the
    // protocol_canonical_native read-back stays truthful. No history entry, empty
    // delta — the caller's apply() tail bumps the version and reconciles.
    // Selection is engine-local UI state, not an undoable transition. Set the
    // engine field AND mirror it onto the canonical shadow (when seeded) so the
    // protocol_canonical_native read-back stays truthful. No history entry, empty
    // delta — the caller's apply() tail bumps the version and reconciles.
    pub(crate) fn set_engine_selection(&mut self, sel: Option<SelectionState>) {
        self.selection = sel.clone();
        if let Some(shadow) = &mut self.replica {
            shadow.doc.selection = sel;
        }
    }

    pub fn apply(&mut self, envelope: CommandEnvelope) -> Result<CommandResult, ProtocolError> {
        if envelope.contract_version != CONTRACT_VERSION {
            return Err(ProtocolError {
                code: "E_CONTRACT_VERSION".to_string(),
                message: format!(
                    "expected contractVersion {} got {}",
                    CONTRACT_VERSION, envelope.contract_version
                ),
            });
        }
        if let Some(expected) = envelope.expected_version {
            if expected != self.version {
                return Err(ProtocolError {
                    code: "E_VERSION_MISMATCH".to_string(),
                    message: format!("expected version {} got {}", expected, self.version),
                });
            }
        }
        // External-pending barrier (history-stream invariant): a host handoff is
        // outstanding — no new command may enter until the cursor commit lands.
        if let Some((seq, dir)) = &self.pending_external {
            return Err(ProtocolError {
                code: "E_EXTERNAL_PENDING".to_string(),
                message: format!(
                    "external history transition pending: commit seq {} direction {} first",
                    seq, dir
                ),
            });
        }
        let base = self.version;
        // History stream: external record is its own transition event.
        if let Command::RecordExternalTransition {
            label,
            affected_layer_ids,
            adapter_id,
            token,
            memory_cost_bytes,
            doc_size_before,
            doc_size_after,
            minted_layer_ids,
        } = envelope.command
        {
            self.record_external(
                &label,
                &affected_layer_ids,
                &adapter_id,
                &token,
                memory_cost_bytes,
                doc_size_before,
                doc_size_after,
                &minted_layer_ids,
            )?;
            // Note: record_external already bumps version; do not bump again.
            // Layers unchanged here - reconcile intentionally skipped (host executes
            // external mutation out-of-band).
            return Ok(CommandResult {
                document_version: self.version,
                delta: RenderDelta {
                    base_version: base,
                    version: self.version,
                    changes: Vec::new(),
                    width: None,
                    height: None,
                },
                status: Some("external-recorded".to_string()),
                external_seq: None,
                pixel_patches: None,
                external_token: None,
            });
        }
        // Walker handoff marker: set when an undo/redo step lands on an
        // EXTERNAL entry — host executes via its adapter then confirms with
        // protocol_history_cursor_commit (which performs the DV bump).
        let mut external_handoff: Option<u64> = None;
        let mut handoff_dir = "";
        // The host's token for the External entry this step landed on, handed
        // back so the host can find the pre-op rasters it parked under that
        // token at record time. Set by the undo and redo External arms; None for
        // every other step.
        let mut external_token: Option<String> = None;
        // Additive document-size signal for this delta: set by the canvas-size arms
        // and by the undo/redo walker when a canvas entry's dimensions are restored.
        // None for every other command, so the delta stays byte-identical.
        let mut delta_dims: Option<(f64, f64)> = None;
        let changes = match envelope.command {
            Command::Noop => Vec::new(),
            cmd @ (Command::Ping { .. }
            | Command::AddLayer { .. }
            | Command::AddLayerWithState { .. }
            | Command::DeleteLayer { .. }) => self.apply_create_or_delete_arm(cmd)?,
            cmd @ (Command::SetVisible { .. }
            | Command::SetLocked { .. }
            | Command::Rename { .. }
            | Command::Reorder { .. }
            | Command::SetBackgroundFlag { .. }
            | Command::SetBlendMode { .. }
            | Command::SetLayerParams { .. }
            | Command::SetAdjustment { .. }) => self.apply_metadata_arm(cmd)?,
            cmd @ (Command::SetSelection { .. }
            | Command::ClearSelection
            | Command::SelectAll
            | Command::InvertSelection
            | Command::TransformLayer { .. }
            | Command::SetOpacity { .. }
            | Command::BrushStroke { .. }) => self.apply_selection_or_transform_arm(cmd)?,
            // -- Structural command arms (duplicate / merge-down / merge-selected /
            //    flatten / rasterize) --
            // Each arm delegates to a private helper in document/structural.rs
            // that performs the full begin_forward -> mutate -> finish_forward
            // sequence and returns the delta. The helpers reject E_INVALID before
            // mutation and treat unknown / not-mergeable ids as silent no-ops
            // (empty delta), the same contract as the 0.1 engine's apply ops.
            Command::DuplicateLayer { id, new_id } => self.apply_duplicate(&id, &new_id)?,
            Command::MergeDown { id, merged_id } => self.apply_merge_down(&id, &merged_id)?,
            Command::MergeSelected { ids, merged_id } => {
                self.apply_merge_selected(&ids, &merged_id)?
            }
            Command::Flatten { merged_id } => self.apply_flatten(&merged_id)?,
            Command::RasterizeLayer { id, pixels } => self.apply_rasterize(&id, pixels.as_ref())?,
            // -- Canvas-size command arms (Crop Canvas / Apply Crop / Resize Canvas) --
            // Each delegates to a private helper in document/canvas.rs that
            // performs begin_forward -> mutate doc_size + (optional) layers ->
            // finish_forward and returns the delta (ordered all-layer Upserts for
            // the crop arms; an empty delta for resize, whose only effect is the
            // document size). Helpers reject E_INVALID before mutation and treat
            // non-positive dims as a silent no-op (matching the 0.1 engine).
            Command::CropCanvas {
                x,
                y,
                width,
                height,
            } => {
                let entries_before = self.entries.len();
                let changes = self.apply_crop_canvas(x, y, width, height)?;
                // A silent no-op creates no entry; a real crop carries the new size.
                if self.entries.len() > entries_before {
                    delta_dims = self.doc_size;
                }
                changes
            }
            Command::ApplyCrop {
                x,
                y,
                width,
                height,
                rotation,
                target_width,
                target_height,
                delete_cropped_pixels,
                fill_background,
            } => {
                let entries_before = self.entries.len();
                let changes = self.apply_apply_crop(
                    x,
                    y,
                    width,
                    height,
                    rotation,
                    target_width,
                    target_height,
                    delete_cropped_pixels,
                    fill_background,
                )?;
                if self.entries.len() > entries_before {
                    delta_dims = self.doc_size;
                }
                changes
            }
            Command::ResizeCanvas { width, height } => {
                let entries_before = self.entries.len();
                let changes = self.apply_resize_canvas(width, height)?;
                // Resize emits an empty layer delta; these dims are its only signal.
                if self.entries.len() > entries_before {
                    delta_dims = self.doc_size;
                }
                changes
            }
            // Handled by the early-return above (kept for exhaustiveness).
            Command::RecordExternalTransition { .. } => Vec::new(),
            Command::Undo => {
                if self.cursor == 0 {
                    // No-op undo: DV still bumps (accepted transition event).
                    Vec::new()
                } else if matches!(
                    &self.entries[self.cursor - 1].payload,
                    EntryPayload::External { .. }
                ) {
                    // External (host-handoff) entry: restore the captured layer
                    // order natively and emit the ordered restatement, then route
                    // the host handoff so the external store steps its own cursor (the
                    // cursor moves on the cursor commit, not here).
                    let (seq, changes) = self.restore_external_layers(self.cursor - 1, "undo");
                    // Document size: both halves are HOST-supplied (the engine's
                    // own `doc_size` is baseline-only and goes stale after the
                    // first host crop, so it is never the source here). Surface a
                    // size delta only when the host supplied BOTH halves and they
                    // differ. BOTH is required: a half-supplied pair is an
                    // incomplete answer, and restoring whichever half happens to be
                    // present would resize the document to a size the host never
                    // said this transition was undoing. A metadata External
                    // (delete/move/reorder), an absent after half, or a size-neutral
                    // mutation with equal halves therefore emits no size at all,
                    // which keeps it an empty layer delta for the host and leaves
                    // its fall-through routing untouched.
                    let (before_size, after_size, token) =
                        match &self.entries[self.cursor - 1].payload {
                            EntryPayload::External {
                                doc_size_before,
                                doc_size_after,
                                token,
                                ..
                            } => (*doc_size_before, *doc_size_after, Some(token.clone())),
                            _ => (None, None, None),
                        };
                    external_token = token;
                    if before_size.is_some() && after_size.is_some() && before_size != after_size {
                        delta_dims = before_size;
                    }
                    external_handoff = Some(seq);
                    handoff_dir = "undo";
                    changes
                } else {
                    let e = &self.entries[self.cursor - 1];
                    match &e.payload {
                        EntryPayload::Native {
                            before,
                            after,
                            doc_size_before,
                            pixel_deltas,
                            ..
                        } => {
                            let new_layers =
                                Self::restore_with_foreign(&self.layers, before, after, true);
                            // `new_layers` is the merged end state (captured entry
                            // vector + foreign survivors). `diff_walker` emits Remove
                            // for ids the entry introduced (current \ merged) and an
                            // ordered Upsert of EVERY merged layer - so the delta is
                            // the authoritative final vector and a restored/merged
                            // layer lands at its snapshot position.
                            let changes = Self::diff_walker(&self.layers, &new_layers);
                            self.layers = new_layers;
                            // Restore the document size for canvas arms. This is
                            // always written (Some or None) so a None->Some crop
                            // undoes back to None; metadata arms leave the value
                            // unchanged, so restoring it is a no-op for them. Attach
                            // the restored size to the delta only when it actually
                            // changed; metadata entries (unchanged dims) stay None.
                            let pre_doc = self.doc_size;
                            self.doc_size = *doc_size_before;
                            if pre_doc != *doc_size_before {
                                delta_dims = *doc_size_before;
                            }
                            if let Some(d) = doc_size_before {
                                if let Some(sh) = &mut self.replica {
                                    sh.doc.width = d.0;
                                    sh.doc.height = d.1;
                                }
                            }
                            // Pixel half of a destructive arm (crop with baking):
                            // replay the entry's `before` states so one undo step
                            // restores metadata AND pixels.
                            let restore = restored_pixel_steps(pixel_deltas, false);
                            self.pending_pixels.extend(restore);
                            // doc_size single-owner invariant (enforced by construction,
                            // never by seed_ptz): doc_size has exactly three owners -
                            // (1) the baseline set in seed_ptz only when engine
                            // doc_size is None (document open), (2) the canvas arms
                            // (document/canvas.rs:90/214/253) which set doc_size AND
                            // capture its before/after pair via begin_forward, (3) this
                            // walker, which restores the captured pair. A canonical
                            // re-push refreshes ONLY the shadow (SelectAll/Invert read it)
                            // and can NEVER mutate doc_size or wipe history, so the
                            // captured pair below always restores a value consistent with
                            // the native dims timeline. Pinned by
                            // repush_never_mutates_native_doc_size and
                            // audit_sequence_repush_preserves_history_and_dims in
                            // document/tests/seed.rs.
                            self.cursor -= 1;
                            changes
                        }
                        EntryPayload::Pixel { .. } => {
                            // Pixel entries live in this same stream, so the
                            // walker is their executor: step the cursor here and
                            // hand the tiles to the enclosing DocumentPixelStore
                            // via `pending_pixel`. That store owns the pixel
                            // buffer, so it writes the tiles and returns them to
                            // the host on `CommandResult::pixel_patches`.
                            // A pixel step changes no layer metadata, so the
                            // RenderDelta stays empty - emptiness is no longer a
                            // signal that "Rust had nothing", which is why the
                            // host must read `pixel_patches`, not the delta.
                            if let Some((layer_id, tiles, node)) =
                                self.pixel_step_at(self.cursor - 1, false)
                            {
                                self.pending_pixels.push((layer_id, tiles, node));
                                self.cursor -= 1;
                            }
                            Vec::new()
                        }
                        EntryPayload::Snapshot { .. } => {
                            // Snapshot undo/redo is handled by the dedicated
                            // undo_snapshot/redo_snapshot path (it restores the
                            // metadata + bitmap-token reference); the metadata
                            // walker leaves Snapshot entries untouched.
                            Vec::new()
                        }
                        // External entries are handled above (captured native
                        // order restore + host handoff); unreachable here.
                        EntryPayload::External { .. } => Vec::new(),
                    }
                }
            }
            Command::Redo => {
                if self.cursor >= self.entries.len() {
                    Vec::new()
                } else if matches!(
                    &self.entries[self.cursor].payload,
                    EntryPayload::External { .. }
                ) {
                    // External (host-handoff) entry: see the undo arm. Redo
                    // restores the up-projected side captured at the sync.
                    let (seq, changes) = self.restore_external_layers(self.cursor, "redo");
                    // Document size, symmetric to the undo arm and driven by the
                    // same host-supplied pair: redo re-applies the AFTER half, and
                    // both halves must be present for the same reason they must on
                    // undo.
                    let (before_size, after_size, token) = match &self.entries[self.cursor].payload
                    {
                        EntryPayload::External {
                            doc_size_before,
                            doc_size_after,
                            token,
                            ..
                        } => (*doc_size_before, *doc_size_after, Some(token.clone())),
                        _ => (None, None, None),
                    };
                    external_token = token;
                    if before_size.is_some() && after_size.is_some() && before_size != after_size {
                        delta_dims = after_size;
                    }
                    external_handoff = Some(seq);
                    handoff_dir = "redo";
                    changes
                } else {
                    let e = &self.entries[self.cursor];
                    match &e.payload {
                        EntryPayload::Native {
                            after,
                            before,
                            doc_size_after,
                            pixel_deltas,
                            ..
                        } => {
                            let new_layers =
                                Self::restore_with_foreign(&self.layers, after, before, true);
                            // `new_layers` is the merged end state; `diff_walker`
                            // emits the authoritative full-vector delta (see undo arm).
                            let changes = Self::diff_walker(&self.layers, &new_layers);
                            self.layers = new_layers;
                            // Restore the document size for canvas arms (see undo
                            // branch: always written, Some or None). Attach the
                            // restored size to the delta only when it actually changed.
                            let pre_doc = self.doc_size;
                            self.doc_size = *doc_size_after;
                            if pre_doc != *doc_size_after {
                                delta_dims = *doc_size_after;
                            }
                            if let Some(d) = doc_size_after {
                                if let Some(sh) = &mut self.replica {
                                    sh.doc.width = d.0;
                                    sh.doc.height = d.1;
                                }
                            }
                            // See the undo arm: destructive arms carry their pixel
                            // half in the entry, so redo replays the `after` states.
                            let restore = restored_pixel_steps(pixel_deltas, true);
                            self.pending_pixels.extend(restore);
                            // doc_size single-owner invariant (enforced by construction,
                            // never by seed_ptz): doc_size has exactly three owners -
                            // (1) the baseline set in seed_ptz only when engine
                            // doc_size is None (document open), (2) the canvas arms
                            // (document/canvas.rs:90/214/253) which set doc_size AND
                            // capture its before/after pair via begin_forward, (3) this
                            // walker, which restores the captured pair. A canonical
                            // re-push refreshes ONLY the shadow (SelectAll/Invert read it)
                            // and can NEVER mutate doc_size or wipe history, so the
                            // captured pair below always restores a value consistent with
                            // the native dims timeline. Pinned by
                            // repush_never_mutates_native_doc_size and
                            // audit_sequence_repush_preserves_history_and_dims in
                            // document/tests/seed.rs.
                            self.cursor += 1;
                            changes
                        }
                        EntryPayload::Pixel { .. } => {
                            // See the undo branch: the walker executes the pixel
                            // step (cursor + tiles) and the enclosing
                            // DocumentPixelStore completes the buffer write.
                            if let Some((layer_id, tiles, node)) =
                                self.pixel_step_at(self.cursor, true)
                            {
                                self.pending_pixels.push((layer_id, tiles, node));
                                self.cursor += 1;
                            }
                            Vec::new()
                        }
                        EntryPayload::Snapshot { .. } => {
                            // See undo branch: snapshot entries are owned by
                            // undo_snapshot/redo_snapshot, not the walker.
                            Vec::new()
                        }
                        // External entries are handled above (captured native
                        // order restore + host handoff); unreachable here.
                        EntryPayload::External { .. } => Vec::new(),
                    }
                }
            }
        };
        if let Some(seq) = external_handoff {
            self.pending_external = Some((seq, handoff_dir.to_string()));
            // The layer vector was restored natively from the entry's captured
            // set, so this delta is the ordered restatement the host projection
            // reconstructs order from. The version still advances only on the
            // cursor commit; the cursor itself is moved there, not here.
            //
            // A host crop's document size rides the SAME early return: the arms
            // above set `delta_dims` only when the host supplied a differing pair,
            // so `None` here means "this transition changed no size" and the delta
            // stays a pure layer restatement the host reads exactly as before.
            let (width, height) = match delta_dims {
                Some((w, h)) => (Some(w), Some(h)),
                None => (None, None),
            };
            return Ok(CommandResult {
                document_version: self.version,
                delta: RenderDelta {
                    base_version: base,
                    version: self.version,
                    changes,
                    width,
                    height,
                },
                status: Some("external".to_string()),
                external_seq: Some(seq),
                pixel_patches: None,
                // The token the host minted for this entry, handed back so the
                // host can re-apply the pre-op rasters it parked under it.
                external_token,
            });
        }
        self.version += 1;
        self.reconcile_replica();
        let (width, height) = match delta_dims {
            Some((w, h)) => (Some(w), Some(h)),
            None => (None, None),
        };
        Ok(CommandResult {
            document_version: self.version,
            delta: RenderDelta {
                base_version: base,
                version: self.version,
                changes,
                width,
                height,
            },
            status: None,
            external_seq: None,
            pixel_patches: None,
            external_token: None,
        })
    }
}

/// Materialize the pixel steps a destructive metadata entry replays.
/// `redo = false` restores the entry's `before` states (undo), `true` the
/// `after` states (redo). Pure over the entry's deltas so both walker arms
/// share one definition.
fn restored_pixel_steps(
    deltas: &[PixelDelta],
    redo: bool,
) -> Vec<(String, Vec<crate::raster::tile::TilePatch>, Arc<StateNode>)> {
    deltas
        .iter()
        .map(|d| {
            let (node, other) = if redo {
                (&d.after, &d.before)
            } else {
                (&d.before, &d.after)
            };
            (
                d.layer_id.clone(),
                crate::wire::projection::state_node_touched_patches(node, other),
                node.clone(),
            )
        })
        .collect()
}
