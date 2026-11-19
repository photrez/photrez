//! The workspace gate as one command. Mirrors the Rust steps of CI so a local
//! run and a runner evaluate the same commands, without a CI-specific tool.

use std::process::Command;

use super::run::workspace_root;

fn cargo(args: &[&str], extra_env: Option<(&str, &str)>, label: &str) -> i32 {
    let program = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    let mut cmd = Command::new(program);
    cmd.args(args).current_dir(workspace_root());
    if let Some((key, value)) = extra_env {
        cmd.env(key, value);
    }
    match cmd.status() {
        Ok(exit) if exit.success() => 0,
        Ok(exit) => {
            eprintln!("xtask ci: {label} exited with {}", exit.code().unwrap_or(1));
            exit.code().unwrap_or(1)
        }
        Err(error) => {
            eprintln!("xtask ci: {label} failed to start: {error}");
            1
        }
    }
}

/// `cargo xtask ci`: format, lint, build-independent tests, workspace tests, docs.
pub(crate) fn run_ci() -> i32 {
    for (label, args, env) in [
        ("format", vec!["fmt", "--check"], None),
        (
            "clippy",
            vec![
                "clippy",
                "--workspace",
                "--all-targets",
                "--",
                "-D",
                "warnings",
            ],
            None,
        ),
        (
            "clippy (measurement feature)",
            vec![
                "clippy",
                "-p",
                "photrez",
                "--all-targets",
                "--features",
                "input-measurement",
                "--",
                "-D",
                "warnings",
            ],
            None,
        ),
        ("core tests", vec!["test", "-p", "photrez-core"], None),
        ("workspace tests", vec!["test", "--workspace"], None),
        (
            "docs",
            vec!["doc", "--no-deps", "--workspace"],
            Some(("RUSTDOCFLAGS", "-D warnings")),
        ),
    ] {
        eprintln!("xtask ci: {label}");
        let refs: Vec<&str> = args.to_vec();
        let code = cargo(&refs, env, label);
        if code != 0 {
            return code;
        }
    }
    0
}
