# Photrez Documentation Guide

> Use this index to find the relevant project documentation. Load focused references rather than every file.

---

## Core project documents

| File | Purpose |
|---|---|
| [`INDEX.md`](INDEX.md) | This documentation map |
| [`DESIGN.md`](DESIGN.md) | Product design principles |
| [`UI_GUIDE.md`](UI_GUIDE.md) | Current UI interaction and visual guidance |
| [`ARCHITECTURE.md`](ARCHITECTURE.md) | Native runtime architecture |
| [`CONVENTIONS.md`](CONVENTIONS.md) | Native Rust and GPUI conventions |
| [`FEATURES.md`](FEATURES.md) | Current feature matrix for the native app |
| [`TRANSLATION.md`](TRANSLATION.md) | English and Indonesian catalog translation guide |

## Build reference

| File | Purpose |
|---|---|
| [`reference/cargo-storage.md`](reference/cargo-storage.md) | Cargo build storage and cache behavior |
| [`reference/command-contract-spec.md`](reference/command-contract-spec.md) | Where the command contract lives now |
| [`reference/design-tokens.md`](reference/design-tokens.md) | Where interface values live now |
| [`reference/render-export-coverage.md`](reference/render-export-coverage.md) | Where render and export coverage is pinned |

## Measurements and document format

| File | Purpose |
|---|---|
| [`frame-pacing-baseline.md`](frame-pacing-baseline.md) | How frame pacing and input latency are measured in the running app |
| [`ptz-migration.md`](ptz-migration.md) | The `.ptz` format version rule and what a format change must ship |

---

> Internal material — plans, decisions, specifications and research notes — is
> intentionally **not** published here. It is gitignored by design;
> `CONTRIBUTING.md` describes the public rules.

The application that preceded this native runtime, and the dated records that
describe it, are preserved in the separate archived repository
[photrez/photrez-legacy](https://github.com/photrez/photrez-legacy).
