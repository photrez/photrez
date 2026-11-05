use super::*;

pub(super) fn run_shape(baseline: &RgbaImage, port: &RgbaImage) -> Result<i32, String> {
    let regions = shape_regions();
    let reports: Vec<ShapeRegionReport> =
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            shapediff::compare_anchored(baseline, port, &regions)
        })) {
            Ok(reports) => reports,
            Err(_) => {
                println!("  shape: engine panicked - the port has a bug");
                return Ok(report_case("shape", 1));
            }
        };
    if reports.len() != 2 {
        println!("  shape: expected 2 regions, got {}", reports.len());
        return Ok(report_case("shape", 1));
    }
    let mut fails = 0i32;
    let expected = shape_expectations();
    print_header("shape", "left-tool-rail.png", "gpui-strip.png");
    for (r, e) in reports.iter().zip(expected.iter()) {
        assert_f32(
            "shape",
            &format!("{}/agree", e.name),
            r.agree_pct,
            e.agree,
            TOL_R3,
            &mut fails,
        );
        assert_f32(
            "shape",
            &format!("{}/fringe", e.name),
            r.fringe_only_pct,
            e.fringe,
            TOL_R3,
            &mut fails,
        );
        assert_f32(
            "shape",
            &format!("{}/hard", e.name),
            r.hard_geometry_mismatch_pct,
            e.hard,
            TOL_R3,
            &mut fails,
        );
        // histogram order is fixed (bg, active, stroke, orange, white)
        let hist: Vec<_> = r.histogram.iter().collect();
        if hist.len() != e.hist.len() {
            println!(
                "  shape {}/histogram: got {} rows, want {} -> FAIL",
                e.name,
                hist.len(),
                e.hist.len()
            );
            fails += 1;
        } else {
            for (i, (c, o, p)) in e.hist.iter().enumerate() {
                let row = &hist[i];
                if row.class != *c {
                    println!(
                        "  shape {}/hist[{}]: class {} != {} -> FAIL",
                        e.name, i, row.class, c
                    );
                    fails += 1;
                }
                assert_eq_int(
                    "shape",
                    &format!("{}/hist[{}].original", e.name, c),
                    row.original,
                    *o,
                    &mut fails,
                );
                assert_eq_int(
                    "shape",
                    &format!("{}/hist[{}].port", e.name, c),
                    row.port,
                    *p,
                    &mut fails,
                );
            }
        }
    }
    Ok(report_case("shape", fails))
}

pub(super) fn run_diff(baseline: &RgbaImage, port: &RgbaImage) -> Result<i32, String> {
    let regions = shape_regions(); // tools 400 / swatch 80, same as diff-report.json
    let reports: Vec<super::RawRegionReport> =
        match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            rawdiff::compare_anchored(baseline, port, &regions)
        })) {
            Ok(res) => res?,
            Err(_) => {
                println!("  diff: engine panicked - the port has a bug");
                return Ok(report_case("diff", 1));
            }
        };
    if reports.len() != 2 {
        println!("  diff: expected 2 regions, got {}", reports.len());
        return Ok(report_case("diff", 1));
    }
    let mut fails = 0i32;
    let expected = diff_expectations();
    print_header("diff", "left-tool-rail.png", "gpui-strip.png");
    for (r, e) in reports.iter().zip(expected.iter()) {
        assert_eq_int(
            "diff",
            &format!("{}/diff_px", e.name),
            r.diff_pixels,
            e.diff_px,
            &mut fails,
        );
        assert_f32(
            "diff",
            &format!("{}/diff_pct", e.name),
            r.diff_pct,
            e.diff_pct,
            TOL_R3,
            &mut fails,
        );
        assert_eq_int(
            "diff",
            &format!("{}/worst_row", e.name),
            u64::from(r.worst_row.row),
            u64::from(e.worst_row),
            &mut fails,
        );
        assert_f32(
            "diff",
            &format!("{}/worst_row/pct", e.name),
            r.worst_row.pct,
            e.worst_pct,
            TOL_PCT,
            &mut fails,
        );
        assert_eq_int(
            "diff",
            &format!("{}/max_delta", e.name),
            u64::from(r.max_channel_delta),
            u64::from(e.max_delta),
            &mut fails,
        );
        let ok_at = r.max_at == e.max_at;
        if !ok_at {
            fails += 1;
        }
        println!(
            "  diff {}/max_at: got {:?} want {:?} -> {}",
            e.name,
            r.max_at,
            e.max_at,
            if ok_at { "PASS" } else { "FAIL" }
        );
    }
    Ok(report_case("diff", fails))
}

pub(super) fn run_zoom(baseline: &RgbaImage, port: &RgbaImage) -> Result<i32, String> {
    let regions = zoom_regions();
    let reports: Vec<InkRegionReport> =
        zoom::compare_anchored(baseline, port, &regions, zoom::DEFAULT_BG_LUMA)?;
    if reports.len() != 2 {
        println!("  zoom: expected 2 regions, got {}", reports.len());
        return Ok(report_case("zoom", 1));
    }
    let mut fails = 0i32;
    print_header("zoom", "left-tool-rail.png", "gpui-strip.png");
    // Tools reproduces the recording (~1.146x). The swatch ratio pins the
    // current capture; the recorded 2026-10-08 value (1.167) moved with the
    // fixture recapture - see the module docs.
    assert_f32(
        "zoom",
        "tools/ratio",
        reports[0].ratio,
        1.146,
        TOL_R3,
        &mut fails,
    );
    assert_f32(
        "zoom",
        "swatch/ratio",
        reports[1].ratio,
        1.014,
        TOL_R3,
        &mut fails,
    );
    Ok(report_case("zoom", fails))
}
