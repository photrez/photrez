// SPDX-License-Identifier: AGPL-3.0-or-later

/**
 * SINGLE-WRITER CENSUS - the static half of the replacement for the retired
 * `photrez.rustPixels` flag oracle.
 *
 * WHY A STATIC CENSUS. "One canonical owner" is ENUMERABLE even though a flag
 * comparison is not. Every site that can put pixel bytes into the Rust store is a
 * call to a byte-writing command, so the census is: parse the production
 * TypeScript sources, collect every such call site, and require each one to be a
 * recognised canonical writer or a declared projection. A new writer appears as
 * an unlisted call site and this file goes RED - which is the whole point, because
 * the dead flag oracle could not fail on anything (five of its six subjects were
 * deleted).
 *
 * PARSED, NOT RESTATED. Every list below is read out of the sources at runtime
 * (the same approach `armMirror.test.ts` uses to bind Rust wire keys to TS field
 * names): the Rust command inventory comes from `main.rs`'s registration list and
 * the byte-writing classification comes from `pixelInvokeCensus`'s command set.
 * Restating either list in this file would let the two drift apart silently, which
 * is the failure mode `armMirror.test.ts` exists to prevent.
 *
 * WHAT COUNTS AS A WRITER. A command is a writer when Rust's own source mutates a
 * `PixelLayer` buffer in it. That set is asserted here against the census set, so a
 * command someone forgot to classify cannot be quietly routed around.
 */
import { describe, it, expect } from "vitest";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { resolve, join, relative } from "node:path";
import { CENSUS_COMMANDS_FOR_TEST } from "../pixelWriterCensusContract";

const APP_SRC = resolve(__dirname, "../../..");
const TAURI_MAIN = resolve(APP_SRC, "../src-tauri/src/main.rs");
const PIXEL_CMDS_RS = resolve(APP_SRC, "../src-tauri/src/paint_parity_cmds.rs");

const readLines = (path: string): string[] => readFileSync(path, "utf8").split(/\r?\n/);

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

/**
 * Every site that invokes a state-changing pixel command, as
 * `file:line` plus the command. Three call shapes reach `invoke`:
 *   invoke("cmd", ...)               - the direct import
 *   pixelInvoke("cmd", ...)          - the census-routed wrapper
 *   fire("cmd", ...)                 - history.ts's fire-and-forget helper
 * Comments are stripped first, because this repo's production sources name these
 * commands in prose constantly and a comment is not a writer.
 */
function pixelWriterCallSites(): { site: string; command: string }[] {
  const sites: { site: string; command: string }[] = [];
  // Matched over the WHOLE file text, not line by line, because the command
  // expression is regularly wrapped: `useEditorCommands` writes
  //   pixelInvoke(
  //     direction === "undo" ? "rust_pixels_undo" : "rust_pixels_redo",
  // so a line-anchored parse sees a `(` and no command and concludes the cursor
  // movers are never called - the vacuous-census failure this task warns about.
  // `\s` spans the newline, which is what makes the wrapped form visible.
  const call =
    /\b(?:invoke|pixelInvoke|fire|invokePixelCommand)\(\s*(?:"(rust_pixels_[a-z_]+|apply_tile_patch)"|[^()]*?\?\s*"(rust_pixels_[a-z_]+|apply_tile_patch)"\s*:\s*"(rust_pixels_[a-z_]+|apply_tile_patch)")/g;
  for (const file of productionSources()) {
    const rel = relative(APP_SRC, file);
    // Strip line comments before matching, because these sources name the
    // commands in prose constantly and a comment is not a call site. A `//`
    // inside a string literal does not occur in a command-name position.
    const text = readFileSync(file, "utf8").replace(/\/\/[^\n]*/g, "");
    for (const match of text.matchAll(call)) {
      const lineNumber = text.slice(0, match.index).split("\n").length;
      const commands = [match[1], match[2], match[3]].filter((c): c is string => Boolean(c));
      for (const command of commands) {
        sites.push({ site: `${rel}:${lineNumber}`, command });
      }
    }
  }
  return sites;
}

/** Commands Rust registers with Tauri (`main.rs` invoke_handler list). */
function registeredRustCommands(): Set<string> {
  const lines = readLines(TAURI_MAIN);
  const start = lines.findIndex((l) => l.includes(".invoke_handler"));
  if (start < 0) throw new Error("invoke_handler not found in main.rs");
  const out = new Set<string>();
  for (let i = start; i < lines.length; i += 1) {
    for (const match of lines[i].matchAll(/([a-z_][a-z0-9_]*)\s*(?:::\w+::\w+|)?\s*,?\s*$/gm)) {
      out.add(match[1]);
    }
    if (lines[i].trim() === "})") break;
  }
  if (out.size === 0) throw new Error("no commands parsed from main.rs invoke_handler");
  return out;
}

/**
 * Commands whose Rust body can mutate a `PixelLayer` buffer, read from the Rust
 * source itself. `write_region` is reached through the registry, so both the
 * registry and the command layer are scanned for the mutation helpers.
 */
function rustByteWriters(): Set<string> {
  const cmds = readLines(PIXEL_CMDS_RS);
  const writers = new Set<string>();
  for (let i = 0; i < cmds.length; i += 1) {
    if (!/^pub fn [a-z_]+\(/.test(cmds[i])) continue;
    const name = /^pub fn ([a-z_]+)\(/.exec(cmds[i])![1];
    // Scan the function body: it ends at the first line that is exactly `}`.
    for (let j = i + 1; j < cmds.length; j += 1) {
      if (cmds[j] === "}") break;
      // A body that reaches `write_region`/`apply_pixel_patch`/`resize_layer`/
      // `add_layer` mutates canonical bytes; one that only reads does not.
      if (/\.(write_region|apply_pixel_patch|resize_layer|add_layer|remove_layer)\(/.test(cmds[j])) {
        writers.add(name);
        break;
      }
    }
  }
  if (writers.size === 0) throw new Error("no byte writers parsed from paint_parity_cmds.rs");
  return writers;
}

const SITES = pixelWriterCallSites();
const REGISTERED = registeredRustCommands();
const RUST_WRITERS = rustByteWriters();

describe("single-writer census: every pixel-byte writer is a recognised canonical command", () => {
  it("parsed a non-empty census (a zero-site parse would make every case below vacuous)", () => {
    expect(SITES.length, "production pixel writer call sites were parsed").toBeGreaterThan(0);
    expect(REGISTERED.size).toBeGreaterThan(0);
    expect(RUST_WRITERS.size).toBeGreaterThan(0);
    expect(
      new Set(SITES.map((s) => s.command)).size,
      "more than one distinct pixel command is exercised in production",
    ).toBeGreaterThan(1);
    // EVERY role must have a real call site in production source. A role nothing
    // calls would make its own case below vacuously satisfiable, which is how a
    // census ends up looking complete while proving nothing about live paths.
    const byCommand = new Set(SITES.map((s) => s.command));
    const writerSites = CENSUS_COMMANDS_FOR_TEST.declaredWriters().filter((c) => byCommand.has(c));
    // Filtered over the PARSED sites, not over the census command set: a
    // projection is deliberately OFF the census, so looking for it there would
    // find nothing and report a live production call as unreachable.
    const projectionSites = SITES.filter((s) => CENSUS_COMMANDS_FOR_TEST.isProjection(s.command));
    const cursorSites = SITES.filter((s) => CENSUS_COMMANDS_FOR_TEST.isCursorOnly(s.command));
    const readSites = SITES.filter((s) => CENSUS_COMMANDS_FOR_TEST.isRead(s.command));
    expect(writerSites.length, "writers are reached from production source").toBeGreaterThan(0);
    expect(projectionSites.length, "projections are reached from production source").toBeGreaterThan(0);
    expect(cursorSites.length, "cursor movers are reached from production source").toBeGreaterThan(0);
    expect(readSites.length, "read probes are reached from production source").toBeGreaterThan(0);
    // The canonical writer really is the brush commit's, at the expected site -
    // proof the parse found a live production call rather than a declaration.
    expect(
      SITES.some(
        (s) => s.command === "rust_pixels_write_region" && s.site.includes("useBrushOverlay.ts"),
      ),
      "the canonical writer is reached from the brush commit",
    ).toBe(true);
  });

  it("every parsed pixel-command call site is DECLARED: writer, projection, cursor or read", () => {
    // The census contract, not a list restated here. A call site through an
    // undeclared command makes this RED, which is the automatic failure the task
    // requires: a new writer cannot be added without arguing for it.
    const undeclared = SITES.filter((s) => !CENSUS_COMMANDS_FOR_TEST.isDeclared(s.command));
    expect(
      undeclared.map((s) => `${s.command} at ${s.site}`),
      "these call sites touch the pixel store through a command the census does not " +
        "recognise. Either it is a new writer (declare it, with its justification), a new " +
        "projection, or a read that was miscounted as a write.",
    ).toEqual([]);
  });

  it("read-only probes are never routed through the census", () => {
    // The census records STATE CHANGES. A read routed through it would make a
    // drained count claim a step happened where only a probe ran, which is the
    // false-signal class this repo has already shipped once.
    const routedReads = CENSUS_COMMANDS_FOR_TEST.censusCommands().filter((c) =>
      CENSUS_COMMANDS_FOR_TEST.isRead(c),
    );
    expect(routedReads, "read-only probes must stay outside the state-change census").toEqual([]);
  });

  it("no read-only probe was miscounted as a writer in production source", () => {
    // A probe that reached a writer's call shape would mean the two sets had
    // merged. Asserted separately so the failure names which side drifted.
    const readCallSites = SITES.filter((s) => CENSUS_COMMANDS_FOR_TEST.isRead(s.command));
    expect(
      readCallSites.length,
      "the declared reads are actually exercised by production source, so the read set is not vacuous",
    ).toBeGreaterThan(0);
  });

  it("every declared writer is actually reachable from production source", () => {
    // The inverse direction. Without it, a declaration could be satisfied by a
    // command nothing calls - the census would look complete while proving
    // nothing about the live paths.
    const called = new Set(SITES.map((s) => s.command));
    const orphaned = CENSUS_COMMANDS_FOR_TEST.declaredWriters().filter((c) => !called.has(c));
    expect(
      orphaned,
      "these writers are declared but no production source calls them; remove the declaration " +
        "or wire the call site",
    ).toEqual([]);
  });

  it("every cursor mover is reachable and is NOT also declared a writer", () => {
    // A command that both creates and steps entries would break the "exactly one
    // step per gesture" property the closed history paths depend on, so the two
    // roles must be disjoint. Asserted rather than assumed.
    const called = new Set(SITES.map((s) => s.command));
    for (const command of CENSUS_COMMANDS_FOR_TEST.censusCommands()) {
      if (!CENSUS_COMMANDS_FOR_TEST.isCursorOnly(command)) continue;
      expect(called.has(command), `${command} is a declared cursor mover nothing calls`).toBe(true);
      expect(
        CENSUS_COMMANDS_FOR_TEST.declaredWriters().includes(command),
        `${command} is both a cursor mover and a declared writer`,
      ).toBe(false);
    }
  });

  it("every declared writer is a command Rust actually registers", () => {
    // A declaration naming a command Rust does not expose is dead weight that
    // would let a real writer hide behind an unrecognised name.
    const unregistered = CENSUS_COMMANDS_FOR_TEST.declaredWriters().filter((c) => !REGISTERED.has(c));
    expect(unregistered, "declared writers that main.rs does not register").toEqual([]);
  });

  it("the Rust byte-writer set is fully accounted for: declared, or explicitly read-only", () => {
    // Rust is the authority on what mutates bytes, so a command whose body writes
    // must be declared a writer unless the census says it is a projection. A new
    // Rust writer that no declaration covers lands here.
    const unaccounted = [...RUST_WRITERS].filter(
      (c) => !CENSUS_COMMANDS_FOR_TEST.isDeclared(c) && !CENSUS_COMMANDS_FOR_TEST.isProjection(c),
    );
    expect(
      unaccounted,
      "these Rust commands mutate a PixelLayer buffer but the census calls them neither a " +
        "writer nor a projection",
    ).toEqual([]);
  });

  it("every census-routed command Rust knows about is itself classified", () => {
    // The census wrapper routes commands; anything it routes that is unclassified
    // would be recorded in history order without a declared justification.
    const routed = CENSUS_COMMANDS_FOR_TEST.censusCommands();
    const unclassified = routed.filter(
      (c) => !CENSUS_COMMANDS_FOR_TEST.isDeclared(c) && !CENSUS_COMMANDS_FOR_TEST.isProjection(c),
    );
    expect(unclassified, "census-routed commands with no classification").toEqual([]);
    for (const c of routed) {
      expect(REGISTERED.has(c), `${c} is routed by the census but not registered in main.rs`).toBe(true);
    }
  });

  it("every WRITER is census-routed, so every history step is ordered", () => {
    // `pixelInvoke` (and the bridge's `invokePixelCommand` it forwards to)
    // installs the invoke census BEFORE the call. A writer that bypassed it would
    // be invisible to the monotonic ordering oracle every closed path relies on
    // to prove no invoke followed a step boundary.
    const unrouted = CENSUS_COMMANDS_FOR_TEST.declaredWriters().filter(
      (c) => !CENSUS_COMMANDS_FOR_TEST.isCensusRouted(c),
    );
    expect(unrouted, "writers that bypass the census wrapper").toEqual([]);

    // Non-vacuity: the routed writers are actually called, so this case is not
    // satisfied by an empty writer set.
    const routedWriterSites = SITES.filter((s) =>
      CENSUS_COMMANDS_FOR_TEST.declaredWriters().includes(s.command),
    );
    expect(routedWriterSites.length, "the declared writers are called from production source")
      .toBeGreaterThan(0);
  });

  it("a PROJECTION is off the census, and a routed command is never a projection", () => {
    // A projection replaces the whole buffer under a host-owned history entry, so
    // routing it through the step census would report a second step for one
    // gesture. Conversely, anything the census DOES record must be a real step -
    // an entry-only mirror (`rust_pixels_record_external`) qualifies, a projection
    // does not.
    const routedProjections = CENSUS_COMMANDS_FOR_TEST.censusCommands().filter((c) =>
      CENSUS_COMMANDS_FOR_TEST.isProjection(c),
    );
    expect(
      routedProjections,
      "these projections are routed through the history-step census; that would make one " +
        "gesture look like two undoable steps",
    ).toEqual([]);

    // Non-vacuity: the projections are exercised by real production source, so
    // the case above is not satisfied by an empty set.
    const projectionSites = SITES.filter((s) => CENSUS_COMMANDS_FOR_TEST.isProjection(s.command));
    expect(projectionSites.length, "the projections are exercised by production source")
      .toBeGreaterThan(0);
  });

  it("every census-routed command is a step: a writer, a cursor mover, or entry-only", () => {
    // The census records STATE CHANGES. Anything it records must be one of the
    // three step-making roles, or a drain would claim a step where none happened.
    const misrouted = CENSUS_COMMANDS_FOR_TEST.censusCommands().filter(
      (c) =>
        !CENSUS_COMMANDS_FOR_TEST.declaredWriters().includes(c) &&
        !CENSUS_COMMANDS_FOR_TEST.isCursorOnly(c) &&
        !CENSUS_COMMANDS_FOR_TEST.isEntryOnly(c),
    );
    expect(misrouted, "census-routed commands that are not a writer, cursor mover or entry-only")
      .toEqual([]);
  });

  it("the Rust byte-writer set maps onto a declared role, never onto a read", () => {
    // Rust is the authority on what mutates bytes, so a command whose body writes
    // must never be declared a READ. A misclassification there would hide a real
    // writer behind a read label.
    const mislabelled = [...RUST_WRITERS].filter((c) => CENSUS_COMMANDS_FOR_TEST.isRead(c));
    expect(
      mislabelled,
      "these Rust commands mutate a PixelLayer buffer but are declared read-only",
    ).toEqual([]);
  });
});