# Contributing to Photrez

Thanks for helping improve Photrez! We welcome contributions that help make Photrez a faster, more reliable desktop image editor for creators.

## Ground Rules

- **Scope:** Align proposals with the current product and keep changes focused.
- **Layout Stability:** Preserve existing desktop UI structures unless a change explicitly requires UI redesign.
- **Architecture:** Respect runtime ownership boundaries described in `docs/ARCHITECTURE.md`.
- **Focused Changes:** Prefer small, well-tested pull requests over large, unannounced refactors.
- **Dependencies:** Avoid adding external dependencies unless strictly necessary.

## Development Setup

Install the Rust toolchain specified by [`rust-toolchain.toml`](rust-toolchain.toml), then use the workspace commands below from the repository root.

```bash
cargo build
cargo dev       # Run the app with the dev profile
cargo ui        # UI iteration profile
bacon           # Save-triggered cargo check -p photrez
```

Before opening a pull request, run the workspace gates:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Pull Request Guidelines

- Describe the change clearly in your PR description.
- Include unit and wiring tests for new features or bug fixes.
- Update documentation if user-facing behavior, shortcuts, or setup requirements change.
- Verify performance when modifying paint, export, rendering, or document history logic.

## Commit Messages

Use [Conventional Commits](https://www.conventionalcommits.org) for pull request titles. `.github/workflows/lint-pr.yml` enforces the convention; there is no local commitlint hook.

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

Describe changes with product and technical terms rather than internal planning terminology. The public-surface staleness check is `sh scripts/check-public-terminology.sh --staleness`.

## Code Standards

- **Application:** Native Rust with GPUI; `photrez-core` owns canonical document state, pixels, and history.
- **History Safety:** Always commit document state to history before executing destructive mutations.
- **Design:** Follow the product guidance in `docs/DESIGN.md` and `docs/UI_GUIDE.md`.

Thank you for contributing to Photrez!

## Release

One file owns the version: `crates/photrez/Cargo.toml`. Set it with `cargo xtask set-version <semver>`. The
command refuses a version shape it does not accept, refuses a lower release number than the one already set, and
restores the manifest byte for byte when `cargo metadata` reports something other than what it wrote. It prints
the version it read back.

A tag that starts with `v` runs `.github/workflows/release.yml`. The `check-tag` job compares the tag with
`cargo pkgid -p photrez` and stops when they disagree. Each platform job then builds with
`cargo build --release -p photrez`, packs that binary with `LICENSE`, `NOTICE` and `THIRD_PARTY.md`, and runs the
archive check below on its own output. The `finalize` job writes `SHA256SUMS`, uploads it, re-downloads the
release and verifies the sums, then sets the release notes from the changelog section that matches the tag; it
refuses a section that is missing or still marked unreleased. Photrez is not code-signed, so those checksums are
the integrity check a user can perform.

Check a build before you publish it:

```
sh scripts/check-release-archive.sh <archive> <windows|macos|linux>
sh scripts/check-release-archive.sh --binary <path> <windows|macos|linux>
sh scripts/check-release-archive.sh --imports <listing> <windows|macos|linux>
```

The first command confirms the archive is a real build, carries the three licence files and holds exactly one
runtime. The other two list the dynamic libraries the shipped binary asks for and refuse a runtime this product
no longer contains: `--binary` reads a binary on its own machine, and `--imports` reads a listing captured on
another platform, so a macOS or Linux answer can be reviewed anywhere. Each of them exits non-zero and names the
cause.
