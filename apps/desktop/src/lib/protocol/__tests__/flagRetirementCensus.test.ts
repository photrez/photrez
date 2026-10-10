// SPDX-License-Identifier: AGPL-3.0-or-later

/**
 * RETIRED-KEY CENSUS - the new-contract guard for the flag retirement slice.
 *
 * WHAT IT GUARDS. The endgame retires two runtime keys that have zero
 * production setters and only dead readers left: `photrez.historyBridge`
 * (the dev history-bridge gate) and `photrez.rustPixels` (the old paint
 * kill switch, deleted after the canonical writer became unconditional).
 * This file goes RED while ANY production source still reads either key
 * or its predicate, so the retirement cannot claim "gone" while a read
 * survives - and a re-introduced read reddens immediately.
 *
 * WHY A STATIC CENSUS (the pattern `pixelWriterCensus.test.ts` uses): a
 * boolean flag comparison cannot fail on anything, which is exactly how
 * these two keys outlived their purpose. A read is enumerable, so the
 * contract is enumerable.
 *
 * SCOPE: production TypeScript under `apps/desktop/src` (tests, the wasm
 * package and declarations are excluded by the traversal). The three
 * `.mjs` measurement scripts that still WRITE the keys are retired in
 * the same change as the reads; they live outside this path and are
 * listed in the retirement ledger entry instead.
 */
import { describe, it, expect } from "vitest";
import { readFileSync, readdirSync, statSync, existsSync } from "node:fs";
import { resolve, join, relative } from "node:path";

const APP_SRC = resolve(__dirname, "../../..");

/** Every `.ts`/`.tsx` under the app's `src`, excluding tests and the wasm pkg. */
function productionSources(dir = APP_SRC): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(dir)) {
    if (entry === "__tests__" || entry === "wasm" || entry === "node_modules") continue;
    const full = join(dir, entry);
    if (statSync(full).isDirectory()) {
      out.push(...productionSources(full));
    } else if (/\.tsx?$/.test(entry) && !/\.test\.tsx?$/.test(entry) && !/\.d\.ts$/.test(entry)) {
      out.push(full);
    }
  }
  return out;
}

const readProduction = (file: string): string =>
  readFileSync(file, "utf8").replace(/\/\/[^\n]*/g, "");

/** Every production read of a retired key or its predicate, as `file:line`. */
function retiredKeyReads(): { site: string; token: string }[] {
  const tokens = [
    /["']photrez\.(?:historyBridge|rustPixels)["']/g,
    /\bhistoryBridgeEnabled\b/g,
    /\brustPixelsFlagEnabled\b/g,
  ];
  const sites: { site: string; token: string }[] = [];
  for (const file of productionSources()) {
    const rel = relative(APP_SRC, file);
    const text = readProduction(file);
    for (const token of tokens) {
      for (const match of text.matchAll(token)) {
        const lineNumber = text.slice(0, match.index).split("\n").length;
        sites.push({ site: `${rel}:${lineNumber}`, token: match[0] });
      }
    }
  }
  return sites;
}

/**
 * The rest of the bridge machinery that dies with the key: the per-entry
 * bridge marker, the gating arm inside `stepRustCursor`, and any importer
 * of the retired probe module. Each is a token that must not survive the
 * retirement in production code.
 */
function retiredBridgeMachinery(): { site: string; token: string }[] {
  const tokens = [
    /\blastCommitRecordedInBridge\b/g,
    /\bcommittedRecordedInBridge\b/g,
    /\bbridgeArm\b/g,
    /\bhistoryCursorParity\b/g,
    /\brestoreSnapshotBitmapsByToken\b/g,
  ];
  const sites: { site: string; token: string }[] = [];
  for (const file of productionSources()) {
    const rel = relative(APP_SRC, file);
    const text = readProduction(file);
    for (const token of tokens) {
      for (const match of text.matchAll(token)) {
        const lineNumber = text.slice(0, match.index).split("\n").length;
        sites.push({ site: `${rel}:${lineNumber}`, token: match[0] });
      }
    }
  }
  return sites;
}

const isBridgeKey = (h: { token: string }) => h.token.includes("historyBridge") || h.token.includes("Bridge");

/**
 * The facade opt-out pair (`photrez.facade`, `photrez.facadeAuthority`) is
 * retired with the single-canonical-engine endgame: the keys have no
 * production setters, and the predicates that read them only select the
 * legacy wasm/emulator arms this retirement deletes. Quote-agnostic literal
 * tokens (double, single, backtick) so a re-introduced read in any quoting
 * style reddens. A read is any occurrence except the predicate's own
 * declaration line: the facade predicate collapses to a default-constant
 * declaration while its call sites go to zero, so the declaration may
 * outlive the reads by a commit; every other occurrence counts.
 */
function facadeKeyReads(): { site: string; token: string }[] {
  const tokens = [
    /["'`]photrez\.facade["'`]/g,
    /["'`]photrez\.facadeAuthority["'`]/g,
    /\bisNativeAuthority\b/g,
    /\bisFacadeEnabled\b/g,
  ];
  const sites: { site: string; token: string }[] = [];
  for (const file of productionSources()) {
    const rel = relative(APP_SRC, file);
    const text = readProduction(file);
    for (const token of tokens) {
      for (const match of text.matchAll(token)) {
        const lineNumber = text.slice(0, match.index).split("\n").length;
        const lineText = text.split("\n")[lineNumber - 1] ?? "";
        if (/function (isFacadeEnabled|isNativeAuthority)/.test(lineText)) continue;
        sites.push({ site: `${rel}:${lineNumber}`, token: match[0] });
      }
    }
  }
  return sites;
}

describe("retired runtime keys stay retired", () => {
  it("no production source reads photrez.historyBridge / its predicate", () => {
    const hits = retiredKeyReads().filter(isBridgeKey);
    expect(hits.map((h) => `${h.site} (${h.token})`)).toEqual([]);
  });

  it("no production source reads photrez.rustPixels / its predicate", () => {
    const hits = retiredKeyReads().filter((h) => !isBridgeKey(h));
    expect(hits.map((h) => `${h.site} (${h.token})`)).toEqual([]);
  });

  it("no production source keeps the bridge marker, arm, or the retired probe module", () => {
    const hits = retiredBridgeMachinery();
    expect(hits.map((h) => `${h.site} (${h.token})`)).toEqual([]);
  });

  it("the retired modules are gone from disk", () => {
    expect(
      existsSync(resolve(APP_SRC, "engine/historyBridgeGate.ts")),
      "historyBridgeGate.ts must be deleted with its key",
    ).toBe(false);
    expect(
      existsSync(resolve(APP_SRC, "engine/historyCursorParity.ts")),
      "historyCursorParity.ts must be deleted with its only callers",
    ).toBe(false);
  });
});

describe("retired facade opt-out keys stay retired", () => {
  it("no production source reads photrez.facade", () => {
    const hits = facadeKeyReads().filter((h) => h.token.includes("photrez.facade") && !h.token.includes("Authority"));
    expect(hits.map((h) => `${h.site} (${h.token})`)).toEqual([]);
  });

  it("no production source reads photrez.facadeAuthority", () => {
    const hits = facadeKeyReads().filter((h) => h.token.includes("photrez.facadeAuthority"));
    expect(hits.map((h) => `${h.site} (${h.token})`)).toEqual([]);
  });

  it("no production source reads the isNativeAuthority predicate", () => {
    const hits = facadeKeyReads().filter((h) => h.token === "isNativeAuthority");
    expect(hits.map((h) => `${h.site} (${h.token})`)).toEqual([]);
  });

  it("no production source reads the isFacadeEnabled predicate", () => {
    const hits = facadeKeyReads().filter((h) => h.token === "isFacadeEnabled");
    expect(hits.map((h) => `${h.site} (${h.token})`)).toEqual([]);
  });
});
