//! Coverage-layer fixtures: one PASS case and one failing case per rule, so
//! the layer cannot pass by checking nothing.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use super::run;

static NEXT: AtomicU64 = AtomicU64::new(0);

const INVENTORY_HEADER: &str = "id,kind,path,family,native_target,status,evidence,notes\n";
const SURFACE_HEADER: &str =
    "id,surface,frozen_refs,native_refs,layout,data,states,input,perf,overall,evidence,notes\n";

fn fixture_dir(name: &str) -> PathBuf {
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "photrez-coverage-{}-{}-{name}",
        std::process::id(),
        id
    ));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).expect("fixture dir");
    path
}

fn write(dir: &Path, file: &str, body: &str) -> PathBuf {
    let path = dir.join(file);
    std::fs::write(&path, body).expect("fixture write");
    path
}

/// One behaviour file and one component file, both named by the same surface.
fn passing(dir: &Path) -> (PathBuf, PathBuf) {
    let inventory = write(
        dir,
        "inventory.csv",
        &format!(
            "{INVENTORY_HEADER}\
B001,behavior,src/useBehavior.ts,canvas,-,WIRED,,\n\
C001,component,src/Panel.tsx,layers,-,WIRED,,\n\
C002,component,src/Shell.tsx,shell,-,WIRED,,\n"
        ),
    );
    let surfaces = write(
        dir,
        "surfaces.csv",
        &format!(
            "{SURFACE_HEADER}\
S001,canvas,src/useBehavior.ts,views/canvas.rs,PASS,PASS,PASS,PASS,PASS,VERIFIED,E1,ok\n\
S002,layers,src/Panel.tsx;src/Shell.tsx,views/layers.rs,PASS,PASS,PASS,PASS,PASS,VERIFIED,E2,ok\n"
        ),
    );
    (inventory, surfaces)
}

fn exit(inventory: &Path, surfaces: &Path) -> i32 {
    run(inventory, surfaces).expect("coverage runs")
}

#[test]
fn the_passing_fixture_reports_pass() {
    let dir = fixture_dir("pass");
    let (inventory, surfaces) = passing(&dir);
    assert_eq!(exit(&inventory, &surfaces), 0);
}

#[test]
fn a_duplicated_surface_id_fails() {
    let dir = fixture_dir("dup");
    let (inventory, _) = passing(&dir);
    let surfaces = write(
        &dir,
        "surfaces-dup.csv",
        &format!(
            "{SURFACE_HEADER}\
S001,canvas,src/useBehavior.ts,views/canvas.rs,PASS,PASS,PASS,PASS,PASS,VERIFIED,E1,ok\n\
S001,layers,src/Panel.tsx,views/layers.rs,PASS,PASS,PASS,PASS,PASS,VERIFIED,E2,ok\n"
        ),
    );
    assert_eq!(exit(&inventory, &surfaces), 1);
}

#[test]
fn a_frozen_ref_outside_the_inventory_fails() {
    let dir = fixture_dir("ref");
    let (inventory, _) = passing(&dir);
    let surfaces = write(
        &dir,
        "surfaces-ref.csv",
        &format!(
            "{SURFACE_HEADER}\
S001,canvas,src/Ghost.ts,-,PASS,PASS,PASS,PASS,PASS,VERIFIED,E1,ok\n"
        ),
    );
    assert_eq!(exit(&inventory, &surfaces), 1);
}

#[test]
fn a_component_family_with_no_surface_fails() {
    let dir = fixture_dir("family");
    let (inventory, _) = passing(&dir);
    let surfaces = write(
        &dir,
        "surfaces-family.csv",
        &format!(
            "{SURFACE_HEADER}\
S001,canvas,src/useBehavior.ts,views/canvas.rs,PASS,PASS,PASS,PASS,PASS,VERIFIED,E1,ok\n"
        ),
    );
    assert_eq!(exit(&inventory, &surfaces), 1);
}

#[test]
fn an_unreferenced_component_fails_while_infrastructure_is_exempt() {
    let dir = fixture_dir("component");
    let (inventory, _) = passing(&dir);
    let surfaces = write(
        &dir,
        "surfaces-component.csv",
        &format!(
            "{SURFACE_HEADER}\
S001,canvas,src/useBehavior.ts,views/canvas.rs,PASS,PASS,PASS,PASS,PASS,VERIFIED,E1,ok\n\
S003,shell,src/Shell.tsx,views/shell.rs,PASS,PASS,PASS,PASS,PASS,VERIFIED,E3,ok\n"
        ),
    );
    // `Panel.tsx` (family `layers`, not infrastructure) is now unnamed.
    assert_eq!(exit(&inventory, &surfaces), 1);
}

#[test]
fn an_unowned_missing_file_fails() {
    let dir = fixture_dir("missing");
    let inventory = write(
        &dir,
        "inventory-missing.csv",
        &format!(
            "{INVENTORY_HEADER}\
B001,behavior,src/useBehavior.ts,canvas,-,WIRED,,\n\
C001,component,src/Panel.tsx,layers,-,WIRED,,\n\
C003,component,src/Absent.tsx,layers,-,MISSING,,\n"
        ),
    );
    let surfaces = write(
        &dir,
        "surfaces-missing.csv",
        &format!(
            "{SURFACE_HEADER}\
S001,canvas,src/useBehavior.ts,views/canvas.rs,PASS,PASS,PASS,PASS,PASS,VERIFIED,E1,ok\n\
S002,layers,src/Panel.tsx,views/layers.rs,PASS,PASS,PASS,PASS,PASS,VERIFIED,E2,ok\n"
        ),
    );
    assert_eq!(exit(&inventory, &surfaces), 1);
}

#[test]
fn native_target_rules_are_enforced_both_ways() {
    let dir = fixture_dir("native");
    let (inventory, _) = passing(&dir);
    let missing_with_target = write(
        &dir,
        "surfaces-missing-target.csv",
        &format!(
            "{SURFACE_HEADER}\
S001,canvas,src/useBehavior.ts,views/canvas.rs,TODO,TODO,TODO,TODO,TODO,MISSING,E1,gap\n\
S002,layers,src/Panel.tsx,views/layers.rs,PASS,PASS,PASS,PASS,PASS,VERIFIED,E2,ok\n"
        ),
    );
    assert_eq!(exit(&inventory, &missing_with_target), 1);
    let implemented_without_target = write(
        &dir,
        "surfaces-no-target.csv",
        &format!(
            "{SURFACE_HEADER}\
S001,canvas,src/useBehavior.ts,-,PASS,PASS,PASS,PASS,PASS,WIRED,E1,ok\n\
S002,layers,src/Panel.tsx,views/layers.rs,PASS,PASS,PASS,PASS,PASS,VERIFIED,E2,ok\n"
        ),
    );
    assert_eq!(exit(&inventory, &implemented_without_target), 1);
}
