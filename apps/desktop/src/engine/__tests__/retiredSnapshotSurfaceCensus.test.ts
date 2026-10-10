// SPDX-License-Identifier: AGPL-3.0-or-later

/**
 * RETIRED SNAPSHOT-SURFACE CENSUS - the new-contract guard for the dead-code
 * cut that removes the snapshot-token history bridge's leftovers.
 *
 * WHAT IT GUARDS. The history panel's jump routes through the native cursor
 * stepper (`navigateHistoryNative`) now, so the model-only pop pair
 * (`undoThroughModel` / `redoThroughModel`) has no production caller, and the
 * snapshot-token payload builder (`buildSnapshotPayload`) plus its DTO module
 * (`snapshotTokenReattach.ts`) have none at all. This file goes RED while ANY
 * production source still names one of the deleted symbols, so the cut cannot
 * claim "gone" while a reference survives - and a re-introduced reference
 * reddens immediately.
 *
 * WHY A STATIC CENSUS (the pattern `flagRetirementCensus.test.ts` uses): a
 * deleted symbol cannot fail on anything once its callers are gone, which is
 * exactly how dead code outlives its purpose. A reference is enumerable, so
 * the contract is enumerable.
 *
 * SCOPE: production TypeScript under `apps/desktop/src` (tests and
 * declarations are excluded by the traversal), plus the on-disk absence of
 * the deleted DTO module.
 */
import { describe, it, expect } from "vitest";
import { readFileSync, readdirSync, statSync, existsSync } from "node:fs";
import { resolve, join, relative } from "node:path";

const APP_SRC = resolve(__dirname, "../../..");

/** Every `.ts`/`.tsx` under the app's `src`, excluding tests and declarations. */
function productionSources(dir = APP_SRC): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(dir)) {
    if (entry === "__tests__" || entry === "node_modules") continue;
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

/** Every production reference to a deleted symbol, as `file:line`. */
function retiredSymbolRefs(): { site: string; token: string }[] {
  const tokens = [
    /\bundoThroughModel\b/g,
    /\bredoThroughModel\b/g,
    /\bbuildSnapshotPayload\b/g,
    /\bsnapshotTokenReattach\b/g,
    /\bBitmapField\b/g,
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

describe("retired snapshot-token history surface stays retired", () => {
  it("no production source names undoThroughModel / redoThroughModel / buildSnapshotPayload / snapshotTokenReattach / BitmapField", () => {
    const hits = retiredSymbolRefs();
    expect(hits.map((h) => `${h.site} (${h.token})`)).toEqual([]);
  });

  it("the deleted DTO module is gone from disk", () => {
    expect(
      existsSync(resolve(APP_SRC, "engine/snapshotTokenReattach.ts")),
      "snapshotTokenReattach.ts must be deleted with its only importer (buildSnapshotPayload)",
    ).toBe(false);
  });
});
