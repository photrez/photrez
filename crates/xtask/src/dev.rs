//! The interactive dev loop: `cargo xtask dev`.
//!
//! `cargo run -p photrez` is the product entry point, but the windows-gnu link
//! chain has one environment requirement a Cargo manifest cannot express: the
//! linker's DLL directory must be on `PATH`.
//!
//! Measured on the development machine (reproduction and fix both rerun with a
//! real link, not a cached no-op): the chain is rustc -> `gcc.exe` (pinned
//! absolute in the user's cargo config) -> collect2 -> `ld.exe`, and gcc
//! selects that ld through its exec prefix at
//! `<mingw64>/x86_64-w64-mingw32/bin/ld.exe`. That ld imports `libintl-8.dll`,
//! `zlib1.dll` and `libzstd.dll`, which live in `<mingw64>\bin` and nowhere
//! adjacent to it; with that directory absent from `PATH` the loader aborts
//! with `STATUS_DLL_NOT_FOUND` (0xC0000135) and collect2 surfaces the low byte
//! as `ld returned 53 exit status`. The gpui-pre manifest build script has the
//! same shape one level up: `windres` (pinned absolute via the `RC`
//! environment) shells out to a bare `gcc` for preprocessing. Prepending the
//! directory fixes both, after which a full build links cleanly on an
//! otherwise stripped `PATH`. The GNU toolchain is sufficient; no MSVC
//! component is involved anywhere in the chain.
//!
//! This command prepends the directory to the CHILD process `PATH` only; the
//! caller's environment is never modified.

use std::ffi::OsString;
use std::path::PathBuf;
use std::process::Command;

/// Candidate linker directories, in the order they are tried. A missing
/// linker is the most common way a fresh clone refuses to build, so the
/// search stays portable instead of trusting one machine's drive letter.
/// `%USERPROFILE%` expands to the real profile directory (the scoop install
/// is user-specific).
const MINGW_BIN_CANDIDATES: &[&str] = &[
    r"C:\msys64\mingw64\bin",
    r"C:\Program Files\Git\mingw64\bin",
    r"%USERPROFILE%\scoop\apps\msys2\current\mingw64\bin",
];

/// Directory holding `gcc.exe` for the gnu link chain, or `None` when no
/// known MSYS2/MSYS2-clone installation exists on this machine.
pub(crate) fn mingw_bin_candidates() -> &'static [&'static str] {
    MINGW_BIN_CANDIDATES
}

pub(crate) fn find_mingw_bin() -> Option<PathBuf> {
    for candidate in MINGW_BIN_CANDIDATES {
        let dir = PathBuf::from(expand_user_profile(candidate));
        if dir.join("gcc.exe").is_file() {
            return Some(dir);
        }
    }
    None
}

/// Replace a `%USERPROFILE%` prefix with the real profile directory. Paths
/// without the marker pass through unchanged.
fn expand_user_profile(path: &str) -> String {
    match path.strip_prefix(r"%USERPROFILE%") {
        Some(rest) => match std::env::var_os("USERPROFILE") {
            Some(profile) => {
                let mut expanded = profile.into_string().unwrap_or_default();
                expanded.push_str(rest);
                expanded
            }
            None => path.to_owned(),
        },
        None => path.to_owned(),
    }
}

/// Build and run the app with the linker directory on the child `PATH`.
/// Extra arguments are forwarded to the application binary after `--`.
pub fn run(args: &[String]) -> i32 {
    run_inner(args, None)
}

/// UI iteration profile: same linker fix as `dev`, with the app crate at
/// opt-level 1 (`dev-ui` in the workspace `Cargo.toml`).
pub fn run_ui(args: &[String]) -> i32 {
    run_inner(args, Some("dev-ui"))
}

fn run_inner(args: &[String], profile: Option<&str>) -> i32 {
    let Some(mingw) = find_mingw_bin() else {
        eprintln!(
            "xtask dev: no gnu linker directory found. Tried: {}. \
             Install MSYS2, then: pacman -S mingw-w64-x86_64-gcc",
            MINGW_BIN_CANDIDATES.join(", ")
        );
        return 1;
    };
    println!("xtask dev: linker directory {}", mingw.display());

    let mut cmd = Command::new("cargo");
    cmd.arg("run");
    if let Some(profile) = profile {
        cmd.args(["--profile", profile]);
    }
    cmd.args([
        "-p",
        "photrez",
        "--features",
        "hot-reload",
        "--bin",
        "photrez",
    ]);
    if !args.is_empty() {
        cmd.arg("--").args(args);
    }
    let child_path = match std::env::var_os("PATH") {
        Some(existing) => {
            let mut prepended = OsString::from(&mingw);
            prepended.push(";");
            prepended.push(existing);
            prepended
        }
        None => OsString::from(&mingw),
    };
    cmd.env("PATH", child_path);

    match cmd.status() {
        Ok(status) => status.code().unwrap_or(1),
        Err(e) => {
            eprintln!("xtask dev: failed to start cargo: {e}");
            1
        }
    }
}
