use super::*;

pub(super) struct OracleSnapshot {
    pub(super) commit: String,
    pub(super) production_files: Vec<String>,
    pub(super) test_files: Vec<String>,
    pub(super) backend_files: Vec<String>,
}

pub(super) fn git_snapshot(root: &Path) -> Result<OracleSnapshot, String> {
    let head = git_output(root, &["rev-parse", "--verify", "HEAD^{commit}"])?;
    let head = String::from_utf8(head)
        .map_err(|e| format!("git returned a non-UTF-8 HEAD: {e}"))?
        .trim()
        .to_string();
    if !matches!(head.len(), 40 | 64) || !head.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(format!("git returned an invalid full HEAD: {head}"));
    }

    let listing = git_output(
        root,
        &[
            "ls-tree",
            "-r",
            "-z",
            "--name-only",
            head.as_str(),
            "--",
            ".",
        ],
    )?;
    snapshot_from_paths(head, parse_ls_tree_paths(&listing)?)
}

pub(super) fn parse_ls_tree_paths(listing: &[u8]) -> Result<Vec<String>, String> {
    if !listing.is_empty() && !listing.ends_with(&[0]) {
        return Err("git ls-tree output is missing its NUL terminator".to_string());
    }
    let mut paths = Vec::new();
    for bytes in listing
        .split(|byte| *byte == 0)
        .filter(|bytes| !bytes.is_empty())
    {
        let path = std::str::from_utf8(bytes)
            .map_err(|e| format!("git returned a non-UTF-8 source path: {e}"))?;
        paths.push(path.to_string());
    }
    paths.sort();
    duplicate_paths(&paths, "git ls-tree HEAD")?;
    Ok(paths)
}

pub(super) fn snapshot_from_paths(
    commit: String,
    mut paths: Vec<String>,
) -> Result<OracleSnapshot, String> {
    paths.sort();
    duplicate_paths(&paths, "Git tree snapshot")?;

    let production_files = paths
        .iter()
        .filter_map(|path| path.strip_prefix("src/"))
        .filter(|path| is_production_path(path))
        .map(str::to_string)
        .collect();
    let test_files: Vec<String> = paths
        .iter()
        .filter(|path| is_test_reference_path(path))
        .cloned()
        .collect();
    if let Some(path) = test_files
        .iter()
        .find(|path| path.contains('\r') || path.contains('\n'))
    {
        return Err(format!(
            "test path cannot be represented in a line manifest: {path:?}"
        ));
    }
    let backend_files = paths
        .iter()
        .filter(|path| {
            path.starts_with("src-tauri/") && Path::new(path).extension().is_some_and(|e| e == "rs")
        })
        .cloned()
        .collect();

    Ok(OracleSnapshot {
        commit,
        production_files,
        test_files,
        backend_files,
    })
}

fn git_output(root: &Path, args: &[&str]) -> Result<Vec<u8>, String> {
    let output = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .map_err(|e| format!("cannot run git in {}: {e}", root.display()))?;
    if !output.status.success() {
        return Err(format!(
            "git {} failed in {}: {}",
            args.join(" "),
            root.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(output.stdout)
}

pub(super) fn compare_production_sets(
    tracked: &[String],
    scanned: &[String],
    tree: &str,
) -> Vec<String> {
    let mut errors = Vec::new();
    if let Err(error) = duplicate_paths(tracked, "Git tree snapshot") {
        errors.push(error);
    }
    if let Err(error) = duplicate_paths(scanned, "filesystem scan") {
        errors.push(error);
    }
    let tracked_set: BTreeSet<&str> = tracked.iter().map(String::as_str).collect();
    let scanned_set: BTreeSet<&str> = scanned.iter().map(String::as_str).collect();
    for rel in tracked_set.difference(&scanned_set) {
        errors.push(format!(
            "src/{rel}: tracked {tree} production file missing from filesystem scan"
        ));
    }
    for rel in scanned_set.difference(&tracked_set) {
        errors.push(format!(
            "src/{rel}: untracked/generated production file is not tracked in {tree}"
        ));
    }
    errors
}

pub(super) fn head_scan_differences(
    tracked: &[String],
    scanned: &[String],
) -> (Vec<String>, Vec<String>) {
    let tracked_set: BTreeSet<&str> = tracked.iter().map(String::as_str).collect();
    let scanned_set: BTreeSet<&str> = scanned.iter().map(String::as_str).collect();
    let missing = tracked_set
        .difference(&scanned_set)
        .map(|path| path.to_string())
        .collect();
    let extra = scanned_set
        .difference(&tracked_set)
        .map(|path| path.to_string())
        .collect();
    (missing, extra)
}
