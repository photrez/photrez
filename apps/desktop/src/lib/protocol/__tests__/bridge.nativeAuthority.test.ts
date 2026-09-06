// Native-authority predicate test. `isNativeAuthority()` maps the
// `photrez.facadeAuthority` flag: absent/unknown/throw -> default wasm (false);
// true ONLY when the flag is explicitly "native". The predicate is read by the
// native-authority dispatch branches (applyCommand, getSnapshot, getHistoryQuery,
// historyCursorCommit).

import { describe, it, expect, beforeEach, afterEach } from "vitest";
import { isNativeAuthority } from "../bridge";

beforeEach(() => {
  localStorage.clear();
});
afterEach(() => {
  localStorage.clear();
});

describe("isNativeAuthority", () => {
  it("returns false when the flag is absent (default wasm)", () => {
    expect(isNativeAuthority()).toBe(false);
  });

  it("returns false when the flag is 'wasm'", () => {
    localStorage.setItem("photrez.facadeAuthority", "wasm");
    expect(isNativeAuthority()).toBe(false);
  });

  it("returns true only when the flag is exactly 'native'", () => {
    localStorage.setItem("photrez.facadeAuthority", "native");
    expect(isNativeAuthority()).toBe(true);
  });

  it("returns false for an unknown value", () => {
    localStorage.setItem("photrez.facadeAuthority", "bogus");
    expect(isNativeAuthority()).toBe(false);
  });

  it("returns false when localStorage throws", () => {
    const realGet = localStorage.getItem.bind(localStorage);
    localStorage.getItem = () => {
      throw new Error("blocked");
    };
    expect(isNativeAuthority()).toBe(false);
    localStorage.getItem = realGet;
  });
});
