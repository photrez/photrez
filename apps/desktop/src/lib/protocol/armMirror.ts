// SPDX-License-Identifier: AGPL-3.0-or-later
// Wire names (serde tag = "type" + camelCase rename) of every variant of the
// Rust `Command` enum in crates/core/src/command.rs. Routing tables and other
// command-side consumers read this list instead of re-spelling the strings.
// armMirror.test.ts parses the Rust source and the TS `Command` union and fails
// when either drifts from this list - keep all three in sync.
export const ARM_MIRROR = [
  "noop",
  "ping",
  "addLayer",
  "deleteLayer",
  "transformLayer",
  "setOpacity",
  "setVisible",
  "setLocked",
  "rename",
  "reorder",
  "setBackgroundFlag",
  "setBlendMode",
  "setLayerParams",
  "setAdjustment",
  "duplicateLayer",
  "mergeDown",
  "mergeSelected",
  "flatten",
  "rasterizeLayer",
  "cropCanvas",
  "applyCrop",
  "resizeCanvas",
  "brushStroke",
  "undo",
  "redo",
  "setSelection",
  "clearSelection",
  "selectAll",
  "invertSelection",
  "recordExternalTransition",
] as const;
