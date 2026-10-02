// Arm lockdown: every Rust `Command` variant must have exactly one TS `Command`
// union arm, the ARM_MIRROR routing list must match the Rust source, and the
// snake_case wire keys the bridge emits must match the Rust FIELD names. Every
// name on every side is parsed from its source file at runtime, never restated,
// so renaming a Rust field or a TS key without the other fails here first.
import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { ARM_MIRROR } from "../armMirror";

// Anchored to this file (vitest provides __dirname); import.meta.url is
// rewritten to a web path by vite and cannot reach files outside the app root.
const COMMAND_RS = resolve(
  __dirname,
  "../../../../../../crates/core/src/command.rs",
);
const TYPES_TS = resolve(__dirname, "../types.ts");
const BRIDGE_TS = resolve(__dirname, "../bridge.ts");

// Wire name (serde camelCase rename) of a PascalCase Rust variant.
function wireName(variant: string): string {
  return variant.charAt(0).toLowerCase() + variant.slice(1);
}

/** Wire names of every `pub enum Command` variant in command.rs. */
function nativeArms(): string[] {
  // /\r?\n/ because core.autocrlf=true can hand back CRLF working trees.
  const lines = readFileSync(COMMAND_RS, "utf8").split(/\r?\n/);
  const start = lines.findIndex((line) => line === "pub enum Command {");
  if (start < 0) throw new Error("pub enum Command not found in command.rs");
  const arms: string[] = [];
  for (let i = start + 1; i < lines.length; i += 1) {
    const line = lines[i];
    if (line === "}") break;
    // Exactly-4-space indent = variant line; fields sit deeper, attrs/comments
    // start with # or /.
    const match = /^ {4}([A-Z][A-Za-z0-9]*)(?: \{|,)/.exec(line);
    if (match) arms.push(wireName(match[1]));
  }
  if (arms.length === 0) throw new Error("no Command variants parsed from command.rs");
  return arms;
}

/** `type: "..."` discriminant literals of the TS `Command` union. */
function commandTypes(): string[] {
  const lines = readFileSync(TYPES_TS, "utf8").split(/\r?\n/);
  const start = lines.findIndex((line) => line === "export type Command =");
  if (start < 0) throw new Error("export type Command not found in types.ts");
  const types: string[] = [];
  for (let i = start + 1; i < lines.length; i += 1) {
    const line = lines[i];
    if (line.startsWith("export type ")) break;
    const match = /type: "([a-z][A-Za-z0-9]*)"/.exec(line);
    if (match) types.push(match[1]);
  }
  if (types.length === 0) throw new Error("no command type literals parsed from types.ts");
  return types;
}

const NATIVE_ARMS = nativeArms();
const COMMAND_TYPES = commandTypes();

/** Entries of `native` that have no counterpart in `types` (the loop under test). */
function unmappedArms(native: readonly string[], types: readonly string[]): string[] {
  return native.filter((arm) => !types.includes(arm));
}

/**
 * Field names of one `pub enum Command` variant, as they appear on the wire.
 *
 * The command enum's VARIANT names are camelCase (serde rename_all) but its FIELD
 * names are not renamed - so a Rust field is its own wire key verbatim.
 */
function rustVariantFields(variant: string): string[] {
  const lines = readFileSync(COMMAND_RS, "utf8").split(/\r?\n/);
  const start = lines.findIndex((line) => line === "pub enum Command {");
  if (start < 0) throw new Error("pub enum Command not found in command.rs");
  const open = lines.findIndex(
    (line, i) => i > start && new RegExp(`^ {4}${variant} \\{$`).test(line),
  );
  if (open < 0) throw new Error(`Command variant ${variant} not found in command.rs`);
  const fields: string[] = [];
  for (let i = open + 1; i < lines.length; i += 1) {
    const line = lines[i];
    if (line === "    }") break;
    // Attributes (#[serde(...)]) and doc comments sit between fields; a field is
    // 8-space-indented `name: Type,`.
    const match = /^ {8}([a-z][a-z0-9_]*):\s/.exec(line);
    if (match) fields.push(match[1]);
  }
  if (fields.length === 0) throw new Error(`no fields parsed for ${variant}`);
  return fields;
}

/**
 * The wire keys `toRustEnvelope` emits for one command variant.
 *
 * Read from the bridge source rather than from a serialized envelope, because the
 * defect this guards is a MISSING key: calling toRustEnvelope would only prove the
 * keys it does emit, and a dropped key would be invisible. Scanning the mapped
 * object literal means a deleted mapping shows up as a missing key here.
 */
function bridgeMappedKeys(variant: string): string[] {
  const lines = readFileSync(BRIDGE_TS, "utf8").split(/\r?\n/);
  const start = lines.findIndex((line) => line.trim() === `case "${variant}":`);
  if (start < 0) throw new Error(`bridge.ts has no case for ${variant}`);
  const keys: string[] = [];
  for (let i = start + 1; i < lines.length; i += 1) {
    const line = lines[i].trim();
    if (line === "break;") break;
    // `key: value` inside the mapped object literal; `type:` is the discriminant
    // and is not a field name.
    const match = /^([a-z][a-z0-9_]*):\s/.exec(line);
    if (match && match[1] !== "type") keys.push(match[1]);
  }
  return keys;
}

describe("arm lockdown: Rust Command enum mirrors the TS Command union", () => {
  it("maps every native arm to a TS union type", () => {
    expect(unmappedArms(NATIVE_ARMS, COMMAND_TYPES)).toEqual([]);
  });

  it("has no TS union type without a native arm", () => {
    expect(unmappedArms(COMMAND_TYPES, NATIVE_ARMS)).toEqual([]);
  });

  it("keeps ARM_MIRROR in sync with the Rust enum source", () => {
    expect([...ARM_MIRROR].sort()).toEqual([...NATIVE_ARMS].sort());
  });

  // Falsifiability: proves the mapping loop fails when a mapping is removed
  // instead of passing vacuously on a shrunken input.
  it("reports the native arm whose TS mapping was removed", () => {
    const pruned = COMMAND_TYPES.filter((type) => type !== "addLayer");
    expect(unmappedArms(NATIVE_ARMS, pruned)).toEqual(["addLayer"]);
  });
});

// The two sides of this binding each carry their literals independently: the Rust
// field names in command.rs and the snake_case keys the bridge emits. Rename one
// side and leave the other, and nothing in either suite notices - the bridge
// builds a well-formed command with the wrong key, and Rust's #[serde(default)]
// silently accepts the absence. That exact shape shipped green here before, so the
// names are bound below rather than restated.
describe("wire-key lockdown: bridge keys match the Rust field names", () => {
  const VARIANT = "recordExternalTransition";
  const RUST = "RecordExternalTransition";

  it("emits exactly the field names the Rust variant declares", () => {
    expect(bridgeMappedKeys(VARIANT).sort()).toEqual(rustVariantFields(RUST).sort());
  });

  it("carries the host document-size halves under their Rust names", () => {
    const fields = rustVariantFields(RUST);
    const keys = bridgeMappedKeys(VARIANT);
    // Named explicitly because these are the two the type-checker cannot see:
    // the mapped object is loosely typed, and an absent key is a legal command.
    expect(fields).toContain("doc_size_before");
    expect(fields).toContain("doc_size_after");
    expect(keys).toContain("doc_size_before");
    expect(keys).toContain("doc_size_after");
  });

  it("does not leak the camelCase TS field names onto the wire", () => {
    const keys = bridgeMappedKeys(VARIANT);
    expect(keys).not.toContain("docSizeBefore");
    expect(keys).not.toContain("docSizeAfter");
  });

  // Falsifiability: the binding must actually fail on disagreement, in BOTH
  // directions, instead of passing because the compared set happened to be
  // empty. A Rust rename with the TS key left behind, and a TS rename with the
  // Rust field left behind, both have to redden.
  it("reddens when the Rust field is renamed and the bridge key is not", () => {
    const renamedRust = rustVariantFields(RUST).map((f) =>
      f === "doc_size_before" ? "doc_size_pre" : f,
    );
    const drifted = rustVariantFields(RUST).filter(
      (f) => !bridgeMappedKeys(VARIANT).includes(f),
    );
    expect(drifted).toEqual([]);
    // With the Rust side renamed, the key the bridge still sends no longer matches.
    const mismatch = renamedRust.filter((f) => !bridgeMappedKeys(VARIANT).includes(f));
    expect(mismatch).toEqual(["doc_size_pre"]);
  });

  it("reddens when a bridge mapping is deleted", () => {
    const keys = bridgeMappedKeys(VARIANT);
    const pruned = keys.filter((k) => k !== "doc_size_after");
    const missing = rustVariantFields(RUST).filter((f) => !pruned.includes(f));
    expect(missing).toEqual(["doc_size_after"]);
  });
});

/**
 * Wire-key lockdown for the RESULT direction, which the `Command` lockdown above
 * does not cover: that one binds the keys the bridge EMITS, while these bind the
 * keys Rust RETURNS against the ones the TS `CommandResult` declares.
 *
 * This exists because the crop-undo raster carrier is a returned field
 * (`CommandResult::external_token` -> `CommandResult.externalToken`). That is the
 * same class of silent break as the size pair: `#[serde(skip_serializing_if)]`
 * plus an optional TS field means a rename on either side drops the field with no
 * error anywhere, and the host then simply never restores the pixels - a green
 * suite over a wrong picture. Both names are parsed from source, never restated.
 */
describe("result wire-key lockdown: CommandResult fields match the TS result type", () => {
  /** Field names of `pub struct CommandResult` in command.rs, as serde emits them. */
  function rustResultFields(): string[] {
    const lines = readFileSync(COMMAND_RS, "utf8").split(/\r?\n/);
    const start = lines.findIndex((line) => line.startsWith("pub struct CommandResult {"));
    if (start < 0) throw new Error("pub struct CommandResult not found in command.rs");
    const fields: string[] = [];
    for (let i = start + 1; i < lines.length; i += 1) {
      const line = lines[i];
      if (line === "}") break;
      // Field lines sit at 4-space indent; attributes and comments do not.
      const match = /^ {4}pub ([a-z][A-Za-z0-9_]*):/.exec(line);
      if (match) {
        // `#[serde(rename_all = "camelCase")]` on the struct makes every field
        // camelCase on the wire, which is what the TS side declares.
        const snake = match[1];
        fields.push(snake.replace(/_([a-z0-9])/g, (_m, c: string) => c.toUpperCase()));
      }
    }
    if (fields.length === 0) throw new Error("no CommandResult fields parsed from command.rs");
    return fields;
  }

  /** Field names declared by the TS `CommandResult` type. */
  function tsResultFields(): string[] {
    const lines = readFileSync(TYPES_TS, "utf8").split(/\r?\n/);
    const start = lines.findIndex((line) => line === "export type CommandResult = {");
    if (start < 0) throw new Error("export type CommandResult not found in types.ts");
    const fields: string[] = [];
    for (let i = start + 1; i < lines.length; i += 1) {
      const line = lines[i];
      if (line === "};") break;
      const match = /^ {2}([a-zA-Z][A-Za-z0-9_]*)\??:/.exec(line);
      if (match) fields.push(match[1]);
    }
    if (fields.length === 0) throw new Error("no CommandResult fields parsed from types.ts");
    return fields;
  }

  it("emits exactly the field names the TS result type declares", () => {
    expect(rustResultFields().sort()).toEqual(tsResultFields().sort());
  });

  it("carries the crop-undo raster token under both names", () => {
    // Named explicitly: this is the field whose absence silently disables the
    // raster restore, and neither the type-checker nor any error path sees it.
    expect(rustResultFields()).toContain("externalToken");
    expect(tsResultFields()).toContain("externalToken");
  });

  // Falsifiability, both directions - the same requirement the Command lockdown
  // states. A rename on either side must be detected, not silently tolerated.
  it("reddens when the Rust field is renamed and the TS field is not", () => {
    const renamed = rustResultFields().map((f) => (f === "externalToken" ? "externalTok" : f));
    const mismatch = renamed.filter((f) => !tsResultFields().includes(f));
    expect(mismatch).toEqual(["externalTok"]);
  });

  it("reddens when the TS field is removed", () => {
    const pruned = tsResultFields().filter((f) => f !== "externalToken");
    const missing = rustResultFields().filter((f) => !pruned.includes(f));
    expect(missing).toEqual(["externalToken"]);
  });
});
