# Contributing to Photrez

Thanks for helping improve Photrez! We welcome contributions that help make Photrez a faster, more reliable desktop image editor for creators.

## Ground Rules

- **Scope:** Align proposals with the current product and keep changes focused.
- **Layout Stability:** Preserve existing desktop UI structures unless a change explicitly requires UI redesign.
- **Architecture:** Respect runtime ownership boundaries described in `docs/ARCHITECTURE.md`.
- **Focused Changes:** Prefer small, well-tested pull requests over large, unannounced refactors.
- **Dependencies:** Avoid adding external dependencies unless strictly necessary.

## Development Setup

Install the Rust toolchain specified by [`rust-toolchain.toml`](rust-toolchain.toml), then use the workspace commands below from the repository root. Run `cargo xtask doctor` once on a fresh clone: it prints the `rustc` and `cargo` versions, the linker directory, and whether `bacon` is on `PATH`, and writes nothing.

```bash
cargo build
cargo dev             # Run the app with the dev profile
cargo ui              # UI iteration profile
cargo xtask dev       # Same as cargo dev, with the GNU linker directory on PATH
cargo xtask watch     # Rebuild and restart the app on save (hot reload loop)
bacon                 # Save-triggered cargo check -p photrez
```

Standard Cargo is the default build tool. Any Cargo build works here as long
as the GNU linker directory can be found; the dev loop does not depend on a
particular Cargo binary. If your machine's `PATH` lacks that directory, use
`cargo xtask dev` instead; it prepends it to the child process only. Our
internal machines may run a Cargo build with shared dependencies for faster
rebuilds and disk self-cleaning; it is run as an isolated binary with
auto-update disabled and the same PATH scaffold. Contributors do not need it.

### Workspace tasks

`cargo xtask` is the task crate for this workspace. It needs only `cargo`,
`rustc`, and the repository itself, so it works on a fresh clone with no
extra install. It holds only the tasks plain cargo cannot do on its own:
running the app with the linker directory on `PATH`, watching rebuilds
through `bacon`, probing the environment, and bumping the version. Check,
clippy, fmt, test and doc stay plain cargo commands below.

```bash
cargo xtask dev       # build and run, linker directory on child PATH
cargo xtask dev-ui    # same, dev-ui profile (app crate at opt-level 1)
cargo xtask watch     # rebuild and restart on save through bacon
cargo xtask doctor    # toolchain, bacon and linker probe
cargo xtask set-version <semver>  # single-source the app version
```

The matching aliases in `.cargo/config.toml` are `cargo dev`, `cargo ui`,
`cargo verify` (clippy with warnings denied) and `cargo doctor`. Extra
arguments after `dev` and `dev-ui` are forwarded to the app after `--`.

### Hot reload (rebuild and restart on save)

Two loops are available for the dev build, both native and both built with
the `hot-reload` feature on the `photrez` crate (debug builds only, never
release). Plain `cargo dev` and `cargo ui` also pass that feature.

1. **Rebuild and restart (bacon).** The `bacon.toml` jobs rebuild and
   restart the app on save. State is lost on each restart, but no extra
   tool is needed beyond `bacon`.

```bash
bacon run       # rebuild and restart the app on save (dev profile)
bacon run-ui    # same, dev-ui profile
cargo xtask watch
```

2. **In-process patch (dx).** `gpui-fast` can hot-patch Rust code in the
   running process through Subsecond, so edits to `render` code apply
   without losing app state. The app must be launched through the Dioxus
   CLI dev server:

```bash
cargo xtask hot                          # keep state on render edits
cargo xtask hot --profile dev-ui         # UI iteration profile
dx serve --hot-patch --features hot-reload --package photrez --bin photrez --windows --skip-assets
```

The `cargo xtask hot` form sets the linker directory on the child PATH, so
`windres` finds its `gcc`. The raw `dx` form is shown for reference and
needs that PATH set by hand. Install `dx` once with `cargo install
dioxus-cli`. Edits to layout or data structures (for example a new field on
a struct) still need a full restart, and custom `Element` phases are not
patched yet. When `dx` is not running, the app logs a warning and carries
on normally.

Both `run` jobs set `background = false` and `on_change_strategy =
"kill_then_restart"`, so the app restarts as soon as a save lands. While
`bacon` runs, plain `cargo check`, `cargo test` and `cargo clippy` stay the
source of truth for correctness. Install `bacon` once with `cargo install
--locked bacon`. Plain `bacon` with no argument runs the default `check`
job.

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

### Public surface

Describe changes in product and technical terms. Public documents must not link to files outside this repository or describe a runtime the project no longer ships. The public-surface staleness check is `sh scripts/check-public-terminology.sh --staleness`.

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
