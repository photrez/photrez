# Contributing to Photrez

Thanks for helping improve Photrez! We welcome contributions that help make Photrez a faster, more reliable desktop image editor for creators.

## Ground Rules

- **Scope:** The MVP feature set is frozen; new features ship as post-MVP minor releases. Align proposals with the release roadmap in `README.md`.
- **Layout Stability:** Preserve existing desktop UI structures unless a change explicitly requires UI redesign.
- **Architecture:** Respect runtime ownership boundaries described in `docs/ARCHITECTURE.md`.
- **Focused Changes:** Prefer small, well-tested pull requests over large, unannounced refactors.
- **Dependencies:** Avoid adding external dependencies unless strictly necessary.

## Development Setup

This project uses **Bun** (`v1.4.0`) as its primary package manager and runtime.

> **2026-10-17:** this source tree no longer builds the v0.1.0 desktop app — the
> browser-side runtime was removed ahead of the native rewrite. The v0.1.0
> sources and dev instructions live in the frozen archive at
> [photrez/photrez-legacy](https://github.com/photrez/photrez-legacy).

```bash
bun install          # Install dependencies
```

### Focused Test Commands

```bash
bun run --filter photrez-desktop test:unit    # Frontend unit tests (node project)
cargo test -p photrez-core                    # Rust core unit tests
cargo test --workspace                       # All Rust crate tests
```

## Pull Request Guidelines

- Describe the change clearly in your PR description.
- Include unit and wiring tests for new features or bug fixes.
- Update documentation if user-facing behavior, shortcuts, or setup requirements change.
- Verify performance when modifying paint, export, rendering, or document history logic.

## Commit Messages

Photrez follows the [Conventional Commits](https://www.conventionalcommits.org) spec. The subject line must be a single line in the form:

```
<type>(<scope>): <subject>
```

- **Type** is one of: `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `build`, `ci`, `chore`, `revert`.
- **Scope** (optional) names the affected module, e.g. `feat(editor)`, `fix(core)`.
- **Subject** uses the imperative mood, starts lowercase, is short (≤ 72 chars), and ends without a period.
- A **breaking change** is marked with `!` after the type/scope, or a `BREAKING CHANGE:` footer.

Examples:
- `feat(editor): add Delete-Layer delegation via EditorClient`
- `fix(core): guard zero-dimension print composite`
- `docs: clarify install steps for new contributors`

### Public terminology

Internal planning and milestone terminology that lives in gitignored project docs must **never** appear in public artifacts — source code, commit messages, PR titles, or committed documentation. Describe the change using product/technical terms instead. This rule is enforced by a shared check (`scripts/check-public-terminology.sh`) in both the local `commit-msg` hook and CI, so it cannot be bypassed.

## Code Standards

- **Frontend:** SolidJS with strict TypeScript (TSX). Avoid `any`.
- **History Safety:** Always commit document state to history before executing destructive mutations.
- **Design Token System:** Follow `@theme` design tokens in `docs/DESIGN.md`.

Thank you for contributing to Photrez!
