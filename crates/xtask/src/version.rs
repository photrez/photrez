//! Single-source the application version in `crates/photrez/Cargo.toml`.
//!
//! `set-version <semver>` validates the semver shape (the rule the retired
//! `scripts/bump-version.ts` enforced), rewrites the canonical manifest, and
//! reads the value back through `cargo metadata` — the manifest string it
//! just wrote is never trusted on its own.

use std::path::{Path, PathBuf};
use std::process::Command;

const MANIFEST_REL: &str = "crates/photrez/Cargo.toml";

pub(crate) fn cmd_set_version(args: &[String]) -> i32 {
    let Some(new_version) = args.first() else {
        eprintln!("xtask set-version <semver>. Example: xtask set-version 0.2.0");
        return 2;
    };
    if !semver_valid(new_version) {
        eprintln!("xtask: invalid version format `{new_version}` (expected MAJOR.MINOR.PATCH[-stage.N][+build])");
        return 1;
    }

    let root = match workspace_root() {
        Some(root) => root,
        None => {
            eprintln!("xtask: could not locate the workspace root (no Cargo.toml upward)");
            return 1;
        }
    };
    let manifest = root.join(MANIFEST_REL);
    match rewrite_manifest_version(&manifest, new_version) {
        Ok(_) => {}
        Err(error) => {
            eprintln!("xtask: {error}");
            return 1;
        }
    };

    // Read the version back through cargo: the manifest line is only accepted
    // once the metadata agrees.
    let output = Command::new("cargo")
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .current_dir(&root)
        .output();
    let Ok(output) = output else {
        eprintln!("xtask: failed to run cargo metadata");
        return 1;
    };
    if !output.status.success() {
        eprintln!(
            "xtask: cargo metadata failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        return 1;
    }
    let metadata = match serde_json::from_slice::<serde_json::Value>(&output.stdout) {
        Ok(value) => value,
        Err(error) => {
            eprintln!("xtask: cargo metadata produced unparsable JSON: {error}");
            return 1;
        }
    };
    let reported = metadata["packages"]
        .as_array()
        .and_then(|packages| {
            packages
                .iter()
                .find(|package| package["name"] == "photrez")
                .and_then(|package| package["version"].as_str())
        })
        .map(str::to_owned);
    match reported.as_deref() {
        Some(version) if version == new_version => {
            println!("xtask: photrez version is {version} (read back from cargo metadata)");
            0
        }
        Some(version) => {
            eprintln!("xtask: cargo metadata reports {version}, expected {new_version}");
            1
        }
        None => {
            eprintln!("xtask: the photrez package is missing from cargo metadata");
            1
        }
    }
}

/// Rewrite the `[package] version` line, preserving every line's original
/// ending (the manifest on disk is CRLF; a naive `join("\n")` churns every
/// line). Returns Ok(false) when the manifest already carries the requested
/// version (idempotent re-runs change nothing).
fn rewrite_manifest_version(manifest: &Path, new_version: &str) -> Result<bool, String> {
    let text = std::fs::read_to_string(manifest)
        .map_err(|error| format!("cannot read {}: {error}", manifest.display()))?;
    let mut in_package = false;
    let mut replaced = false;
    let mut out = String::with_capacity(text.len() + 16);
    for line in text.split_inclusive(['\n']) {
        let trimmed = line.trim_end_matches(['\n', '\r']);
        if trimmed == "[package]" {
            in_package = true;
        } else if trimmed.starts_with('[') {
            in_package = false;
        }
        if in_package && !replaced && trimmed.starts_with("version = ") {
            let ending = line.trim_start_matches(trimmed);
            out.push_str(&format!("version = \"{new_version}\"{ending}"));
            replaced = true;
        } else {
            out.push_str(line);
        }
    }
    if !replaced {
        return Err(format!(
            "{} carries no [package] version line",
            manifest.display()
        ));
    }
    std::fs::write(manifest, out)
        .map_err(|error| format!("cannot write {}: {error}", manifest.display()))?;
    Ok(true)
}

/// The semver shape the retired `scripts/bump-version.ts` enforced
/// (`MAJOR.MINOR.PATCH[-stage.N][+build]`); hand-rolled to keep the regex
/// crate out of xtask's dependency list.
fn semver_valid(version: &str) -> bool {
    let (core, build) = match version.split_once('+') {
        Some((core, build)) => (core, Some(build)),
        None => (version, None),
    };
    if let Some(build) = build {
        if build.is_empty() || !build.chars().all(|c| c.is_ascii_alphanumeric() || c == '.') {
            return false;
        }
    }
    let (release, pre) = match core.split_once('-') {
        Some((release, pre)) => (release, Some(pre)),
        None => (core, None),
    };
    let mut numbers = release.split('.');
    let (major, minor, patch) = (numbers.next(), numbers.next(), numbers.next());
    let valid_number = |part: Option<&str>| {
        part.is_some_and(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()))
    };
    if !(valid_number(major)
        && valid_number(minor)
        && valid_number(patch)
        && numbers.next().is_none())
    {
        return false;
    }
    if let Some(pre) = pre {
        if pre.is_empty()
            || !pre.split('.').all(|part| {
                !part.is_empty()
                    && part
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
            })
        {
            return false;
        }
    }
    true
}

/// Walk upward from the executable's directory to the workspace root (the
/// first ancestor holding a Cargo.toml with a `[workspace]` table).
fn workspace_root() -> Option<PathBuf> {
    let mut dir = std::env::current_exe()
        .ok()?
        .parent()
        .map(Path::to_path_buf)?;
    loop {
        let manifest = dir.join("Cargo.toml");
        if manifest.is_file()
            && std::fs::read_to_string(&manifest)
                .map(|text| text.contains("[workspace]"))
                .unwrap_or(false)
        {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
}
