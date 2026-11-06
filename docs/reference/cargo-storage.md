# Cargo storage guidance

Checked 2026-10-22 against the official Cargo Book; restated 2026-10-28 for the
single-root-workspace layout, and 2026-11-06 against the current `target/` tree, the
`dev` profile and `cargo tree -d`. This is advisory; it does not change workspace
profiles or machine-wide settings.

## Keep build artifacts off the system drive

Cargo's generated artifacts live in the workspace target directory by default. `CARGO_TARGET_DIR` or Cargo's `build.target-dir` setting can redirect them. For this repository there is one root workspace and one lockfile; the checked-in `.cargo/config.toml` pins `[build] target-dir = "target"`, so every crate builds into the single `target/` directory at the repository root. Do not commit an absolute drive letter in shared project config.

The normal workflow needs no `CARGO_TARGET_DIR` override; run Cargo from the repository root and the shared target directory receives all artifacts. This keeps the build cache with the checkout and avoids accidental temporary target trees. If a local override is necessary, reuse one stable directory and keep its absolute drive path out of shared config.

```powershell
# From the repository root; uses the root target directory on D:.
cargo test --locked --workspace

# The interactive dev loop; same target directory.
cargo dev
```

The commands above use the project-local relative targets and need no environment override. If a local `CARGO_TARGET_DIR` override is necessary, use one stable path per workspace and keep the absolute drive path out of shared config; the environment setting overrides the project config.

## Cargo home is separate from build output

`CARGO_HOME` stores downloaded registry crates and Git checkouts. On Windows its default is `%USERPROFILE%\.cargo`; `cargo clean` does not remove these caches. If that cache should live on another volume, copy the existing Cargo home first, then set `CARGO_HOME` for future Cargo processes and verify builds/credentials before removing the old copy. Cargo Home may contain configuration and credentials; never delete it blindly. Clearing cached sources is recoverable by re-downloading, but requires network access.

## Clean deliberately; keep useful dev defaults

`cargo clean` with no scope removes the entire selected target directory, not Cargo Home. Preview first with `cargo clean --dry-run`; when ready, pass the intended `--target-dir` explicitly. This avoids clearing the wrong workspace's artifacts.

The default `dev` profile keeps full debug information and incremental compilation for faster rebuilds and debugging. Incremental data is stored under the build directory. Keep those defaults for normal work; for a one-off build where rebuild speed is unimportant, `CARGO_INCREMENTAL=0` can disable incremental data for that invocation, trading slower rebuilds for less intermediate output. Avoid globally stripping debug information without measuring the debugging impact.

If builds across many workspaces need shared dependency artifacts, Cargo's build-cache documentation points to the third-party `sccache` wrapper. It is optional and not part of this repository's setup.

## Official sources

- [Cargo environment variables](https://doc.rust-lang.org/cargo/reference/environment-variables.html): `CARGO_HOME`, `CARGO_TARGET_DIR`, and `CARGO_INCREMENTAL`.
- [Cargo Home](https://doc.rust-lang.org/cargo/guide/cargo-home.html): cache contents, relocation, and recovery.
- [Build cache](https://doc.rust-lang.org/cargo/reference/build-cache.html): target/build output layout and incremental artifacts.
- [`cargo clean`](https://doc.rust-lang.org/cargo/commands/cargo-clean.html): deletion scope, `--target-dir`, and `--dry-run`.
- [Profiles](https://doc.rust-lang.org/cargo/reference/profiles.html): development defaults and incremental/debug tradeoffs.

## 2026-10-22: Shrinking build artifacts without losing the debugger

Advisory only when written on 2026-10-22: no artifact was deleted and no config file was
changed. The project later applied one recommendation from below: `fc0c0dbd` (2026-11-01)
sets `debug = "line-tables-only"` in the `dev` profile, in place of the full-debug-info
default. The other experiments remain unapplied. Each is a one-command, per-invocation
change, and closing the shell undoes it.
The sizes below describe the pre-consolidation per-crate target directory; after the
workspace consolidation the root `target/` is the only build tree, and the old
`crates/photrez/target/` numbers are historical.

### What actually occupies the space in this repo

Measured read-only on 2026-10-22 with `Get-ChildItem -Recurse` (no Cargo build ran):

| Path | Size |
| --- | --- |
| `crates/photrez/target/` | 15.44 GiB (the root `target/` was already cleaned and holds 0.05 GiB) |
| `crates/photrez/target/debug/deps/` | 13,887 MiB |
| `crates/photrez/target/debug/incremental/` | 936 MiB |
| `crates/photrez/target/debug/build/` | 324 MiB |

Inside `debug/deps/`, by extension: `.exe` 9,358 MiB (16 files), `.rlib` 2,876 MiB (538),
`.rmeta` 1,138 MiB (915), `.dll` 372 MiB (45), `.o` 132 MiB (767), `.d` 11 MiB (976).
There are no `.pdb` files: this tree is built with the `x86_64-pc-windows-gnu` toolchain
(`rustc 1.95.0`), so debug info is embedded in the executables themselves rather than
split into a separate symbol file. The single largest artifact on that date was
`crates/photrez/target/debug/photrez.exe` at 631 MiB.

Measured 2026-11-06 in the root `target/`, after the consolidation and the `debug`
change above: `target/debug/photrez.exe` is 290 MiB, `target/dev-ui/photrez.exe` is
296 MiB, and the two profiling binaries `target/debug/measure-input.exe` and
`target/debug/measure-frames.exe` are 305 MiB and 300 MiB. The largest single files are
the test and example binaries under `target/debug/deps/`, about 324 MiB each.

Two consequences worth keeping in view:

- The 9.3 GiB of `.exe` files under `deps/` are test and example binaries built by
  `cargo test --no-run`, each carrying full (`debug = 2`) debug info. Reducing the debug
  level shrinks the executables themselves, not just a side-car file.
- `incremental/` is only 936 MiB, about 6% of the tree. Disabling incremental compilation
  is a poor trade here: it gives up fast rebuilds for a small fraction of the space.

Both bullets are 2026-10-22 readings of the per-crate tree, and two later changes move
them. The `debug = 2` note applies to binaries built before 2026-11-01; the `dev` profile
now builds with line tables. In the consolidated root `target/`, `incremental/` is a much
larger share than 6%: a `du -sm` read on 2026-11-06, taken while another build ran,
measured 8.6 GiB under `target/debug/incremental` against 28.8 GiB under
`target/debug/deps`, and `du -sh target` reported 43 GB for the whole tree. Re-measure
before you act on either ratio.

The `debug` directory holds both the `dev` and `test` profiles, and `release` holds both
`release` and `bench`; the directory names are historical and do not match the profile
names ([Cargo Book, Build cache](https://doc.rust-lang.org/cargo/reference/build-cache.html)).

### Order of attack: reversible experiments first

Each item below is a single command, changes nothing on disk except the artifacts that
command itself produces, and leaves no configuration behind.

**1. Measure before touching anything (official).**

```powershell
cargo clean --dry-run
```

`--dry-run` "displays a summary of what would be deleted without deleting anything"; add
`--verbose` to list the actual files
([cargo-clean](https://doc.rust-lang.org/cargo/commands/cargo-clean.html)).

Scope matters more than the directory you run it from. Checked 2026-11-06 with
`cargo metadata --no-deps` inside `crates/photrez`: `workspace_root` is the repository
root and `target_directory` is the root `target/`. One command from a member crate
therefore reports, and without `--dry-run` deletes, every crate's artifacts. Use `-p`
or `--profile` to narrow the scope, not `cd`.

**2. Reduce debug info for one invocation, no file changes (verified).**

Verified on stable `cargo 1.95.0` in `crates/photrez`. PowerShell strips double quotes
when handing arguments to native executables, so `--config KEY="value"` is parsed by Cargo
as a bare word and fails. Two forms work (checked with `cargo metadata --no-deps --offline`
on stable and `cargo <toolchain> -Z unstable-options config get` on nightly. Nightly was
installed only for this config check and then removed):

```powershell
# Simple per-crate debug level via environment variable (no quoting pain):
$env:CARGO_PROFILE_DEV_DEBUG = 'line-tables-only'
cargo <command>

# Dependency override via a throwaway TOML file (env vars cannot set the package form):
#   [profile.dev]
#   debug = "line-tables-only"
#   [profile.dev.package."*"]
#   debug = false
$profileOverride = Join-Path $env:TEMP 'cargo-profile-override.toml'
cargo --config $profileOverride build
```

Notes:

- `CARGO_PROFILE_DEV_DEBUG` is a documented environment variable
  ([Environment Variables](https://doc.rust-lang.org/cargo/reference/environment-variables.html)).
- The `profile.<name>.package.<name>` form (the `"*"` dependency override) is documented as
  **not** settable by environment variable
  ([Configuration](https://doc.rust-lang.org/cargo/reference/config.html)). Confirmed:
  `CARGO_PROFILE_DEV_PACKAGE_*_DEBUG` is ignored, so `--config` with a TOML file is the
  supported per-invocation route for dependency overrides.
- `--config` accepts a path to an extra configuration file in addition to `KEY=VALUE`, and
  "takes precedence over environment variables, which take precedence over configuration
  files" ([Configuration](https://doc.rust-lang.org/cargo/reference/config.html)).
- `line-tables-only` generates "the minimal amount of debug info for backtraces with
  filename/line number info, but not anything else, i.e. no variable or function parameter
  info": backtraces keep file and line, the debugger loses variable inspection
  ([Profiles](https://doc.rust-lang.org/cargo/reference/profiles.html)).

The Cargo Book itself recommends this exact shape as a configuration change
([Optimizing Build Performance](https://doc.rust-lang.org/cargo/guide/build-performance.html)):

```toml
[profile.dev]
debug = "line-tables-only"

[profile.dev.package."*"]
debug = false

[profile.debugging]
inherits = "dev"
debug = true
```

with the stated trade-offs: faster codegen, faster links, smaller target directory, and
"requires a full rebuild to have a high-quality debugger experience". Applying it as a
checked-in profile is a persistent change and should be a separate, deliberate decision,
not a side effect of a disk-pressure cleanup. The project took that decision on
2026-11-01 and set `debug = "line-tables-only"` under `[profile.dev]`. It did not add the
`[profile.debugging]` profile above.

Previewing before any cleanup, on the pre-consolidation tree: `cargo clean --dry-run` in
`crates/photrez` reported `Summary 10568 files, 15.4GiB total` / `warning: no files
deleted due to --dry-run`
([cargo-clean](https://doc.rust-lang.org/cargo/commands/cargo-clean.html)).

**3. Targeted clean, not a full wipe (official).**

```powershell
cargo clean -p <package-spec>     # only that package's artifacts
cargo clean --profile <name>      # one profile directory
cargo clean --doc                 # only target/doc
```

All three are official `cargo clean` options. `-p` cleans "only the specified packages";
`--profile` removes "all artifacts in the directory with the given profile name". Because
`dev` and `test` share the `debug/` directory, verify which files a `--profile` run would
remove with `--dry-run` first. With one workspace there is one build tree, so scoping
comes from these options; `CARGO_TARGET_DIR` and `--target-dir` move the whole workspace
to another tree instead of narrowing the delete.

**4. Disable incremental compilation for one invocation (official).**

```powershell
$env:CARGO_INCREMENTAL = '0'; cargo build
```

`CARGO_INCREMENTAL=0` "will force disabling it" for the current compilation
([Environment Variables](https://doc.rust-lang.org/cargo/reference/environment-variables.html)).
Per the Profiles reference, incremental data "is stored in the target directory". The
2026-10-22 reading above measured that directory at 936 MiB, about 6% of that tree, and
the consolidated root tree holds far more incremental data now: see the note there.
Measure the current size before you decide, and weigh it against slower rebuilds. Use
this only when a one-off artifact dump matters more than iteration speed. It prevents new
incremental data but does not delete existing cache files; preview a targeted clean
before removing them.

### What does not help, and why

- **Automatic cache cleaning does not touch build artifacts.** `cache.auto-clean-frequency`
  (default `"1 day"`) controls deletion of unused files in the *global* cache in Cargo
  Home. The Cargo Book states plainly: "this tracking is currently only implemented for the
  global cache in Cargo's home directory... Support for tracking build artifacts is not yet
  implemented"
  ([Configuration](https://doc.rust-lang.org/cargo/reference/config.html), tracking issue
  [cargo#13136](https://github.com/rust-lang/cargo/issues/13136)). Turning it to
  `"always"` frees space in `%USERPROFILE%\.cargo`, never in `target/`.
- **`cargo clean` does not touch Cargo Home.** "Once a crate is cached it is not removed by
  the clean command"
  ([Environment Variables](https://doc.rust-lang.org/cargo/reference/environment-variables.html)).
- **sccache is a time optimisation, not a space optimisation.** The Cargo Book points at
  it for sharing built dependencies across workspaces
  ([Build cache](https://doc.rust-lang.org/cargo/reference/build-cache.html)), but the
  community reports it *adds* disk use: it copies compiled rlibs into each project's
  target directory and keeps its own copy in the sccache cache, so it trades disk for
  compile time ([Rust Users Forum](https://users.rust-lang.org/t/is-there-a-way-to-make-the-size-of-whole-project-files-smaller/100758)).
  Not a candidate here.
- **The nightly metadata change is not available on this toolchain.** `-Zembed-metadata=no`
  removes the duplicated crate metadata stored in both `.rmeta` and `.rlib`, reported at
  -9.5% to -36% depending on profile ([Kobzol's write-up](https://kobzol.github.io/rust/rustc/2025/06/02/reduce-cargo-target-dir-size-with-z-no-embed-metadata.html),
  [Inside Rust, 2026-08-18](https://blog.rust-lang.org/inside-rust/2026/08/18/reducing-target-dir-size-on-nightly/)).
  It is unstable and nightly-only, and the local toolchain is stable `rustc 1.95.0`
  (`x86_64-pc-windows-gnu`), so it is not an option today.
- **Reducing `debug` invalidates the existing cache.** Old artifacts keep their full debug
  info until they are deleted, so a profile change is only visible after a clean rebuild
  ([Rust Users Forum](https://users.rust-lang.org/t/is-there-a-way-to-make-the-size-of-whole-project-files-smaller/100758)).

### Community evidence (anecdotal, not guarantees)

| Claim | Source |
| --- | --- |
| `debug = 1` cut one project's release debuginfo from 480 MB to 105 MB | [Rust Users Forum, 2021](https://users.rust-lang.org/t/keeping-debug-info-but-smaller/55009) |
| `debug = "line-tables-only"` plus a `debugging` opt-in profile gave a ~20% smaller `target/debug` on one project | [Stack Overflow](https://stackoverflow.com/questions/79952745/why-rust-is-taking-a-ton-of-my-disk-space-and-how-to-make-it-use-less) |
| A `target/debug` breakdown showing `incremental/` at 16 GB and `deps/` at 21 GB: final artifacts were the smallest part | [Stack Overflow](https://stackoverflow.com/questions/79952745/why-rust-is-taking-a-ton-of-my-disk-space-and-how-to-make-it-use-less) |
| Debug info is only in final artifacts; intermediate metadata dominates `deps/` | [same thread](https://stackoverflow.com/questions/79952745/why-rust-is-taking-a-ton-of-my-disk-space-and-how-to-make-it-use-less) |
| A single binary measured 231 MiB → 136 MiB (`debug = 1`) → 49 MiB (`debug = 0`) → 34 MiB (`strip = true`) | [r/rust](https://www.reddit.com/r/rust/comments/1g3q6rg/why_are_rust_binaries_so_large/) |
| Metadata is stored twice (`.rmeta` and inside `.rlib`); removing one copy saves 20–35% | [r/rust](https://www.reddit.com/r/rust/comments/1bms2gj/media_ran_out_of_disk_space_on_my_1tb_machine_a/) |
| Switching debug level to line tables is the usual community fix | [r/rust](https://www.reddit.com/r/rust/comments/1bms2gj/media_ran_out_of_disk_space_on_my_1tb_machine_a/), [r/rust](https://www.reddit.com/r/rust/comments/1eyfl57/rust_target_folder_is_big_in_size/) |
| `cargo tree -d` to find duplicate dependency versions | [Rust Users Forum](https://users.rust-lang.org/t/is-there-a-way-to-make-the-size-of-whole-project-files-smaller/100758) |

Measured 2026-11-06 with `cargo tree -d --offline` at the repository root: 49 crates
resolve to more than one version, 94 version entries in total, for example `bitflags`
1.3.2/2.13.0, `itertools` 0.11.0/0.13.0/0.14.0, `png` 0.17.16/0.18.1, `fontdb`
0.20.0/0.23.0, and `windows-sys` 0.52.0/0.61.2. The 2026-10-22 reading in `crates/photrez`
named `base64` 0.22.1/0.23.1 among them; that pair is no longer reported. Most come from
the upstream `gpui-kit` / `gpui-pre` trees and are not removable from this repository
without upstream changes; each duplicate adds a second compiled copy to `deps/`.

Third-party cleaners exist but add a dependency and are not needed here:
[`cargo-sweep`](https://github.com/holmgr/cargo-sweep) (its README now warns the project is
unmaintained and points to `cargo-clean-all`),
[`cargo-cache`](https://github.com/matthiaskrgr/cargo-cache) (manages Cargo Home, not the
target directory).

### Upstream direction

The Rust project has an accepted 2026 goal, "Target directory size reduction"
([goals.rust-lang.org](https://goals.rust-lang.org/2026/target-dir-size.html)), targeting
30–40% smaller fresh builds in a first phase. It lists two changes directly relevant here:
reducing debuginfo to `line-tables-only` in the `dev` profile
([cargo#17518](https://github.com/rust-lang/cargo/pull/17518)), and the
`-Zembed-metadata=no` experiment described above. The `dev` profile defaults in the
current Cargo Book are still `debug = true` and `incremental = true`, so the shipped
defaults remain unchanged until those land.

### Sources

Official:

- [Cargo Book, Profiles](https://doc.rust-lang.org/cargo/reference/profiles.html): `debug` levels, `incremental`, `strip`, `split-debuginfo`, default profiles, profile overrides.
- [Cargo Book, Environment Variables](https://doc.rust-lang.org/cargo/reference/environment-variables.html): `CARGO_INCREMENTAL`, `CARGO_TARGET_DIR`, `CARGO_HOME`, `CARGO_PROFILE_*`.
- [Cargo Book, Configuration](https://doc.rust-lang.org/cargo/reference/config.html): `build.target-dir`, `build.incremental`, `cache.auto-clean-frequency`, `--config` precedence, `profile.<name>` keys.
- [Cargo Book, Build cache](https://doc.rust-lang.org/cargo/reference/build-cache.html): target/build directory layout, `debug/` vs `release/`, `incremental/`, sccache.
- [Cargo Book, cargo clean](https://doc.rust-lang.org/cargo/commands/cargo-clean.html): `--dry-run`, `-p`, `--profile`, `--doc`, `--target-dir`.
- [Cargo Book, Optimizing Build Performance](https://doc.rust-lang.org/cargo/guide/build-performance.html): the `line-tables-only` recommendation and its trade-offs.
- [Cargo Book, Cargo Home](https://doc.rust-lang.org/cargo/guide/cargo-home.html): cache contents and relocation.

Community and upstream:

- [Rust Users Forum, "Is there a way to make the size of whole project files smaller?"](https://users.rust-lang.org/t/is-there-a-way-to-make-the-size-of-whole-project-files-smaller/100758)
- [Rust Users Forum, "Keeping debug info, but smaller?"](https://users.rust-lang.org/t/keeping-debug-info-but-smaller/55009)
- [Stack Overflow, "Why Rust is taking a ton of my disk space and how to make it use less"](https://stackoverflow.com/questions/79952745/why-rust-is-taking-a-ton-of-my-disk-space-and-how-to-make-it-use-less)
- [r/rust, "Ran out of disk space on my 1TB machine"](https://www.reddit.com/r/rust/comments/1bms2gj/media_ran_out_of_disk_space_on_my_1tb_machine_a/)
- [r/rust, "Rust target folder is big in size"](https://www.reddit.com/r/rust/comments/1eyfl57/rust_target_folder_is_big_in_size/)
- [r/rust, "Why are rust binaries so large?"](https://www.reddit.com/r/rust/comments/1g3q6rg/why_are_rust_binaries_so_large/)
- [Kobzol, "Reducing Cargo target directory size with -Zno-embed-metadata"](https://kobzol.github.io/rust/rustc/2025/06/02/reduce-cargo-target-dir-size-with-z-no-embed-metadata.html)
- [Inside Rust, "Reducing target directory size on nightly" (2026-08-18)](https://blog.rust-lang.org/inside-rust/2026/08/18/reducing-target-dir-size-on-nightly/)
- [Rust Project Goals, "Target directory size reduction"](https://goals.rust-lang.org/2026/target-dir-size.html)
- [cargo#13136, track build artifacts for automatic cache cleaning](https://github.com/rust-lang/cargo/issues/13136)
- [cargo#17518, reduce debuginfo to `line-tables-only` in the `dev` profile](https://github.com/rust-lang/cargo/pull/17518)
