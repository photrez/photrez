use super::*;

fn validate_rows(
    records: &[HashMap<String, String>],
    oracle_snapshot: &OracleSnapshot,
    head_snapshot: &OracleSnapshot,
    manifest_name: &str,
) -> Vec<String> {
    let mut errors: Vec<String> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut seen_ids = BTreeSet::new();
    let oracle_paths: BTreeSet<String> = oracle_snapshot
        .production_files
        .iter()
        .map(|rel| format!("apps/desktop/src/{rel}"))
        .collect();
    let head_paths: BTreeSet<String> = head_snapshot
        .production_files
        .iter()
        .map(|rel| format!("apps/desktop/src/{rel}"))
        .collect();
    let mut production_rows = BTreeSet::new();
    let mut out_of_target_rows = BTreeSet::new();
    for row in records {
        let path = row.get("path").cloned().unwrap_or_default();
        let status = row.get("status").cloned().unwrap_or_default();
        let id = row.get("id").map(String::as_str).unwrap_or_default();
        if !seen_ids.insert(id.to_string()) {
            errors.push(format!("{id}: duplicate row ID"));
        }
        if !STATUSES.contains(&status.as_str()) {
            errors.push(format!(
                "{path}: status `{status}` is not in the vocabulary"
            ));
        }
        if status == "NA" && row.get("notes").map(|n| n.is_empty()).unwrap_or(true) {
            errors.push(format!("{path}: NA without a reason"));
        }
        if status == "VERIFIED" && row.get("evidence").map(|n| n.is_empty()).unwrap_or(true) {
            errors.push(format!("{path}: VERIFIED without evidence"));
        }
        if !seen.insert(path.clone()) {
            errors.push(format!("{path}: duplicate row"));
        }
        if id.starts_with('F') {
            production_rows.insert(path.clone());
            if !oracle_paths.contains(&path) {
                errors.push(format!(
                    "{path}: generated/untracked Oracle production row is not in pinned HEAD"
                ));
            }
        } else if id.starts_with('O') {
            out_of_target_rows.insert(path);
        }
    }

    for rel in &oracle_snapshot.production_files {
        let path = format!("apps/desktop/src/{rel}");
        if !production_rows.contains(&path) {
            errors.push(format!("{path}: oracle production file is unmapped"));
        }
    }
    let expected_out_of_target: BTreeSet<String> =
        head_paths.difference(&oracle_paths).cloned().collect();
    for path in expected_out_of_target.difference(&out_of_target_rows) {
        errors.push(format!(
            "{path}: tracked HEAD-only production file is unmapped"
        ));
    }
    for path in out_of_target_rows.difference(&expected_out_of_target) {
        errors.push(format!(
            "{path}: extra out-of-target row is not in the tracked Oracle/HEAD difference"
        ));
    }

    let test_index = REFERENCE_FILES
        .iter()
        .position(|(path, _, _, _, _)| path.contains("*.test."));
    if let Some(test_index) = test_index {
        let (_, family, target, status, notes) = REFERENCE_FILES[test_index];
        let test_id = test_index + 1;
        let id = format!("R{test_id:03}");
        let matches: Vec<_> = records
            .iter()
            .filter(|row| row.get("id").map(String::as_str) == Some(id.as_str()))
            .collect();
        if matches.len() != 1 {
            errors.push(format!("{id}: expected one test-manifest summary row"));
        }
        let expected_notes = format!(
            "{notes}; exact tracked manifest ({} paths); oracle {}",
            oracle_snapshot.test_files.len(),
            oracle_snapshot.commit
        );
        if let Some(row) = matches.first() {
            for (field, expected) in [
                ("path", manifest_name),
                ("family", family),
                ("native_target", target),
                ("status", status),
                ("notes", &expected_notes),
            ] {
                if row.get(field).map(String::as_str) != Some(expected) {
                    errors.push(format!("{id}: {field} does not match the test manifest"));
                }
            }
        }
    }

    let backend_index = REFERENCE_FILES
        .iter()
        .position(|(path, _, _, _, _)| path.contains("src-tauri/**/*.rs"));
    if let Some(backend_index) = backend_index {
        let (_, family, _, status, _) = REFERENCE_FILES[backend_index];
        let backend_start = backend_index + 1;
        let expected_paths: BTreeSet<String> = oracle_snapshot
            .backend_files
            .iter()
            .map(|path| format!("apps/desktop/{path}"))
            .collect();
        let actual_rows: Vec<_> = records
            .iter()
            .filter(|row| {
                row.get("path")
                    .is_some_and(|path| path.starts_with("apps/desktop/src-tauri/"))
            })
            .collect();
        let actual_paths: BTreeSet<String> = actual_rows
            .iter()
            .filter_map(|row| row.get("path").cloned())
            .collect();
        for path in expected_paths.difference(&actual_paths) {
            errors.push(format!("{path}: backend reference row is missing"));
        }
        for path in actual_paths.difference(&expected_paths) {
            errors.push(format!("{path}: extra backend reference row"));
        }
        for (index, expected_path) in oracle_snapshot.backend_files.iter().enumerate() {
            let id = format!("R{:03}", backend_start + index);
            let expected_path = format!("apps/desktop/{expected_path}");
            let Some(row) = records
                .iter()
                .find(|row| row.get("id").map(String::as_str) == Some(id.as_str()))
            else {
                errors.push(format!("{expected_path}: backend row {id} is missing"));
                continue;
            };
            if row.get("path").map(String::as_str) != Some(expected_path.as_str()) {
                errors.push(format!("{id}: expected backend path {expected_path}"));
            }
            for (field, expected) in [
                ("family", family),
                ("native_target", "UNKNOWN"),
                ("status", status),
                ("notes", UNKNOWN_BACKEND_NOTE),
            ] {
                if row.get(field).map(String::as_str) != Some(expected) {
                    errors.push(format!(
                        "{expected_path}: backend disposition field {field} differs"
                    ));
                }
            }
        }
    }
    errors
}

pub(super) fn validate_with_snapshot(
    csv_path: &Path,
    manifest_path: &Path,
    oracle_snapshot: &OracleSnapshot,
    head_snapshot: &OracleSnapshot,
    oracle_scan: &[String],
    head_scan: &[String],
) -> i32 {
    let text = match std::fs::read_to_string(csv_path) {
        Ok(text) => text,
        Err(e) => {
            eprintln!(
                "inventory validate: cannot read {}: {e}",
                csv_path.display()
            );
            return 1;
        }
    };
    let manifest = match std::fs::read_to_string(manifest_path) {
        Ok(manifest) => manifest,
        Err(error) => {
            eprintln!(
                "inventory validate: cannot read test manifest {}: {error}",
                manifest_path.display()
            );
            return 1;
        }
    };
    let Some(manifest_name) = manifest_path.file_name().and_then(|name| name.to_str()) else {
        eprintln!("inventory validate: test manifest filename is not UTF-8");
        return 1;
    };
    match validate_text(
        &text,
        &manifest,
        oracle_snapshot,
        head_snapshot,
        oracle_scan,
        head_scan,
        manifest_name,
    ) {
        Ok(rows) => {
            println!(
                "inventory validate: PASS ({rows} rows, {} Oracle and {} HEAD production files)",
                oracle_snapshot.production_files.len(),
                head_snapshot.production_files.len()
            );
            0
        }
        Err((errors, rows)) => {
            for error in errors.iter().take(20) {
                eprintln!("inventory validate: FAIL {error}");
            }
            eprintln!(
                "inventory validate: {} problem(s) across {} rows",
                errors.len(),
                rows
            );
            1
        }
    }
}

pub(super) fn validate_text(
    text: &str,
    manifest: &str,
    oracle_snapshot: &OracleSnapshot,
    head_snapshot: &OracleSnapshot,
    oracle_scan: &[String],
    head_scan: &[String],
    manifest_name: &str,
) -> Result<usize, (Vec<String>, usize)> {
    let (header, rows) = csv::parse(text).map_err(|error| (vec![error], 0))?;
    let records = csv::records_as_maps(&header, &rows).map_err(|error| (vec![error], 0))?;
    let mut errors = Vec::new();
    if !text
        .lines()
        .any(|line| line == format!("# oracle: {}", oracle_snapshot.commit))
    {
        errors.push(format!(
            "oracle header does not identify pinned HEAD {}",
            oracle_snapshot.commit
        ));
    }
    if !text
        .lines()
        .any(|line| line == format!("# head: {}", head_snapshot.commit))
    {
        errors.push(format!(
            "HEAD header does not identify tracked HEAD {}",
            head_snapshot.commit
        ));
    }
    errors.extend(compare_production_sets(
        &oracle_snapshot.production_files,
        oracle_scan,
        "Oracle",
    ));
    let (missing_head_files, _) = head_scan_differences(&head_snapshot.production_files, head_scan);
    errors.extend(missing_head_files.into_iter().map(|path| {
        format!("src/{path}: tracked HEAD production file missing from filesystem scan")
    }));
    errors.extend(validate_manifest(manifest, oracle_snapshot));
    errors.extend(validate_rows(
        &records,
        oracle_snapshot,
        head_snapshot,
        manifest_name,
    ));
    let out_of_target = records
        .iter()
        .filter(|row| row.get("id").is_some_and(|id| id.starts_with('O')))
        .count();
    let reference_count = records
        .iter()
        .filter(|row| row.get("id").is_some_and(|id| id.starts_with('R')))
        .count();
    let expected_summary = format!(
        "# oracle production files: {} | HEAD production files: {} | out-of-target rows: {out_of_target} | reference rows: {reference_count}",
        oracle_snapshot.production_files.len(),
        head_snapshot.production_files.len()
    );
    if !text.lines().any(|line| line == expected_summary) {
        errors.push("inventory header counts do not match emitted rows".to_string());
    }
    if errors.is_empty() {
        Ok(records.len())
    } else {
        Err((errors, records.len()))
    }
}

fn validate_manifest(manifest: &str, snapshot: &OracleSnapshot) -> Vec<String> {
    let actual: Vec<String> = manifest.lines().map(str::to_string).collect();
    let expected: Vec<String> = snapshot
        .test_files
        .iter()
        .map(|path| format!("apps/desktop/{path}"))
        .collect();
    let mut errors = Vec::new();
    let mut seen = BTreeSet::new();
    for path in &actual {
        if !seen.insert(path) {
            errors.push(format!("{path}: duplicate test manifest path"));
        }
    }
    let expected_set: BTreeSet<&str> = expected.iter().map(String::as_str).collect();
    let actual_set: BTreeSet<&str> = actual.iter().map(String::as_str).collect();
    for path in expected_set.difference(&actual_set) {
        errors.push(format!("{path}: test manifest path is missing"));
    }
    for path in actual_set.difference(&expected_set) {
        errors.push(format!("{path}: extra test manifest path"));
    }
    if actual != expected {
        errors.push("test manifest paths are not in expected sorted order".to_string());
    }
    errors
}
