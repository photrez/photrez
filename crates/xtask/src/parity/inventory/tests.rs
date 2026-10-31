use super::{
    build_inventory, compare_production_sets, head_scan_differences, parse_ls_tree_paths,
    reference_rows, snapshot_from_paths, validate_text, OracleSnapshot,
};

const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";
const HEAD_COMMIT: &str = "fedcba9876543210fedcba9876543210fedcba98";
const MANIFEST_NAME: &str = "inventory.tests.txt";

fn snapshot(paths: &[&str]) -> OracleSnapshot {
    snapshot_at(COMMIT, paths)
}

fn head_snapshot(paths: &[&str]) -> OracleSnapshot {
    snapshot_at(HEAD_COMMIT, paths)
}

fn snapshot_at(commit: &str, paths: &[&str]) -> OracleSnapshot {
    let listing = format!("{}\0", paths.join("\0"));
    let paths = parse_ls_tree_paths(listing.as_bytes()).expect("fixture path listing");
    snapshot_from_paths(commit.to_string(), paths).expect("fixture tree snapshot")
}

fn with_commit(snapshot: &OracleSnapshot, commit: &str) -> OracleSnapshot {
    OracleSnapshot {
        commit: commit.to_string(),
        production_files: snapshot.production_files.clone(),
        test_files: snapshot.test_files.clone(),
        backend_files: snapshot.backend_files.clone(),
    }
}

fn validate_error(
    csv: &str,
    manifest: &str,
    oracle: &OracleSnapshot,
    head: &OracleSnapshot,
) -> Vec<String> {
    validate_text(
        csv,
        manifest,
        oracle,
        head,
        &oracle.production_files,
        &head.production_files,
        MANIFEST_NAME,
    )
    .expect_err("fixture must be rejected")
    .0
}

fn remove_row(csv: &str, id: &str) -> String {
    let prefix = format!("{id},");
    format!(
        "{}\n",
        csv.lines()
            .filter(|line| !line.starts_with(&prefix))
            .collect::<Vec<_>>()
            .join("\n")
    )
}

#[test]
fn full_tree_snapshot_preserves_test_census_filters() {
    let snapshot = snapshot(&[
        "src/App.tsx",
        "src/__tests__/alpha.test.ts",
        "src/components/editor/AdjustmentsPanel.tsx",
        "src/components/editor/AdjustmentsPanel.test.tsx",
        "e2e/editor.spec.ts",
        "e2e/pixels.test.js",
        "e2e/node_modules/dependency.test.ts",
        "e2e/pkg/generated.test.tsx",
        "e2e/case-sensitive.test.JS",
        "src-tauri/build.rs",
        "src-tauri/src/main.rs",
        "src-tauri/src/windows/dialog.rs",
    ]);

    assert_eq!(
        snapshot.production_files,
        vec![
            "App.tsx".to_string(),
            "components/editor/AdjustmentsPanel.tsx".to_string()
        ]
    );
    assert_eq!(
        snapshot.test_files,
        vec![
            "e2e/editor.spec.ts".to_string(),
            "e2e/pixels.test.js".to_string(),
            "src/__tests__/alpha.test.ts".to_string(),
            "src/components/editor/AdjustmentsPanel.test.tsx".to_string(),
        ]
    );
    assert_eq!(
        snapshot.backend_files,
        vec![
            "src-tauri/build.rs".to_string(),
            "src-tauri/src/main.rs".to_string(),
            "src-tauri/src/windows/dialog.rs".to_string(),
        ]
    );
}

#[test]
fn production_set_comparison_names_missing_extra_and_duplicate_paths() {
    let snapshot = snapshot(&["src/components/editor/types.ts"]);
    let tracked = snapshot.production_files;
    assert!(compare_production_sets(&tracked, &tracked, "Oracle").is_empty());

    let missing = compare_production_sets(&tracked, &[], "Oracle");
    assert!(missing.iter().any(|error| {
        error == "src/components/editor/types.ts: tracked Oracle production file missing from filesystem scan"
    }));
    let extra = compare_production_sets(
        &tracked,
        &[
            "components/editor/types.ts".to_string(),
            "generated/new.ts".to_string(),
        ],
        "Oracle",
    );
    assert!(extra.iter().any(|error| {
        error == "src/generated/new.ts: untracked/generated production file is not tracked in Oracle"
    }));
    let duplicate = compare_production_sets(
        &tracked,
        &[
            "components/editor/types.ts".to_string(),
            "components/editor/types.ts".to_string(),
        ],
        "Oracle",
    );
    assert!(duplicate
        .iter()
        .any(|error| error == "components/editor/types.ts: duplicate path in filesystem scan"));
}

#[test]
fn out_of_target_uses_tracked_head_and_scan_drift_fails_closed() {
    let oracle = snapshot(&["src/components/editor/AdjustmentsPanel.tsx"]);
    let head = head_snapshot(&[
        "src/components/editor/AdjustmentsPanel.tsx",
        "src/components/editor/octoberOnly.ts",
    ]);
    let (csv, manifest, out_of_target, _) = build_inventory(&oracle, &head, MANIFEST_NAME);
    assert_eq!(out_of_target, 1);
    assert!(csv.contains("O001,behavior,apps/desktop/src/components/editor/octoberOnly.ts"));
    assert!(validate_text(
        &csv,
        &manifest,
        &oracle,
        &head,
        &oracle.production_files,
        &head.production_files,
        MANIFEST_NAME,
    )
    .is_ok());

    let missing_scan = vec!["components/editor/AdjustmentsPanel.tsx".to_string()];
    let (missing, extra) = head_scan_differences(&head.production_files, &missing_scan);
    assert_eq!(missing, ["components/editor/octoberOnly.ts"]);
    assert!(extra.is_empty());
    let errors = validate_text(
        &csv,
        &manifest,
        &oracle,
        &head,
        &oracle.production_files,
        &missing_scan,
        MANIFEST_NAME,
    )
    .expect_err("missing tracked HEAD path fails validation")
    .0;
    assert!(errors.iter().any(|error| {
        error == "src/components/editor/octoberOnly.ts: tracked HEAD production file missing from filesystem scan"
    }));

    let with_generated = vec![
        "components/editor/AdjustmentsPanel.tsx".to_string(),
        "components/editor/octoberOnly.ts".to_string(),
        "generated/created.tsx".to_string(),
    ];
    let (missing, extra) = head_scan_differences(&head.production_files, &with_generated);
    assert!(missing.is_empty());
    assert_eq!(extra, ["generated/created.tsx"]);
    let (tracked_csv, _, _, _) = build_inventory(&oracle, &head, MANIFEST_NAME);
    assert!(!tracked_csv.contains("generated/created.tsx"));
    assert!(validate_text(
        &csv,
        &manifest,
        &oracle,
        &head,
        &oracle.production_files,
        &with_generated,
        MANIFEST_NAME,
    )
    .is_ok());

    assert_eq!(
        head_scan_differences(&head.production_files, &head.production_files),
        (Vec::<String>::new(), Vec::<String>::new())
    );
    assert!(validate_text(
        &csv,
        &manifest,
        &oracle,
        &head,
        &oracle.production_files,
        &head.production_files,
        MANIFEST_NAME,
    )
    .is_ok());
}

#[test]
fn generator_emits_backend_paths_with_unknown_targets() {
    let mut paths = vec!["src/components/editor/AdjustmentsPanel.tsx".to_string()];
    paths.extend((0..19).map(|index| format!("src-tauri/src/backend_{index:02}.rs")));
    paths.extend([
        "e2e/editor.spec.ts".to_string(),
        "src/__tests__/canvas.test.tsx".to_string(),
    ]);
    let listing = format!("{}\0", paths.join("\0"));
    let snapshot = snapshot_from_paths(
        COMMIT.to_string(),
        parse_ls_tree_paths(listing.as_bytes()).expect("fixture path listing"),
    )
    .expect("fixture snapshot");
    let head_snapshot = with_commit(&snapshot, HEAD_COMMIT);

    let references = reference_rows(&snapshot, MANIFEST_NAME);
    assert_eq!(references.len(), 31);
    assert_eq!(references[11][0], "R012");
    assert_eq!(references[11][2], MANIFEST_NAME);
    assert!(references[11][7].contains("exact tracked manifest (2 paths)"));
    assert!(references[11][7].contains(COMMIT));
    for (index, backend) in snapshot.backend_files.iter().enumerate() {
        let row = &references[12 + index];
        assert_eq!(row[0], format!("R{:03}", 13 + index));
        assert_eq!(row[2], format!("apps/desktop/{backend}"));
        assert_eq!(row[3], "reference-platform");
        assert_eq!(row[4], "UNKNOWN");
        assert_eq!(row[5], "REFERENCE");
        assert_eq!(row[7], "No per-file native destination is evidenced.");
    }

    let (csv, manifest, out_of_target, reference_count) =
        build_inventory(&snapshot, &head_snapshot, MANIFEST_NAME);
    assert_eq!(out_of_target, 0);
    assert_eq!(reference_count, 31);
    assert!(csv.contains("reference rows: 31"));
    assert!(csv.contains(&format!("# oracle: {COMMIT}")));
    assert!(csv.contains(&format!("# head: {HEAD_COMMIT}")));
    assert_eq!(
        manifest,
        "apps/desktop/e2e/editor.spec.ts\napps/desktop/src/__tests__/canvas.test.tsx\n"
    );
}

#[test]
fn test_manifest_missing_extra_duplicate_fail_and_restore_passes() {
    let snapshot = snapshot(&[
        "src/components/editor/AdjustmentsPanel.tsx",
        "e2e/editor.spec.ts",
        "src/__tests__/canvas.test.tsx",
    ]);
    let head = head_snapshot(&[
        "src/components/editor/AdjustmentsPanel.tsx",
        "e2e/editor.spec.ts",
        "src/__tests__/canvas.test.tsx",
    ]);
    let (csv, manifest, _, _) = build_inventory(&snapshot, &head, MANIFEST_NAME);
    assert!(validate_text(
        &csv,
        &manifest,
        &snapshot,
        &head,
        &snapshot.production_files,
        &head.production_files,
        MANIFEST_NAME
    )
    .is_ok());

    let missing = manifest.replace("apps/desktop/e2e/editor.spec.ts\n", "");
    assert!(validate_error(&csv, &missing, &snapshot, &head)
        .iter()
        .any(|error| error == "apps/desktop/e2e/editor.spec.ts: test manifest path is missing"));

    let extra = format!("{manifest}apps/desktop/src/fake.test.ts\n");
    assert!(validate_error(&csv, &extra, &snapshot, &head)
        .iter()
        .any(|error| error == "apps/desktop/src/fake.test.ts: extra test manifest path"));

    let duplicate = format!("{manifest}apps/desktop/src/__tests__/canvas.test.tsx\n");
    assert!(validate_error(&csv, &duplicate, &snapshot, &head)
        .iter()
        .any(|error| error
            == "apps/desktop/src/__tests__/canvas.test.tsx: duplicate test manifest path"));

    assert!(validate_text(
        &csv,
        &manifest,
        &snapshot,
        &head,
        &snapshot.production_files,
        &head.production_files,
        MANIFEST_NAME
    )
    .is_ok());
}

#[test]
fn backend_rows_missing_or_extra_fail_then_restore_passes() {
    let snapshot = snapshot(&[
        "src/components/editor/AdjustmentsPanel.tsx",
        "src-tauri/src/main.rs",
        "src-tauri/src/menu.rs",
        "src/__tests__/canvas.test.tsx",
    ]);
    let head = head_snapshot(&[
        "src/components/editor/AdjustmentsPanel.tsx",
        "src-tauri/src/main.rs",
        "src-tauri/src/menu.rs",
        "src/__tests__/canvas.test.tsx",
    ]);
    let (csv, manifest, _, _) = build_inventory(&snapshot, &head, MANIFEST_NAME);
    assert!(csv.contains("R013,reference,apps/desktop/src-tauri/src/main.rs,reference-platform,UNKNOWN,REFERENCE,,No per-file native destination is evidenced."));
    assert!(validate_text(
        &csv,
        &manifest,
        &snapshot,
        &head,
        &snapshot.production_files,
        &head.production_files,
        MANIFEST_NAME
    )
    .is_ok());

    let missing = remove_row(&csv, "R013");
    assert!(validate_error(&missing, &manifest, &snapshot, &head)
        .iter()
        .any(
            |error| error == "apps/desktop/src-tauri/src/main.rs: backend reference row is missing"
        ));

    let extra = format!(
        "{csv}R999,reference,apps/desktop/src-tauri/src/generated.rs,reference-platform,UNKNOWN,REFERENCE,,No per-file native destination is evidenced.\n"
    );
    assert!(validate_error(&extra, &manifest, &snapshot, &head)
        .iter()
        .any(
            |error| error == "apps/desktop/src-tauri/src/generated.rs: extra backend reference row"
        ));

    assert!(validate_text(
        &csv,
        &manifest,
        &snapshot,
        &head,
        &snapshot.production_files,
        &head.production_files,
        MANIFEST_NAME
    )
    .is_ok());
}

#[test]
fn csv_parser_handles_quoted_commas() {
    let text = "# comment\nid,path\n1,\"a.rs, b.rs\"\n2,\"say \"\"hi\"\"\"\n";
    let (header, rows) = super::csv::parse(text).expect("parses");
    assert_eq!(header, vec!["id", "path"]);
    assert_eq!(rows[0], vec!["1", "a.rs, b.rs"]);
    assert_eq!(rows[1], vec!["2", "say \"hi\""]);
}
