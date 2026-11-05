//! Single-source the application version in `crates/photrez/Cargo.toml`.
//!
//! `set-version <semver>` validates the semver shape (the rule the retired
//! `scripts/bump-version.ts` enforced), rewrites the canonical manifest, and
//! reads the value back through `cargo metadata` — the manifest string it
//! just wrote is never trusted on its own. A read-back that disagrees restores
//! the manifest bytes it replaced, so a failed run leaves no half-applied
//! version behind.

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
    let original = match std::fs::read(&manifest) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("xtask: cannot read {}: {error}", manifest.display());
            return 1;
        }
    };
    let current = match read_manifest_version(&manifest) {
        Ok(current) => current,
        Err(error) => {
            eprintln!("xtask: {error}");
            return 1;
        }
    };
    if let Some(lower) = release_number_below(&current, new_version) {
        // A tag is made from this number, so a silent downgrade would publish a
        // release older than the one already shipped.
        eprintln!(
            "xtask: refusing to move {current} down to {new_version} (release number {lower} \
             is lower than the current one; a stage or build change on the same release number \
             is allowed)"
        );
        return 1;
    }
    match rewrite_manifest_version(&manifest, new_version) {
        Ok(_) => {}
        Err(error) => {
            eprintln!("xtask: {error}");
            return 1;
        }
    };

    // Read the version back through cargo: the manifest line is only accepted
    // once the metadata agrees.
    let read_back = || read_metadata_version(&root);
    let reported = verify_or_restore(new_version, read_back, || {
        std::fs::write(&manifest, &original)
            .map(|_| format!("version {current} restored in {}", manifest.display()))
            .map_err(|error| {
                format!(
                    "restoring {} failed: {error}; the file still says {new_version}",
                    manifest.display()
                )
            })
    });
    match reported {
        Ok(version) => {
            println!("xtask: photrez version is {version} (read back from cargo metadata)");
            0
        }
        Err(error) => {
            eprintln!("xtask: {error}");
            1
        }
    }
}

/// Ask cargo what the manifest now says. The `Err` text names the cause, so a
/// broken toolchain is not reported as a version mismatch.
fn read_metadata_version(root: &Path) -> Result<String, String> {
    let output = Command::new("cargo")
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .current_dir(root)
        .output()
        .map_err(|error| format!("failed to run cargo metadata: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let metadata = serde_json::from_slice::<serde_json::Value>(&output.stdout)
        .map_err(|error| format!("cargo metadata produced unparsable JSON: {error}"))?;
    let packages = metadata["packages"]
        .as_array()
        .ok_or_else(|| "cargo metadata lists no packages".to_owned())?;
    let package = packages
        .iter()
        .find(|package| package["name"] == "photrez")
        .ok_or_else(|| "the photrez package is missing from cargo metadata".to_owned())?;
    package["version"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| "the photrez package has no version in cargo metadata".to_owned())
}

/// Accept the rewrite only when the read-back agrees, and roll the manifest
/// back otherwise. Without the rollback a refused run would still leave the
/// file carrying the version it just rejected.
fn verify_or_restore(
    expected: &str,
    read_back: impl Fn() -> Result<String, String>,
    rollback: impl FnOnce() -> Result<String, String>,
) -> Result<String, String> {
    match read_back() {
        Ok(reported) if reported == expected => Ok(reported),
        Ok(reported) => match rollback() {
            Ok(acted) => Err(format!(
                "cargo metadata reports {reported}, expected {expected}; {acted}"
            )),
            Err(error) => Err(format!(
                "cargo metadata reports {reported}, expected {expected}; {error}"
            )),
        },
        Err(error) => match rollback() {
            Ok(acted) => Err(format!("{error}; {acted}")),
            Err(rollback_error) => Err(format!("{error}; {rollback_error}")),
        },
    }
}

/// The `[package] version` currently on disk, or an error naming the file.
fn read_manifest_version(manifest: &Path) -> Result<String, String> {
    let text = std::fs::read_to_string(manifest)
        .map_err(|error| format!("cannot read {}: {error}", manifest.display()))?;
    version_line_value(&text)
        .ok_or_else(|| format!("{} carries no [package] version line", manifest.display()))
}

fn version_line_value(text: &str) -> Option<String> {
    let mut in_package = false;
    for line in text.split_inclusive(['\n']) {
        let trimmed = line.trim_end_matches(['\n', '\r']);
        if trimmed == "[package]" {
            in_package = true;
        } else if trimmed.starts_with('[') {
            in_package = false;
        } else if in_package && trimmed.starts_with("version = ") {
            return trimmed
                .split('"')
                .nth(1)
                .map(|value| value.trim_end_matches('"').to_owned());
        }
    }
    None
}

/// The release number (`MAJOR.MINOR.PATCH`) lowered compared with `current`,
/// reported as the new core. A stage or build change on the same core is not a
/// downgrade, because release candidates and betas are cut on a released
/// number.
fn release_number_below(current: &str, candidate: &str) -> Option<String> {
    let before = release_core(current)?;
    let after = release_core(candidate)?;
    (after < before).then(|| format!("{after:?}"))
}

fn release_core(version: &str) -> Option<[u64; 3]> {
    let head = version
        .split(['-', '+'])
        .next()
        .unwrap_or(version)
        .trim()
        .trim_start_matches('v');
    let mut parts = head.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    (parts.next().is_none()).then_some([major, minor, patch])
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

#[cfg(test)]
mod tests {
    use super::{release_number_below, semver_valid, verify_or_restore, version_line_value};

    /// The rule exists because a tag is cut from this number, so lowering it
    /// would publish an older release. A stage or build change on the same
    /// release number stays allowed, which is how a beta is cut after a release.
    #[test]
    fn a_lower_release_number_is_refused_and_a_stage_change_is_not() {
        assert_eq!(
            release_number_below("0.2.0", "0.1.9").as_deref(),
            Some("[0, 1, 9]")
        );
        assert_eq!(release_number_below("0.2.0", "0.2.0-beta.1"), None);
        assert_eq!(release_number_below("0.2.0", "0.2.0+build.7"), None);
        assert_eq!(release_number_below("0.2.0", "0.3.0"), None);
        assert_eq!(
            release_number_below("0.10.0", "0.9.0").as_deref(),
            Some("[0, 9, 0]"),
            "numeric comparison, not lexical: 0.9.0 really is below 0.10.0"
        );
        assert_eq!(
            release_number_below("0.9.0", "0.10.0"),
            None,
            "and 0.10.0 is a move up from 0.9.0"
        );
        assert_eq!(
            release_number_below("nightly", "0.2.0"),
            None,
            "an unparsable current value must not block the run"
        );
    }

    #[test]
    fn the_semver_shape_still_rejects_the_forms_the_retired_script_rejected() {
        for accepted in ["0.2.0", "1.10.3", "0.2.0-beta.1", "0.2.0+build.7"] {
            assert!(semver_valid(accepted), "{accepted} should parse");
        }
        for refused in ["0.2", "0.2.0.1", "v0.2.0", "0.2.0-", "", "0.x.0"] {
            assert!(!semver_valid(refused), "{refused} should be refused");
        }
    }

    /// The trap this covers: a manifest that was already rewritten when cargo
    /// disagrees, so the run reports failure and the tree keeps the refused
    /// version anyway. The rollback has to run, and say so.
    #[test]
    fn a_disagreement_restores_the_bytes_it_replaced() {
        let path = std::env::temp_dir().join(format!(
            "xtask-version-rollback-{}-{}.toml",
            std::process::id(),
            "disagree"
        ));
        std::fs::write(&path, b"[package]\nversion = \"0.2.0\"\n").expect("seed temp manifest");
        let original = std::fs::read(&path).expect("read temp manifest");

        let outcome = verify_or_restore(
            "0.3.0",
            || Ok("0.2.0".to_owned()),
            || {
                std::fs::write(&path, &original)
                    .map(|_| "version 0.2.0 restored".to_owned())
                    .map_err(|error| format!("rollback failed: {error}"))
            },
        );

        let error = match outcome {
            Ok(reported) => {
                std::fs::remove_file(&path).ok();
                panic!("a read-back of 0.2.0 must not be accepted for 0.3.0: {reported}");
            }
            Err(error) => error,
        };
        let restored = std::fs::read_to_string(&path).expect("read after rollback");
        std::fs::remove_file(&path).ok();

        assert!(
            error.contains("cargo metadata reports 0.2.0, expected 0.3.0"),
            "names the disagreement: {error}"
        );
        assert!(error.contains("restored"), "names the rollback: {error}");
        assert_eq!(
            version_line_value(&restored).as_deref(),
            Some("0.2.0"),
            "the file no longer carries the refused version"
        );
    }

    #[test]
    fn an_agreement_is_accepted_without_touching_the_rollback() {
        let mut rolled_back = false;
        let outcome = verify_or_restore(
            "0.3.0",
            || Ok("0.3.0".to_owned()),
            || {
                rolled_back = true;
                Ok("restored".to_owned())
            },
        );
        assert_eq!(outcome.as_deref(), Ok("0.3.0"));
        assert!(
            !rolled_back,
            "an accepted version must not restore the old bytes"
        );
    }
}
