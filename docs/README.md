# Photrez Documentation Guide

> Use this index to find the document for your task.

---

## Core project documents

| File | Purpose |
|---|---|
| [`README.md`](README.md) | This documentation map |
| [`DESIGN.md`](DESIGN.md) | Product design principles |
| [`UI_GUIDE.md`](UI_GUIDE.md) | Current UI interaction and visual guidance |
| [`ARCHITECTURE.md`](ARCHITECTURE.md) | Native runtime architecture |
| [`CONVENTIONS.md`](CONVENTIONS.md) | Native Rust and GPUI conventions |
| [`FEATURES.md`](FEATURES.md) | Current feature matrix for the native app |
| [`TRANSLATION.md`](TRANSLATION.md) | English and Indonesian catalog translation guide |

## Crate references

| File | Purpose |
|---|---|
| [`../crates/core/README.md`](../crates/core/README.md) | Core ownership, module boundaries, invariants, and test command |
| [`../crates/document/README.md`](../crates/document/README.md) | Document model ownership, non-goals, and test command |
| [`../crates/raster/README.md`](../crates/raster/README.md) | Pixel buffer, tile, kernel, paint, and brush ownership, non-goals, and test command |
| [`../crates/format/README.md`](../crates/format/README.md) | `.ptz` envelope, writer, and codec ownership, non-goals, and test command |
| [`../crates/render/README.md`](../crates/render/README.md) | Compositing and rasterizer ownership, non-goals, and test command |

## Project policies

| File | Purpose |
|---|---|
| [`../CONTRIBUTING.md`](../CONTRIBUTING.md) | Contribution, review, and release rules |
| [`../SECURITY.md`](../SECURITY.md) | Vulnerability reporting |
| [`../CODE_OF_CONDUCT.md`](../CODE_OF_CONDUCT.md) | Community standards |
| [`../CHANGELOG.md`](../CHANGELOG.md) | Release history |
| [`../THIRD_PARTY.md`](../THIRD_PARTY.md) | Third-party notices |

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

The application that preceded this native runtime, and the dated records that
describe it, are preserved in the separate archived repository
[photrez/photrez-legacy](https://github.com/photrez/photrez-legacy).
