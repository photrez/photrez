# Contributing to Photrez

Thanks for helping improve Photrez! We welcome contributions that help make Photrez a faster, more reliable desktop image editor for creators.

## Ground Rules

- **Scope:** Align proposals with the current product and keep changes focused.
- **Layout Stability:** Preserve existing desktop UI structures unless a change explicitly requires UI redesign.
- **Architecture:** Respect runtime ownership boundaries described in `docs/ARCHITECTURE.md`.
- **Focused Changes:** Prefer small, well-tested pull requests over large, unannounced refactors.
- **Dependencies:** Avoid adding external dependencies unless strictly necessary.

## Development Setup

### Toolchain

Install the Rust toolchain named by [`rust-toolchain.toml`](rust-toolchain.toml).
It pins the compiler version and the components, so a fresh clone and a CI run
evaluate the same `rustc`, `clippy` and `rustfmt`.

Run `cargo xtask doctor` once on a fresh clone. It prints the `rustc` and
`cargo` versions, the active toolchain host, the linker directory, the
Windows SDK compiler and whether `bacon` is on `PATH`. It writes nothing.

### System packages

GPUI needs the X11, Wayland, fontconfig and Vulkan client libraries. On
Debian or Ubuntu, install them before the first build:

```bash
sudo apt-get update
sudo apt-get install -y libxkbcommon-dev libxkbcommon-x11-dev \
  libwayland-dev libx11-dev libfontconfig1-dev libfreetype6-dev \
  libvulkan-dev
```

On macOS, install the Xcode command line tools with `xcode-select --install`.
On Windows, see the toolchain table below.

### Windows toolchains

Windows has two Rust toolchains. Use MSVC: `rustup` installs it by default and
CI builds and tests it. The GNU toolchain builds too, but CI does not cover it.

| Toolchain | What you need | Status |
| --- | --- | --- |
| `x86_64-pc-windows-msvc` | Visual Studio Build Tools with the "Desktop development with C++" workload | Recommended. `rustup` installs this host by default on Windows, and CI builds and tests this toolchain. `cargo xtask dev` needs no extra `PATH` entry. |
| `x86_64-pc-windows-gnu` | MSYS2 with `mingw-w64-x86_64-gcc` | Supported for local work. CI does not run this toolchain. `cargo xtask dev` adds the MinGW directory to the child `PATH`. |

Two Windows-specific notes:

- **Keep one MinGW on `PATH`, MSYS2's first.** Git for Windows ships its own
  `mingw64\bin` with the same MinGW runtime DLL names as MSYS2. When Git's
  directory comes first, `cc1.exe` loads the wrong DLLs and dies silently, so
  `windres` reports only "preprocessing failed". Put `C:\msys64\mingw64\bin`
  before `...\Git\mingw64\bin`, or remove Git's copy. This applies to the GNU
  toolchain only.
- **Release builds need the Windows SDK.** `gpui-fast-windows` compiles HLSL
  shaders with `fxc.exe` from the Windows SDK. A debug build does not need it.
  The SDK ships with Visual Studio and also installs standalone
  (`winget install Microsoft.WindowsSDK.10.0.26100`).

### Build and run

Standard Cargo is the default build tool, and the dev loop does not depend on
a particular Cargo binary. Run these from the repository root:

```bash
cargo build
cargo dev             # Run the app with the dev profile
cargo ui              # UI iteration profile
cargo xtask dev       # Same as cargo dev, with the MinGW directory on PATH on GNU hosts
cargo xtask watch     # Rebuild and restart the app on save
bacon                 # Save-triggered cargo check -p photrez
```

On the GNU toolchain, a build also needs the MinGW directory on `PATH`.
`cargo xtask dev` prepends it to the child process only. Our internal
machines run a separate Cargo build with shared dependencies for faster
rebuilds and disk self-cleaning; contributors do not need it.

### Rebuild and restart on save

A compiled Rust app cannot reload modules in place, so the fast loop is
rebuild and restart on save. The `bacon.toml` file defines it:

```bash
bacon run       # rebuild and restart the app on save (dev profile)
bacon run-ui    # same, dev-ui profile
cargo xtask watch
```

Both `run` jobs set `background = false` and `on_change_strategy =
"kill_then_restart"`, so the app restarts as soon as a save lands. State is
lost on each restart. While `bacon` runs, plain `cargo check`, `cargo test`
and `cargo clippy` stay the source of truth for correctness. Install `bacon`
once with `cargo install --locked bacon`. Plain `bacon` with no argument runs
the default `check` job.

### Workspace tasks

`cargo xtask` is the task crate for this workspace. It needs only `cargo`,
`rustc`, and the repository itself, so it works on a fresh clone with no
extra install. It holds only the tasks plain cargo cannot do on its own:
running the app with the MinGW directory on `PATH` on GNU hosts, watching
rebuilds through `bacon`, probing the environment, bumping the version, and
running the workspace gate as one command.

```bash
cargo xtask dev       # build and run, MinGW directory on child PATH on GNU hosts
cargo xtask dev-ui    # same, dev-ui profile (app crate at opt-level 1)
cargo xtask watch     # rebuild and restart on save through bacon
cargo xtask doctor    # toolchain, bacon and linker probe
cargo xtask ci        # format, clippy, core and workspace tests, docs
cargo xtask set-version <semver>  # single-source the app version
```

The matching aliases in `.cargo/config.toml` are `cargo dev`, `cargo ui`,
`cargo verify` (clippy with warnings denied) and `cargo doctor`. Extra
arguments after `dev` and `dev-ui` are forwarded to the app after `--`.

### Checks

Before opening a pull request, run the workspace gates:

```bash
cargo xtask ci        # the same gates CI runs, in one command
cargo build -p photrez
```

The last command builds the shipped executable: the workspace tests only build
test harnesses, so the app binary is linked here. CI runs the same gates and
also installs `cargo-nextest` 0.9.152 for the workspace test job. Setting
`.config/nextest.toml` pins that version locally; install it with:

```bash
cargo install cargo-nextest --locked --version 0.9.152
cargo nextest run --workspace --profile ci
cargo test --doc --workspace   # nextest does not run doctests
```

Nextest runs each test in its own process and caps app tests at four workers,
which keeps GPUI windows from exhausting the runner. The separate
`packaging-lint` workflow checks shell syntax, manifests, and release notice
files without a Rust build. The pre-push hook runs the gates this machine can
run, including the documentation build and the repository scripts.

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

Describe changes in product and technical terms. Public documents must not link to files outside this repository or describe a runtime the project no longer ships.

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

A `v`-prefixed version tag runs `.github/workflows/release.yml`. The workflow is generated by
`dist generate` from `dist-workspace.toml`; do not hand-edit it. `dist` builds an archive for each configured
target, adds `LICENSE`, `NOTICE` and `THIRD_PARTY.md`, and generates a `.sha256` for every archive plus a
combined `SHA256SUMS`. The release title and notes come from `CHANGELOG.md`. Photrez is not code-signed, so
those checksums are the integrity check a user can perform.

Check the plan before publishing:

```
dist plan --tag v<version>
```

The plan lists the archives and checksum files the tag will produce. `dist build --tag v<version>` runs the
same packaging locally.
