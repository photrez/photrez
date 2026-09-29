// The validation smokes in tests/validation set localStorage["photrez.rustPixels"]
// ="1" in-page because every assertion in them is about what the rust pixel path
// does, and they take it back off in finish() on every exit path. The three live
// runners (scripts/live-verify.mjs, perf-audit-live.mjs, ipc-frequency-live.mjs)
// must not be able to arm it at all: they map PHOTREZ_FLAGS to localStorage
// entries at start-up, and two of the three never cleared that key, so one
// leaked env var armed every later run. They share one parser that refuses the
// key, and these tests pin the refusal by resolving that parser rather than by
// grepping the runners for a literal write - a runner builds the key through a
// template, so a literal grep cannot see the one path that matters.
//
// This file pins three halves: the helper must actually remove the key without
// ever throwing over the run result; every harness that sets the key must call
// it before the page goes away (the removal cannot happen at all once the page
// is closed); and no live runner can produce the key from an env channel.

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
  parseHarnessFlags: (raw: string) => Array<{ key: string; value: string }>;
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

// The three live runners seed localStorage from PHOTREZ_FLAGS before the app
// boots. They share one parser, so the reserved key is refused once.
const FLAG_PARSER_SCRIPTS = [
  "scripts/live-verify.mjs",
  "scripts/perf-audit-live.mjs",
  "scripts/ipc-frequency-live.mjs",
];

// The seed expression every runner evaluates over CDP, mirrored so the
// assertion is about the key set that reaches localStorage rather than about
// the shape of the source text.
function seedExpression(pairs: Array<{ key: string; value: string }>): string {
  return pairs
    .map((f) => `localStorage.setItem(${JSON.stringify(f.key)}, ${JSON.stringify(f.value)});`)
    .join("");
}

describe("PHOTREZ_FLAGS cannot arm the reserved pixel enablement", () => {
  it("drops the reserved key, keeps every other flag, and reports the refusal", async () => {
    const { parseHarnessFlags } = await loadHelper();
    const warn = vi.spyOn(console, "error").mockImplementation(() => {});

    const pairs = parseHarnessFlags("facade=1 rustPixels=1 facadeAuthority=native");

    expect(pairs).toEqual([
      { key: "photrez.facade", value: "1" },
      { key: "photrez.facadeAuthority", value: "native" },
    ]);
    expect(seedExpression(pairs)).not.toContain(RUST_PIXELS_FLAG);
    // A silent refusal is the failure this pins: the run would measure the
    // non-Rust pixel path and the numbers would still look plausible.
    expect(warn).toHaveBeenCalledTimes(1);
    expect(String(warn.mock.calls[0][0])).toContain(RUST_PIXELS_FLAG);
    warn.mockRestore();
  });

  it.each(FLAG_PARSER_SCRIPTS)("%s reaches the key only through the refusing parser", async (rel) => {
    const { parseHarnessFlags } = await loadHelper();
    const warn = vi.spyOn(console, "error").mockImplementation(() => {});
    const src = readFileSync(repoFile(rel), "utf8");

    // Shape: the runner imports the shared parser and builds no "photrez.<key>"
    // pair itself, so that parser is its only route to a key. Either half
    // going away re-opens the leak, so both are asserted.
    expect(src).toMatch(
      /import \{[^}]*\bparseHarnessFlags\b[^}]*\} from "\.\/harness-flag-cleanup\.mjs"/,
    );
    expect(src).not.toContain("`photrez.${");
    // Behavior, for the one input this runner has: a hostile env var.
    expect(seedExpression(parseHarnessFlags("rustPixels=1,facade=1"))).not.toContain(RUST_PIXELS_FLAG);
    warn.mockRestore();
  });
});

describe("validation smoke flag cleanup", () => {
  const SMOKES = ["tests/validation/bitmap-sync.mjs", "tests/validation/restore-sync.mjs"];

  it.each(SMOKES)("%s sets the flag and takes it back off on every exit path", (rel) => {
    const src = readFileSync(repoFile(rel), "utf8");
    // The smoke has to turn the path on, otherwise its assertions are vacuous.
    expect(src).toContain(`localStorage.setItem("${RUST_PIXELS_FLAG}", "1")`);

    // A single finish() owns the removal, so count call sites rather than
    // trusting one: the happy exit, the early bail, and the top-level catch all
    // have to route through it.
    const finishDefs = src.match(/^async function finish\(/gm) ?? [];
    expect(finishDefs.length).toBe(1);
    // finish() must clear the flag BEFORE closing the page: after browser.close()
    // the evaluate in the helper has nothing to run against.
    const body = src.slice(src.indexOf("async function finish("));
    const bodyText = body.slice(0, body.indexOf("\n}"));
    expect(bodyText.indexOf("clearHarnessFlag(")).toBeGreaterThan(-1);
    expect(bodyText.indexOf("clearHarnessFlag(")).toBeLessThan(bodyText.indexOf("browser.close()"));

    // A throw after the flag is set must also clean up, so the top-level
    // rejection handler has to route through finish() rather than exiting.
    const catchBlock = src.slice(src.lastIndexOf("})().catch("));
    expect(catchBlock).toContain("await finish(");
    expect(catchBlock).not.toContain("process.exit(");

    // No raw exit may bypass finish(): a process.exit outside it would leave the
    // key set on the dev origin, which is the leak this guards.
    const exits = src.match(/process\.exit\(/g) ?? [];
    const exitsInFinish = bodyText.match(/process\.exit\(/g) ?? [];
    expect(exits.length).toBe(exitsInFinish.length);
  });
});
