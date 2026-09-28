// The live verification harness flips localStorage["photrez.rustPixels"]="1"
// in-page (scripts/live-verify.mjs, pixelCommit step) so its fill step routes
// through the rust pixel path. WebView2 persists that origin's localStorage on
// disk, so a run that never removes the key leaves it set for the next run and
// for any later read of the flag.
//
// These tests pin both halves of the cleanup: the helper must actually remove
// that one key without ever throwing over the run result, and the harness must
// call it from its finally block BEFORE the app process is killed (the page has
// to still be alive for the removal to happen at all).

import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it, vi } from "vitest";

const RUST_PIXELS_FLAG = "photrez.rustPixels";

function repoFile(rel: string): string {
  // vitest runs with cwd=apps/desktop; also accept a repo-root cwd.
  const candidates = [
    resolve(process.cwd(), "../..", rel),
    resolve(process.cwd(), rel),
    resolve(process.cwd(), "..", rel),
  ];
  const found = candidates.find((p) => existsSync(p));
  if (!found) throw new Error(`cannot locate ${rel}; tried:\n  ${candidates.join("\n  ")}`);
  return found;
}

type Helper = {
  clearHarnessFlag: (cdp: unknown) => Promise<boolean>;
  RUST_PIXELS_FLAG: string;
};

async function loadHelper(): Promise<Helper> {
  return (await import(repoFile("scripts/harness-flag-cleanup.mjs"))) as Helper;
}

// Executes the expression the helper sends over CDP against a stub page
// localStorage, mirroring Runtime.evaluate on the real dev origin.
function fakeCdp(store: Map<string, string>, opts?: { breakRemove?: boolean }) {
  const localStorage = {
    removeItem: (k: string) => {
      if (opts?.breakRemove) return; // simulate a page that refuses the removal
      store.delete(k);
    },
    getItem: (k: string) => (store.has(k) ? (store.get(k) as string) : null),
  };
  const evaluated: string[] = [];
  return {
    evaluated,
    async evaluate(expression: string) {
      evaluated.push(expression);
      return new Function("localStorage", `return (${expression});`)(localStorage);
    },
  };
}

describe("clearHarnessFlag (dev-origin pixel flag)", () => {
  it("removes photrez.rustPixels and leaves every other key alone", async () => {
    const { clearHarnessFlag } = await loadHelper();
    const store = new Map([
      [RUST_PIXELS_FLAG, "1"],
      ["photrez.facade", "1"],
    ]);
    const cdp = fakeCdp(store);
    const warn = vi.spyOn(console, "error").mockImplementation(() => {});

    const ok = await clearHarnessFlag(cdp);

    expect(ok).toBe(true);
    expect(store.has(RUST_PIXELS_FLAG)).toBe(false);
    expect(store.get("photrez.facade")).toBe("1");
    expect(cdp.evaluated.join("\n")).toContain(RUST_PIXELS_FLAG);
    expect(warn).not.toHaveBeenCalled();
    warn.mockRestore();
  });

  it("reports and never throws when the page is unreachable", async () => {
    const { clearHarnessFlag } = await loadHelper();
    const warn = vi.spyOn(console, "error").mockImplementation(() => {});
    const cdp = {
      async evaluate(): Promise<string> {
        throw new Error("ws closed");
      },
    };

    await expect(clearHarnessFlag(cdp)).resolves.toBe(false);
    expect(warn).toHaveBeenCalled();
    warn.mockRestore();
  });

  it("reports when there is no CDP session at all", async () => {
    const { clearHarnessFlag } = await loadHelper();
    const warn = vi.spyOn(console, "error").mockImplementation(() => {});

    await expect(clearHarnessFlag(null)).resolves.toBe(false);
    expect(warn).toHaveBeenCalled();
    warn.mockRestore();
  });

  it("reports instead of claiming success when the key survives removal", async () => {
    const { clearHarnessFlag } = await loadHelper();
    const store = new Map([[RUST_PIXELS_FLAG, "1"]]);
    const cdp = fakeCdp(store, { breakRemove: true });
    const warn = vi.spyOn(console, "error").mockImplementation(() => {});

    await expect(clearHarnessFlag(cdp)).resolves.toBe(false);
    expect(store.get(RUST_PIXELS_FLAG)).toBe("1");
    expect(warn).toHaveBeenCalled();
    warn.mockRestore();
  });
});

describe("live-verify harness wiring", () => {
  it("clears the flag from the finally block before the app is killed", () => {
    const src = readFileSync(repoFile("scripts/live-verify.mjs"), "utf8");
    const finallyAt = src.indexOf("} finally {");
    expect(finallyAt).toBeGreaterThan(-1);
    const finallyBlock = src.slice(finallyAt, finallyAt + 700);
    const clearAt = finallyBlock.indexOf("clearHarnessFlag(cdp)");
    const killAt = finallyBlock.indexOf("await cleanup()");
    expect(clearAt).toBeGreaterThan(-1);
    expect(killAt).toBeGreaterThan(clearAt);
  });
});
