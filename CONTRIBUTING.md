# Contributing to Photrez

Thanks for helping improve Photrez! We welcome contributions that help make Photrez a faster, more reliable desktop image editor for creators.

## Ground Rules

- **Scope:** Keep changes aligned with product goals in `docs/spec/product-scope.md`.
- **Layout Stability:** Preserve existing desktop UI structures unless a change explicitly requires UI redesign.
- **Architecture:** Respect runtime ownership boundaries described in `docs/ARCHITECTURE.md`.
- **Focused Changes:** Prefer small, well-tested pull requests over large, unannounced refactors.
- **Dependencies:** Avoid adding external dependencies unless strictly necessary.

## Development Setup

This project uses **Bun** (`v1.3.14`) as its primary package manager and runtime.

```bash
bun install          # Install dependencies
bun run tauri dev    # Launch desktop app in dev mode
bun run verify       # Run full verification gate (tests + build)
```

### Focused Test Commands

```bash
bun run --filter photrez-desktop test --run   # Frontend unit/component tests
bun run build                                 # Frontend build verification
cargo test -p photrez-core                    # Rust core unit tests
cargo test --workspace                       # All Rust crate tests
```

## Pull Request Guidelines

- Describe the change clearly in your PR description.
- Include unit and wiring tests for new features or bug fixes.
- Update documentation if user-facing behavior, shortcuts, or setup requirements change.
- Verify performance when modifying paint, export, rendering, or document history logic.

## Code Standards

- **Frontend:** SolidJS with strict TypeScript (TSX). Avoid `any`.
- **History Safety:** Always commit document state to history before executing destructive mutations.
- **Design Token System:** Follow `@theme` design tokens in `docs/DESIGN.md`.

Thank you for contributing to Photrez!
