//! Small runners for the workspace tasks that shell out to a program.
//!
//! Only the tasks cargo does not already provide live here: the watch loop
//! (`watch`, through bacon) and the environment probe (`doctor`).

use std::path::PathBuf;
use std::process::Command;

/// Workspace root: two levels above this crate's manifest directory.
pub(crate) fn workspace_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .and_then(|crates| crates.parent())
        .unwrap_or(&manifest_dir)
        .to_path_buf()
}

fn run(status: std::io::Result<std::process::ExitStatus>, label: &str) -> i32 {
    match status {
        Ok(exit) if exit.success() => 0,
        Ok(exit) => {
            // Name the exit code: a child that stops without a message is
            // otherwise a silent failure.
            match exit.code() {
                Some(code) => eprintln!("xtask {label}: exited with {code}"),
                None => eprintln!("xtask {label}: terminated by signal"),
            }
            exit.code().unwrap_or(1)
        }
        Err(error) => {
            eprintln!("xtask {label}: failed to start: {error}");
            1
        }
    }
}

/// Run any program, e.g. `bacon`. `args` goes verbatim after the program name.
pub(crate) fn program_cmd(program: &str, args: &[String]) -> i32 {
    let mut cmd = Command::new(program);
    cmd.args(args);
    cmd.current_dir(workspace_root());
    run(cmd.status(), program)
}

/// `cargo xtask watch`: rebuild and restart the app on save through bacon.
/// The `run` job in `bacon.toml` holds the restart strategy; a job name may
/// be passed as the only argument (`cargo xtask watch run-ui`).
pub(crate) fn watch(extra: &[String]) -> i32 {
    if extra.is_empty() {
        program_cmd("bacon", &["run".to_owned()])
    } else {
        program_cmd("bacon", extra)
    }
}

fn probe(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines()
        .next()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
}

/// The Windows SDK's HLSL compiler, which the release build of
/// `gpui-fast-windows` needs. The lookup follows the crate's own order: the
/// `GPUI_FXC_PATH` variable, then `PATH`, then the SDK directory.
fn windows_sdk_fxc() -> Option<PathBuf> {
    if !cfg!(windows) {
        return None;
    }
    if let Some(path) = std::env::var_os("GPUI_FXC_PATH") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }
    if let Ok(output) = Command::new("where.exe").arg("fxc.exe").output()
        && output.status.success()
    {
        let first = String::from_utf8_lossy(&output.stdout);
        if let Some(line) = first.lines().next() {
            let path = PathBuf::from(line.trim());
            if path.is_file() {
                return Some(path);
            }
        }
    }
    for root in [
        r"C:\Program Files (x86)\Windows Kits\10\bin",
        r"C:\Program Files\Windows Kits\10\bin",
    ] {
        let Ok(versions) = std::fs::read_dir(root) else {
            continue;
        };
        for version in versions.flatten() {
            let candidate = version.path().join("x64").join("fxc.exe");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// `cargo xtask doctor`: print what the dev loop needs, no writes.
/// Returns non-zero only when `rustc` or `cargo` cannot run.
pub(crate) fn doctor() -> i32 {
    println!("workspace: {}", workspace_root().display());
    let mut ok = true;
    for (name, program, args) in [
        ("rustc", "rustc", vec!["--version"]),
        ("cargo", "cargo", vec!["--version"]),
    ] {
        match probe(program, &args) {
            Some(version) => println!("{name}: ok ({version})"),
            None => {
                println!("{name}: MISSING (install the toolchain in rust-toolchain.toml)");
                ok = false;
            }
        }
    }
    match probe("bacon", &["--version"]) {
        Some(version) => println!("bacon: ok ({version})"),
        None => println!(
            "bacon: not on PATH (needed for `cargo xtask watch`; install with `cargo install --locked bacon`)"
        ),
    }
    match crate::dev::toolchain_host().as_deref() {
        Some(host) => println!("toolchain host: {host}"),
        None => println!("toolchain host: unknown (rustc could not report it)"),
    }
    match crate::dev::find_mingw_bin() {
        Some(dir) => println!("gnu linker dir: ok ({})", dir.display()),
        None => {
            println!("gnu linker dir: not found (needed only on the windows-gnu toolchain)")
        }
    }
    if cfg!(windows) {
        match windows_sdk_fxc() {
            Some(path) => println!("windows sdk fxc: ok ({})", path.display()),
            None => println!(
                "windows sdk fxc: not found (a Windows release build needs it, a debug build \
                 does not; install the Windows SDK or set GPUI_FXC_PATH)"
            ),
        }
    }
    if ok { 0 } else { 1 }
}
