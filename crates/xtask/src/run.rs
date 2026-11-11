//! Small runners for the workspace tasks that shell out to a program.
//!
//! Only the tasks cargo does not already provide live here: the hot reload
//! loop (`watch`, through bacon) and the environment probe (`doctor`).

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
            // A child that exits non-zero without a message is the silent
            // failure this task must surface, because `dx` prints its own
            // diagnostics to the terminal and nothing else.
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

/// `cargo xtask hot`: run `dx serve --hot-patch` with the linker directory on
/// the child PATH. `dx` needs it for `windres`, and it cannot find the
/// workspace binary on its own, so this task names the package and bin.
pub(crate) fn hot(extra: &[String]) -> i32 {
    let Some(mingw) = crate::dev::find_mingw_bin() else {
        eprintln!(
            "xtask hot: no gnu linker directory found. Tried: {}. \
             Install MSYS2, then: pacman -S mingw-w64-x86_64-gcc",
            crate::dev::mingw_bin_candidates().join(", ")
        );
        return 1;
    };
    if Command::new("dx").arg("--version").output().is_err() {
        eprintln!(
            "xtask hot: `dx` is not on PATH. Install it with `cargo install dioxus-cli`, \
             then run `cargo xtask doctor`."
        );
        return 1;
    }
    let mut args = vec![
        "serve".to_owned(),
        "--hot-patch".to_owned(),
        "--features".to_owned(),
        "hot-reload".to_owned(),
        "--package".to_owned(),
        "photrez".to_owned(),
        "--bin".to_owned(),
        "photrez".to_owned(),
        "--windows".to_owned(),
        "--skip-assets".to_owned(),
    ];
    args.extend(extra.iter().cloned());

    let mut cmd = Command::new("dx");
    cmd.args(&args);
    cmd.current_dir(workspace_root());
    let child_path = match std::env::var_os("PATH") {
        Some(existing) => {
            let mut prepended = std::ffi::OsString::from(&mingw);
            prepended.push(";");
            prepended.push(existing);
            prepended
        }
        None => std::ffi::OsString::from(&mingw),
    };
    cmd.env("PATH", child_path);
    // `dx` is an interactive TUI: it reads keys from the terminal and draws
    // its status there, so the child keeps the inherited stdin/stdout.
    match cmd.status() {
        Ok(exit) if exit.success() => 0,
        Ok(exit) => {
            match exit.code() {
                Some(code) => eprintln!("xtask hot: dx exited with {code}"),
                None => eprintln!("xtask hot: dx terminated by signal"),
            }
            exit.code().unwrap_or(1)
        }
        Err(error) => {
            eprintln!("xtask hot: failed to start dx: {error}");
            1
        }
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
    match probe("dx", &["--version"]) {
        Some(version) => println!("dx: ok ({version})"),
        None => println!(
            "dx: not on PATH (needed for `dx serve --hot-patch`; install with `cargo install dioxus-cli`)"
        ),
    }
    match crate::dev::find_mingw_bin() {
        Some(dir) => println!("gnu linker dir: ok ({})", dir.display()),
        None => {
            println!("gnu linker dir: not found (fine off Windows GNU; `cargo xtask dev` needs it)")
        }
    }
    if ok {
        0
    } else {
        1
    }
}
