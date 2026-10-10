import { describe, it, expect, vi } from "vitest";
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

// The store's write side (registering a token) went away with the
// snapshot-token payload builder, so an unregistered token is the only state a
// store can be in; disposeSnapshot then takes its direct-close path. The
// close-once / idempotency contracts of release() and drop() are exercised
// through disposeSnapshot in the history suites.
describe("BitmapStore (token registry, read side)", () => {
  it("get/has resolve an unregistered token to null / false", () => {
    const store = new BitmapStore();
    expect(store.get("nope")).toBeNull();
    expect(store.has("nope")).toBe(false);
  });

  it("release and drop on an unknown token are safe no-ops", () => {
    const store = new BitmapStore();
    expect(() => store.release("missing")).not.toThrow();
    expect(() => store.drop("missing")).not.toThrow();
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
