# docs/INDEX.md — Photrez Documentation Guide

> Use this index to find the relevant project documentation. Load focused references rather than every file.

---

## Core project documents

| File | Purpose |
|---|---|
| `DESIGN.md` | Design system — how the product should look and behave |
| `UI_GUIDE.md` | How to use the UI — interaction and visual guidance |
| `ARCHITECTURE.md` | How it is built — runtime architecture |
| `CONVENTIONS.md` | How to contribute — code patterns and domain behavior |
| `FEATURES.md` | Feature implementation status |
| `TRANSLATION.md` | en/id translation guide (describes the current Tauri-era layout; rewritten when the native app lands) |
| `ptz-migration.md` | `.ptz` format versions and the compatibility contract |
| `frame-pacing-baseline.md` | Dated runtime measurements — an append-only record |

## Runtime references (`reference/`)

These describe the **shipped** Tauri + WebGL2 runtime and the Tailwind-era UI. They stay until the native UI replaces what they document; check their status before relying on them.

| File | Read when... |
|---|---|
| `reference/command-contract-spec.md` | Working on the Tauri shell IPC contract |
| `reference/design-tokens.md` | Working on the current UI's styling and token values |
| `reference/render-export-parity-matrix.md` | Checking WebGL preview / Canvas2D export parity |

---

> Internal material — plans, decisions, specifications, research, and archive — is intentionally **not** published here. It is gitignored by design; `CONTRIBUTING.md` describes the public rules.
