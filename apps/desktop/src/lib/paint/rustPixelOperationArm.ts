// SPDX-License-Identifier: AGPL-3.0-or-later

/** The three production operations that route through {@link resolveRustPixelOperationArm}. */
export type RustPixelOperation = "bucket" | "fill" | "bake";

/**
 * - `legacy`: run the existing TypeScript bitmap/history path unchanged.
 * - `rust`:   run the canonical Rust pixel write path.
 * - `blocked`: the flag is on but no paint surface exists; fail visibly instead of
 *   silently writing through the legacy arm while the flag says Rust is authoritative.
 */
export type RustPixelOperationArm = "legacy" | "rust" | "blocked";

/**
 * Single decision point for the three raster call sites (paint bucket, fill layer,
 * adjustment bake). `operation` is kept so the guard can later log or gate per
 * operation; routing itself depends only on the flag and surface readiness.
 */
export function resolveRustPixelOperationArm(
  operation: RustPixelOperation,
  enabled: boolean,
  surfaceReady: boolean,
): RustPixelOperationArm {
  if (!enabled) return "legacy";
  return surfaceReady ? "rust" : "blocked";
}
