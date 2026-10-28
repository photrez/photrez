use std::fs;
use std::path::{Path, PathBuf};

use super::{production_files, run, InventoryOpts};

/// A scratch tree with one file per classification branch: an exact override,
/// a prefix-rule file, an unclassified file, a test file that must be excluded,
/// and a type-only file.
struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new(tag: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("xtask-inventory-{}-{}", tag, std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("src/components/editor")).expect("fixture dirs");
        Self { root }
    }

    fn write(&self, rel: &str, contents: &str) {
        let path = self.root.join("src").join(rel);
        fs::create_dir_all(path.parent().expect("parent")).expect("create dirs");
        fs::write(path, contents).expect("write fixture file");
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn out_path(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "xtask-inventory-out-{tag}-{}.csv",
        std::process::id()
    ))
}

#[test]
fn known_tree_classifies_every_branch() {
    let fx = Fixture::new("branches");
    fx.write("components/editor/AdjustmentsPanel.tsx", "export {}");
    fx.write("components/editor/canvas/HoverHighlight.tsx", "export {}");
    fx.write("components/editor/someUnmappedThing.ts", "export {}");
    fx.write("components/editor/someThing.test.ts", "test only");
    fx.write("components/editor/types.ts", "export {}");
    fx.write("components/editor/paintStrokeRenderer.ts", "export {}");

    let files = production_files(&fx.root);
    assert_eq!(
        files,
        vec![
            "components/editor/AdjustmentsPanel.tsx",
            "components/editor/canvas/HoverHighlight.tsx",
            "components/editor/paintStrokeRenderer.ts",
            "components/editor/someUnmappedThing.ts",
            "components/editor/types.ts",
        ],
        "test files are excluded and the rest sorted"
    );

    let out = out_path("branches");
    let code = run(&InventoryOpts {
        oracle_root: fx.root.clone(),
        head_root: fx.root.clone(),
        out: out.clone(),
    });
    let text = fs::read_to_string(&out).expect("csv written");
    assert!(
        text.contains("components/editor/AdjustmentsPanel.tsx,adjustments"),
        "exact override classified: {}",
        text.lines().nth(4).unwrap_or("")
    );
    assert!(text.contains(",MISSING,"), "prefix rule status carried");
    assert!(
        text.contains(",UNMAPPED,"),
        "unclassified file falls through to UNMAPPED"
    );
    // The re-baseline override classifies the C4 file instead of leaving it
    // unmapped.
    assert!(text.contains("paintStrokeRenderer.ts,brush,"));
    assert_eq!(code, 1, "UNMAPPED rows make the precheck fail");
    let _ = fs::remove_file(&out);
}

#[test]
fn head_only_files_become_out_of_target_rows() {
    let fx = Fixture::new("oot");
    let oracle = Fixture::new("oot-oracle");
    fx.write("components/editor/AdjustmentsPanel.tsx", "export {}");
    fx.write("components/editor/octoberOnly.ts", "export {}");
    oracle.write("components/editor/AdjustmentsPanel.tsx", "export {}");

    let out = out_path("oot");
    let code = run(&InventoryOpts {
        oracle_root: oracle.root.clone(),
        head_root: fx.root.clone(),
        out: out.clone(),
    });
    let text = fs::read_to_string(&out).expect("csv written");
    assert!(
        text.contains("behavior,apps/desktop/src/components/editor/octoberOnly.ts,"),
        "head-only file present: {text}"
    );
    assert!(
        text.contains(",OUT-OF-TARGET,"),
        "head-only file marked out of target"
    );
    assert_eq!(code, 0, "both trees classify fully, so validation passes");
    let _ = fs::remove_file(&out);
}

#[test]
fn validator_fails_when_a_row_is_removed() {
    // RED proof for the file-completeness layer: deleting one F row must make
    // the validator fail naming the removed file.
    let fx = Fixture::new("red");
    fx.write("components/editor/AdjustmentsPanel.tsx", "export {}");
    fx.write("components/editor/types.ts", "export {}");

    let out = out_path("red");
    let code = run(&InventoryOpts {
        oracle_root: fx.root.clone(),
        head_root: fx.root.clone(),
        out: out.clone(),
    });
    assert_eq!(code, 0, "fixture classifies fully");
    let text = fs::read_to_string(&out).expect("csv");
    let mut lines: Vec<String> = text
        .lines()
        .map(str::to_owned)
        .filter(|l| !l.starts_with("components/editor/types.ts") && !l.ends_with("types.ts,F"))
        .collect();
    lines.retain(|l| !(l.contains("types.ts") && l.starts_with("F")));
    let removed: String = lines.join("\n");
    fs::write(&out, &removed).expect("write reduced csv");
    let code = super::validate(&out, &fx.root);
    assert_eq!(code, 1, "validator must fail with a row removed");
    let _ = fs::remove_file(&out);
}

#[test]
fn aggregate_reference_rows_carry_measured_counts() {
    let fx = Fixture::new("globs");
    fx.write("components/editor/types.ts", "export {}");
    // The oracle root also needs src-tauri content to measure.
    fs::create_dir_all(fx.root.join("src-tauri/src")).expect("tauri dir");
    fs::write(fx.root.join("src-tauri/src/main.rs"), "fn main() {}").expect("rs file");
    fs::write(fx.root.join("src-tauri/src/lib.rs"), "pub fn f() {}").expect("rs file");
    fs::write(fx.root.join("src/components/editor/x.test.ts"), "test").expect("test file");

    let out = out_path("globs");
    run(&InventoryOpts {
        oracle_root: fx.root.clone(),
        head_root: fx.root.clone(),
        out: out.clone(),
    });
    let text = fs::read_to_string(&out).expect("csv");
    assert!(
        text.contains("apps/desktop/src-tauri/**/*.rs (2 files)"),
        "rs glob row carries the measured count: {text}"
    );
    assert!(
        text.contains("*.test.(ts|tsx) (1 files)"),
        "test glob row carries the measured count"
    );
    let _ = fs::remove_file(&out);
}

#[test]
fn csv_parser_handles_quoted_commas() {
    let text = "# comment\nid,path\n1,\"a.rs, b.rs\"\n2,\"say \"\"hi\"\"\"\n";
    let (header, rows) = super::csv::parse(text).expect("parses");
    assert_eq!(header, vec!["id", "path"]);
    assert_eq!(rows[0], vec!["1", "a.rs, b.rs"]);
    assert_eq!(rows[1], vec!["2", "say \"hi\""]);
}

/// Keep the unused-import lint honest when tests reference the Path import.
#[allow(unused)]
fn _assert_path_used(p: &Path) {}
