// Arm lockdown: every Rust `Command` variant must have exactly one TS `Command`
// union arm, and the ARM_MIRROR routing list must match the Rust source. Both
// sides are parsed from their source files at runtime, so adding a native arm
// without a TS mapping (or dropping a mapping) fails here before it ships.
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
