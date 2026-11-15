//! The interactive dev loop: `cargo xtask dev`.
//!
//! The windows-gnu link chain needs the MinGW directory on `PATH`: the linker
//! and `windres` (the gpui-pre build script) load MinGW runtime DLLs from it.
//! Without it the loader aborts with `STATUS_DLL_NOT_FOUND` (0xC0000135),
//! which collect2 reports as `ld returned 53 exit status`.
//!
//! The directory goes on the CHILD process `PATH` only; the caller's
//! environment is untouched. The MSVC toolchain needs no MinGW directory, so
//! nothing is prepended there.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Candidate linker directories, in the order they are tried. The search
/// stays portable instead of trusting one machine's drive letter.
/// `%USERPROFILE%` expands to the real profile directory.
const MINGW_BIN_CANDIDATES: &[&str] = &[
    r"C:\msys64\mingw64\bin",
    r"C:\Program Files\Git\mingw64\bin",
    r"%USERPROFILE%\scoop\apps\msys2\current\mingw64\bin",
];

/// Directory holding `gcc.exe` for the gnu link chain, or `None` when no
/// known MSYS2 installation exists on the system.
pub(crate) fn find_mingw_bin() -> Option<PathBuf> {
    for candidate in MINGW_BIN_CANDIDATES {
        let dir = PathBuf::from(expand_user_profile(candidate));
        if dir.join("gcc.exe").is_file() {
            return Some(dir);
        }
    }
    None
}

/// The active toolchain's host triple, for example
/// `x86_64-pc-windows-gnu` or `x86_64-pc-windows-msvc`.
pub(crate) fn toolchain_host() -> Option<String> {
    let output = Command::new("rustc")
        .arg("--print")
        .arg("host-tuple")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let host = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    (!host.is_empty()).then_some(host)
}

/// True when the active toolchain is the Windows GNU one, which is the only
/// chain that needs the MinGW directory on `PATH`.
fn needs_mingw_on_path() -> bool {
    toolchain_host().is_some_and(|host| host.ends_with("-windows-gnu"))
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

/// Print the MinGW directory the windows-gnu toolchain needs, in the POSIX
/// form a Git Bash shell expects. Other toolchains print nothing.
pub fn print_mingw_dir() -> i32 {
    if !needs_mingw_on_path() {
        return 0;
    }
    match find_mingw_bin() {
        Some(mingw) => {
            println!("{}", posix_path(&mingw));
            0
        }
        None => {
            eprintln!(
                "xtask mingw-dir: no MinGW directory found. Tried: {}. \
                 Install MSYS2, then: pacman -S mingw-w64-x86_64-gcc",
                MINGW_BIN_CANDIDATES.join(", ")
            );
            0
        }
    }
}

/// Convert a Windows path to the POSIX form a Git Bash shell expects, for
/// example `C:\msys64\mingw64\bin` to `/c/msys64/mingw64/bin`.
fn posix_path(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let mut chars = text.chars();
    let drive = chars.next();
    let separator = chars.next();
    match (drive, separator) {
        (Some(drive), Some(':')) if drive.is_ascii_alphabetic() => {
            let drive = drive.to_ascii_lowercase();
            format!("/{drive}{}", chars.as_str())
        }
        _ => text,
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
    let mut cmd = Command::new("cargo");
    cmd.arg("run");
    if let Some(profile) = profile {
        cmd.args(["--profile", profile]);
    }
    cmd.args(["-p", "photrez", "--bin", "photrez"]);
    if !args.is_empty() {
        cmd.arg("--").args(args);
    }

    // Only the windows-gnu chain needs the MinGW directory on PATH. A
    // contributor on the MSVC toolchain must not be blocked by a missing
    // MSYS2 install, so the directory is added only when it is required.
    if needs_mingw_on_path() {
        let Some(mingw) = find_mingw_bin() else {
            eprintln!(
                "xtask dev: the active toolchain is windows-gnu, but no MinGW directory \
                 was found. Tried: {}. Install MSYS2, then: pacman -S mingw-w64-x86_64-gcc",
                MINGW_BIN_CANDIDATES.join(", ")
            );
            return 1;
        };
        println!("xtask dev: linker directory {}", mingw.display());
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
    }

    match cmd.status() {
        Ok(status) => status.code().unwrap_or(1),
        Err(e) => {
            eprintln!("xtask dev: failed to start cargo: {e}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::posix_path;
    use std::path::Path;

    #[test]
    fn a_windows_path_becomes_a_posix_path_entry() {
        assert_eq!(
            posix_path(Path::new(r"C:\msys64\mingw64\bin")),
            "/c/msys64/mingw64/bin"
        );
    }

    #[test]
    fn a_path_without_a_drive_letter_passes_through() {
        assert_eq!(posix_path(Path::new("/usr/local/bin")), "/usr/local/bin");
    }
}
