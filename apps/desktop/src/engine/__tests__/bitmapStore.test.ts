import { describe, it, expect, vi, beforeEach } from "vitest";
import {
  BitmapStore,
  bitmapStoreFor,
  releaseBitmapStore,
  tokenForBitmap,
  existingTokenForBitmap,
} from "../bitmapStore";

function fakeBitmap(): ImageBitmap {
  return { width: 4, height: 4, close: vi.fn() } as unknown as ImageBitmap;
}

describe("BitmapStore", () => {
  it("set/get/has resolve a token to the SAME bitmap", () => {
    const store = new BitmapStore();
    const b = fakeBitmap();
    store.set("t1", b);
    expect(store.has("t1")).toBe(true);
    expect(store.has("nope")).toBe(false);
    expect(store.get("t1")).toBe(b);
    expect(store.get("nope")).toBeNull();
  });

  it("release closes exactly once and forgets the mapping (idempotent)", () => {
    const store = new BitmapStore();
    const b = fakeBitmap();
    store.set("t1", b);
    store.release("t1");
    expect(b.close).toHaveBeenCalledTimes(1);
    expect(store.has("t1")).toBe(false);

    // Double-release must be a guarded no-op — no second close, no crash.
    store.release("t1");
    expect(b.close).toHaveBeenCalledTimes(1);
    // Releasing an unknown token is also a safe no-op.
    expect(() => store.release("missing")).not.toThrow();
  });

  it("release on an already-closed bitmap is swallowed (no double-close crash)", () => {
    const store = new BitmapStore();
    // A bitmap whose close() throws (already detached) must not crash release.
    const b = { width: 4, height: 4, close: vi.fn(() => { throw new Error("already detached"); }) } as unknown as ImageBitmap;
    store.set("t1", b);
    expect(() => store.release("t1")).not.toThrow();
    expect(store.has("t1")).toBe(false);
  });

  it("drop removes the mapping WITHOUT closing (close stays owned by disposeSnapshot)", () => {
    const store = new BitmapStore();
    const b = fakeBitmap();
    store.set("t1", b);
    store.drop("t1");
    expect(store.has("t1")).toBe(false);
    expect(b.close).not.toHaveBeenCalled();
  });

  it("bitmapStoreFor returns a per-doc instance and releaseBitmapStore drops it", () => {
    const a = bitmapStoreFor("docA");
    const b = bitmapStoreFor("docA");
    expect(a).toBe(b);
    releaseBitmapStore("docA");
  });
});

describe("bitmap token stability", () => {
  it("the same bitmap object always maps to the same stable token", () => {
    const b = fakeBitmap();
    const t1 = tokenForBitmap(b);
    const t2 = tokenForBitmap(b);
    expect(t1).toBe(t2);
    expect(existingTokenForBitmap(b)).toBe(t1);
  });

  it("different bitmap objects map to different tokens", () => {
    const a = fakeBitmap();
    const c = fakeBitmap();
    expect(tokenForBitmap(a)).not.toBe(tokenForBitmap(c));
  });

  it("an unregistered bitmap has no existing token", () => {
    expect(existingTokenForBitmap(fakeBitmap())).toBeNull();
  });
});
