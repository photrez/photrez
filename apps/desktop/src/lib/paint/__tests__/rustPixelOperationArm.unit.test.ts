// SPDX-License-Identifier: AGPL-3.0-or-later
import { describe, it, expect } from "vitest";
import {
  resolveRustPixelOperationArm,
  type RustPixelOperation,
} from "../rustPixelOperationArm";

const OPERATIONS: RustPixelOperation[] = ["bucket", "fill", "bake"];

describe("resolveRustPixelOperationArm truth table", () => {
  // enabled=false must stay on the legacy arm for every operation and every
  // surface state: a missing surface must never turn a flag-off call into a
  // blocked one (that would surface an error on layers that never route).
  for (const operation of OPERATIONS) {
    it(`${operation}: disabled -> legacy regardless of surface readiness`, () => {
      expect(resolveRustPixelOperationArm(operation, false, false)).toBe("legacy");
      expect(resolveRustPixelOperationArm(operation, false, true)).toBe("legacy");
    });
  }

  for (const operation of OPERATIONS) {
    it(`${operation}: enabled + ready surface -> rust`, () => {
      expect(resolveRustPixelOperationArm(operation, true, true)).toBe("rust");
    });

    it(`${operation}: enabled + missing surface -> blocked`, () => {
      expect(resolveRustPixelOperationArm(operation, true, false)).toBe("blocked");
    });
  }
});
