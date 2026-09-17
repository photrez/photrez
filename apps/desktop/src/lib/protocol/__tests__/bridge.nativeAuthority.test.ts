// Native-authority predicate test. `isNativeAuthority()` maps the
// `photrez.facadeAuthority` flag: absent/unknown -> default native (true);
// false ONLY when the flag is explicitly "wasm" (the opt-out) or when the
// store itself is unreadable (no-storage keeps the legacy path). The predicate
// is read by the native-authority dispatch branches (applyCommand, getSnapshot,
// getHistoryQuery, historyCursorCommit).
//
// TRANSITIONAL (photrez.facadeAuthority default): delete this file's
// default-ON pins when the opt-out flag is retired.

import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { isFacadeEnabled, isNativeAuthority } from "../bridge";

beforeEach(() => {
  localStorage.clear();
});
afterEach(() => {
  vi.unstubAllGlobals();
  localStorage.clear();
});

describe("isNativeAuthority under the photrez.facadeAuthority default-ON flip", () => {
  it("returns true when the flag is absent (default native)", () => {
    expect(isNativeAuthority()).toBe(true);
  });

  it("returns false when the flag is 'wasm' (the opt-out)", () => {
    localStorage.setItem("photrez.facadeAuthority", "wasm");
    expect(isNativeAuthority()).toBe(false);
  });

  it("returns true when the flag is exactly 'native'", () => {
    localStorage.setItem("photrez.facadeAuthority", "native");
    expect(isNativeAuthority()).toBe(true);
  });

  it("returns true for an unknown value (only 'wasm' opts out)", () => {
    localStorage.setItem("photrez.facadeAuthority", "bogus");
    expect(isNativeAuthority()).toBe(true);
  });

  // No-storage safety net: unset flag means ON, but an absent or unreadable
  // store means OFF (SSR / privacy-mode guard). stubGlobal replaces the whole
  // global, so unlike spying getItem it reaches the guarded branches.
  it("returns false for both predicates when storage is absent", () => {
    vi.stubGlobal("localStorage", undefined);
    try {
      expect(isFacadeEnabled()).toBe(false);
      expect(isNativeAuthority()).toBe(false);
    } finally {
      vi.unstubAllGlobals();
    }
  });

  it("returns false for both predicates when storage throws", () => {
    vi.stubGlobal("localStorage", {
      getItem: () => {
        throw new Error("unreadable store");
      },
    });
    try {
      expect(isFacadeEnabled()).toBe(false);
      expect(isNativeAuthority()).toBe(false);
    } finally {
      vi.unstubAllGlobals();
    }
  });
});
