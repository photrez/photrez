// SPDX-License-Identifier: AGPL-3.0-or-later

/**
 * Unit tests for `parseHarnessFlags` (scripts/harness-flag-cleanup.mjs).
 *
 * Every live runner (scripts/live-verify.mjs, scripts/perf-audit-live.mjs,
 * scripts/ipc-frequency-live.mjs) maps PHOTREZ_FLAGS to localStorage entries
 * through this one parser, so a regression here silently changes what every
 * live measurement run boots with. The parser's companion tests were retired
 * along with the migration flags they pinned; this file pins the parser ITSELF
 * - the splitting, the k=v mapping, and the malformed-entry behaviour - with
 * no flag semantics involved.
 */
import { existsSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

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

type FlagPair = { key: string; value: string };

async function loadParser(): Promise<(raw?: string) => FlagPair[]> {
  const mod = (await import(repoFile("scripts/harness-flag-cleanup.mjs"))) as {
    parseHarnessFlags: (raw?: string) => FlagPair[];
  };
  return mod.parseHarnessFlags;
}

const FACADE_PAIRS: FlagPair[] = [
  { key: "photrez.facade", value: "1" },
  { key: "photrez.facadeAuthority", value: "native" },
];

describe("parseHarnessFlags (the shared PHOTREZ_FLAGS parser)", () => {
  it("maps one k=v entry to a photrez.-prefixed pair, value as written", async () => {
    const parse = await loadParser();
    expect(parse("facade=1")).toEqual([{ key: "photrez.facade", value: "1" }]);
    expect(parse("facade=0")).toEqual([{ key: "photrez.facade", value: "0" }]);
  });

  it("splits on spaces, commas, and mixtures of both", async () => {
    const parse = await loadParser();
    expect(parse("facade=1 facadeAuthority=native")).toEqual(FACADE_PAIRS);
    expect(parse("facade=1,facadeAuthority=native")).toEqual(FACADE_PAIRS);
    expect(parse("facade=1,  facadeAuthority=native\n\tbgFlagRoute=0")).toEqual([
      ...FACADE_PAIRS,
      { key: "photrez.bgFlagRoute", value: "0" },
    ]);
  });

  it("keeps every '=' after the first inside the value", async () => {
    const parse = await loadParser();
    expect(parse("custom=a=b")).toEqual([{ key: "photrez.custom", value: "a=b" }]);
  });

  it("drops malformed entries: no '=', empty key, stray separators", async () => {
    const parse = await loadParser();
    expect(parse("rustPixels facade=1 =native , ,")).toEqual([
      { key: "photrez.facade", value: "1" },
    ]);
  });

  it("keeps an explicitly empty value (it is written, so it passes through)", async () => {
    const parse = await loadParser();
    expect(parse("facade=")).toEqual([{ key: "photrez.facade", value: "" }]);
  });

  it("answers an empty list for empty or absent input", async () => {
    const parse = await loadParser();
    expect(parse("")).toEqual([]);
    expect(parse(undefined)).toEqual([]);
    expect(parse("   ,  ")).toEqual([]);
  });
});
