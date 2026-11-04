use super::*;
use gpui_kit::test::TestWindowExt as _;

/// A control's frame as four floats, so a failure message can say where the
/// element actually was when the click missed it.
fn row_frame(window: &mut gpui_kit::Window, id: &'static str) -> [f32; 4] {
    let bounds = window.find(id).bounds();
    [
        f32::from(bounds.origin.x),
        f32::from(bounds.origin.y),
        f32::from(bounds.size.width),
        f32::from(bounds.size.height),
    ]
}

/// Open the Print dialog with a pinned printer list. `print_document_action`
/// enumerates this machine's spooler, and a host with no printers gets the
/// warning row in place of the picker, so the inspector's contents would depend
/// on where the test runs. The perf tests keep the real enumeration: it is what
/// they measure.
fn open_print_dialog(content: &Entity<App>, window: &mut gpui_kit::Window, cx: &mut gpui_kit::App) {
    content.update(cx, |app, cx| {
        app.print_document_action(&crate::app::PrintDocument, cx);
        app.state_for_test().update(cx, |state, _| {
            state.printers = vec!["First printer".to_string(), "Second printer".to_string()];
            state.print_error = None;
            state.print_printer = 0;
        });
    });
    window.render_frame(cx);
    assert!(
        window.try_find("print-error").is_none(),
        "a listed printer keeps the warning row out of the inspector"
    );
}

#[gpui_kit::test]
fn print_dialog_controls_paper_orientation_and_copies(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        open_print_dialog(&content, window, cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).active_dialog,
            Some(crate::state::DialogId::Print)
        );
        // The paper list is inline behind its select, like the dialog's.
        window.click("print-paper", cx);
        window.render_frame(cx);
        window.click("print-paper-1", cx);
        window.render_frame(cx);
        window.click("print-orientation-landscape", cx);
        window.render_frame(cx);
        window.click("print-copies", cx);
        window.render_frame(cx);
        window.press("3", cx);
        window.press("enter", cx);
        window.render_frame(cx);
        window.click("print-unit", cx);
        window.render_frame(cx);
        window.click("print-unit-cm", cx);
        window.render_frame(cx);
        window.click("print-center", cx);
        window.render_frame(cx);
        window.click("print-scale-fit", cx);
        window.render_frame(cx);
        window.click("print-paper-white", cx);
        window.render_frame(cx);
        {
            let state = content.read(cx).state_for_test().read(cx);
            assert_eq!(state.print_paper, 1);
            assert!(state.print_landscape);
            assert_eq!(state.print_copies, 3, "typing 3 commits three copies");
            assert_eq!(state.print_unit, "cm");
            // This test failed once under full parallel load with no clue about why.
            // The row's own frame travels with the message so the next occurrence says
            // whether the click landed where the row actually was.
            assert!(
                !state.print_center_on_page,
                "Center on Page toggles; row frame {:?}",
                row_frame(window, "print-center")
            );
            assert!(
                !state.print_scale_to_fit,
                "Scale to Fit toggles; row frame {:?}",
                row_frame(window, "print-scale-fit")
            );
            assert!(
                !state.print_show_paper_white,
                "Show Paper White toggles; row frame {:?}",
                row_frame(window, "print-paper-white")
            );
        }
        window.click("print-cancel", cx);
        window.render_frame(cx);
        assert_eq!(
            content.read(cx).state_for_test().read(cx).active_dialog,
            None
        );
    })
    .expect("test window remains open");
}

/// The Print dialog's Scale track writes the percentage (10..=400) and drops
/// Scale to Fit, matching the dialog's range input (min 10, max 400).
#[gpui_kit::test]
fn print_scale_track_writes_the_percentage(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        open_print_dialog(&content, window, cx);
        let bounds = content
            .read(cx)
            .state_for_test()
            .read(cx)
            .print_scale_slider_bounds
            .get()
            .expect("the Scale track is laid out");
        let x = bounds.origin.x + gpui_kit::px(f32::from(bounds.size.width) * 0.5);
        let y = bounds.origin.y + bounds.size.height / 2.0;
        let _ = window.dispatch_event(
            gpui_kit::PlatformInput::MouseDown(gpui_kit::MouseDownEvent {
                button: gpui_kit::MouseButton::Left,
                position: gpui_kit::point(x, y),
                ..Default::default()
            }),
            cx,
        );
        let _ = window.dispatch_event(
            gpui_kit::PlatformInput::MouseUp(gpui_kit::MouseUpEvent {
                button: gpui_kit::MouseButton::Left,
                position: gpui_kit::point(x, y),
                click_count: 1,
                ..Default::default()
            }),
            cx,
        );
        window.render_frame(cx);
        let state = content.read(cx).state_for_test();
        let scale = state.read(cx).print_scale_percent;
        assert!(
            (200..=205).contains(&scale),
            "halfway is about 205 %, got {scale}"
        );
        assert!(
            !state.read(cx).print_scale_to_fit,
            "dragging the Scale track clears Scale to Fit"
        );
    })
    .expect("test window remains open");
}

/// The Scale track takes a click as a focus, and from there the keyboard covers
/// the same 10..=400 range the pointer path maps, clearing Scale to Fit.
#[gpui_kit::test]
fn the_print_scale_track_answers_arrow_keys(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        open_print_dialog(&content, window, cx);

        let bounds = window.find("print-scale-track").bounds();
        window.click_at(
            "print-scale-track",
            gpui_kit::point(
                gpui_kit::px(f32::from(bounds.size.width) * 0.5),
                gpui_kit::px(f32::from(bounds.size.height) * 0.5),
            ),
            cx,
        );
        window.render_frame(cx);
        let centre = content
            .read(cx)
            .state_for_test()
            .read(cx)
            .print_scale_percent;
        assert!(
            (195..=215).contains(&centre),
            "the click that focuses the track maps to about 205 %, got {centre}"
        );

        window.press("right", cx);
        window.press("up", cx);
        assert_eq!(
            content
                .read(cx)
                .state_for_test()
                .read(cx)
                .print_scale_percent,
            centre + 2,
            "arrows step the scale up one percent each"
        );
        window.press("home", cx);
        assert_eq!(
            content
                .read(cx)
                .state_for_test()
                .read(cx)
                .print_scale_percent,
            10,
            "Home reaches the track's 10 % floor"
        );
        window.press("end", cx);
        let state = content.read(cx).state_for_test();
        assert_eq!(
            state.read(cx).print_scale_percent,
            400,
            "End reaches the track's 400 % ceiling"
        );
        assert!(
            !state.read(cx).print_scale_to_fit,
            "keyboard steps leave Fit cleared, as a drag does"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
#[ignore = "perf: run serially (--ignored --nocapture); parallel load inflates p95"]
fn print_dialog_latency(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        let close = |content: &gpui_kit::Entity<App>, cx: &mut gpui_kit::App| {
            content.update(cx, |app, cx| {
                app.state_for_test().update(cx, |state, _| {
                    state.active_dialog = None;
                });
            });
        };
        close(&content, cx);
        window.render_frame(cx);
        let mut samples = Vec::new();
        for _ in 0..9 {
            close(&content, cx);
            window.render_frame(cx);
            let start = std::time::Instant::now();
            content.update(cx, |app, cx| {
                app.print_document_action(&crate::app::PrintDocument, cx);
            });
            window.render_frame(cx);
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        samples.sort_by(f64::total_cmp);
        let p50 = samples[samples.len() / 2];
        let p95 = samples[(samples.len() * 95 / 100).min(samples.len() - 1)];
        let listed: Vec<String> = samples.iter().map(|s| format!("{s:.4}")).collect();
        println!(
            "print_dialog p50={p50:.3} p95={p95:.3} ms n={} samples=[{}]",
            samples.len(),
            listed.join(",")
        );
        assert!(
            p95 <= 33.3,
            "print dialog p95 {p95:.3} ms over the 33.3 ms budget"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
#[ignore = "perf: run serially (--ignored --nocapture); parallel load inflates p95"]
fn print_scale_latency(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        content.update(cx, |app, cx| {
            app.print_document_action(&crate::app::PrintDocument, cx);
        });
        window.render_frame(cx);
        let bounds = content
            .read(cx)
            .state_for_test()
            .read(cx)
            .print_scale_slider_bounds
            .get()
            .expect("the Scale track is laid out");
        let mut samples = Vec::new();
        for index in 0..9 {
            let fraction = if index % 2 == 0 { 0.3 } else { 0.7 };
            let x = bounds.origin.x + gpui_kit::px(f32::from(bounds.size.width) * fraction);
            let y = bounds.origin.y + bounds.size.height / 2.0;
            let start = std::time::Instant::now();
            let _ = window.dispatch_event(
                gpui_kit::PlatformInput::MouseDown(gpui_kit::MouseDownEvent {
                    button: gpui_kit::MouseButton::Left,
                    position: gpui_kit::point(x, y),
                    ..Default::default()
                }),
                cx,
            );
            let _ = window.dispatch_event(
                gpui_kit::PlatformInput::MouseUp(gpui_kit::MouseUpEvent {
                    button: gpui_kit::MouseButton::Left,
                    position: gpui_kit::point(x, y),
                    click_count: 1,
                    ..Default::default()
                }),
                cx,
            );
            window.render_frame(cx);
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        samples.sort_by(f64::total_cmp);
        let p50 = samples[samples.len() / 2];
        let p95 = samples[(samples.len() * 95 / 100).min(samples.len() - 1)];
        let listed: Vec<String> = samples.iter().map(|s| format!("{s:.4}")).collect();
        println!(
            "print_scale p50={p50:.3} p95={p95:.3} ms n={} samples=[{}]",
            samples.len(),
            listed.join(",")
        );
        assert!(
            p95 <= 33.3,
            "print scale p95 {p95:.3} ms over the 33.3 ms budget"
        );
    })
    .expect("test window remains open");
}

#[gpui_kit::test]
fn the_printer_properties_button_carries_the_catalog_copy(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        open_print_dialog(&content, window, cx);
        // Measured on this mount: 135 px for the catalog copy alone, 154 px once an
        // extra ellipsis is appended to it. The copy ends in an ellipsis in both
        // catalogs, so the wider button is the suffix coming back.
        let frame = row_frame(window, "print-settings");
        assert!(
            frame[2] <= 140.,
            "the printer-properties button is {} px wide, so its label carries more than the \
             catalog copy; row frame {:?}",
            frame[2],
            frame
        );
    })
    .expect("test window remains open");
}

/// Reads the open number session, if a field has one.
fn num_session(content: &Entity<App>, cx: &gpui_kit::App) -> Option<crate::state::NumFieldSession> {
    content.read(cx).state_for_test().read(cx).num_field.clone()
}

/// The copies row takes a typed count. The `- 1 +` stepper it replaces is
/// gone, and the row keeps the dialog's 1..=99 clamp.
#[gpui_kit::test]
fn the_copies_row_is_a_typed_field_not_a_stepper(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        open_print_dialog(&content, window, cx);
        assert!(
            window.try_find("print-copies").is_some(),
            "the copies row is one editable field"
        );
        assert!(
            window.try_find("print-copies-dec").is_none(),
            "the minus stepper button is gone"
        );
        assert!(
            window.try_find("print-copies-inc").is_none(),
            "the plus stepper button is gone"
        );

        window.click("print-copies", cx);
        window.render_frame(cx);
        let opened = num_session(&content, cx).expect("the copies field opens an edit session");
        assert_eq!(
            (opened.id, opened.buffer.as_str()),
            ("print-copies", "1"),
            "the field seeds the dialog's own copy count"
        );
        for key in ["1", "2"] {
            window.press(key, cx);
        }
        window.press("enter", cx);
        window.render_frame(cx);
        let typed = content.read(cx).state_for_test().read(cx).print_copies;
        assert_eq!(typed, 12, "typing 12 commits twelve copies");

        // Over the ceiling: the field clamps to 99 rather than accepting 999.
        window.click("print-copies", cx);
        for key in ["9", "9", "9"] {
            window.press(key, cx);
        }
        window.press("enter", cx);
        window.render_frame(cx);
        let ceiling = content.read(cx).state_for_test().read(cx).print_copies;
        assert_eq!(ceiling, 99, "the typed count keeps the 99-copy ceiling");

        // Under the floor: a zero commits one copy, not zero.
        window.click("print-copies", cx);
        window.press("0", cx);
        window.press("enter", cx);
        window.render_frame(cx);
        let floor = content.read(cx).state_for_test().read(cx).print_copies;
        assert_eq!(floor, 1, "the typed count keeps the 1-copy floor");
    })
    .expect("test window remains open");
}

/// Each orientation button draws the page shape the reference puts inside it,
/// in that button's own aspect, and clicking it still drives the orientation.
#[gpui_kit::test]
fn the_orientation_buttons_draw_a_page_shape(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        open_print_dialog(&content, window, cx);
        assert!(
            window.try_find("print-portrait-glyph").is_some(),
            "the Portrait button draws a page, not only its label"
        );
        assert!(
            window.try_find("print-landscape-glyph").is_some(),
            "the Landscape button draws a page, not only its label"
        );
        let portrait = row_frame(window, "print-portrait-glyph");
        let landscape = row_frame(window, "print-landscape-glyph");
        assert!(
            portrait[2] > 0.0 && portrait[3] > 0.0,
            "the portrait button paints a page, not only text; frame {portrait:?}"
        );
        assert!(
            portrait[3] > portrait[2],
            "the portrait page is taller than it is wide; frame {portrait:?}"
        );
        assert!(
            landscape[2] > landscape[3],
            "the landscape page is wider than it is tall; frame {landscape:?}"
        );

        window.click("print-orientation-landscape", cx);
        window.render_frame(cx);
        let landscape_on = content.read(cx).state_for_test().read(cx).print_landscape;
        assert!(
            landscape_on,
            "the landscape button still drives the orientation the dialog prints with"
        );
    })
    .expect("test window remains open");
}

/// The printed Width field is editable: a typed size becomes the scale
/// percentage the dialog already drives, and it leaves Scale to Fit.
#[gpui_kit::test]
fn the_printed_size_fields_edit_the_scale(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        open_print_dialog(&content, window, cx);
        let state = content.read(cx).state_for_test();
        assert!(
            state.read(cx).print_scale_to_fit,
            "the dialog starts in Scale to Fit, so the percentage is not yet the driver"
        );

        window.click("print-width", cx);
        window.render_frame(cx);
        let width_seed: f64 = num_session(&content, cx)
            .expect("the Width field opens an edit session")
            .buffer
            .parse()
            .expect("the Width seed is the number the row shows");
        window.press("escape", cx);

        window.click("print-height", cx);
        window.render_frame(cx);
        let height_seed: f64 = num_session(&content, cx)
            .expect("the Height field opens an edit session")
            .buffer
            .parse()
            .expect("the Height seed is the number the row shows");
        window.press("escape", cx);
        assert!(
            (width_seed - height_seed).abs() > 0.05,
            "each axis seeds its own size, got {width_seed} and {height_seed}"
        );

        // Type the Height axis's own size into the Width field. The percentage
        // that follows is that size over the Width basis, so the two axes cannot
        // be sharing one seed.
        let expected = (height_seed / width_seed * 100.0)
            .round()
            .clamp(10.0, 400.0) as u32;
        window.click("print-width", cx);
        window.render_frame(cx);
        for key in height_seed.to_string().chars() {
            window.press(&key.to_string(), cx);
        }
        window.press("enter", cx);
        window.render_frame(cx);
        let state = content.read(cx).state_for_test();
        assert_eq!(
            state.read(cx).print_scale_percent,
            expected,
            "a typed Width sets the scale the dialog prints with"
        );
        assert!(
            !state.read(cx).print_scale_to_fit,
            "an edited size leaves Scale to Fit, as the reference does"
        );
    })
    .expect("test window remains open");
}

/// Stores what the driver answered and lays the dialog out again.
fn set_driver_papers(
    window: &mut gpui_kit::Window,
    content: &Entity<App>,
    cx: &mut gpui_kit::App,
    papers: Option<Vec<(String, f64, f64)>>,
    error: Option<String>,
) {
    content.update(cx, |app, cx| {
        app.state_for_test().update(cx, |state, _| {
            state.print_papers = papers;
            state.print_papers_error = error;
        });
    });
    window.render_frame(cx);
}

/// The Paper Size row answers from the driver's own list: a populated list is
/// shown, an empty list claims nothing extra, and a failure says why instead of
/// rendering an empty list silently.
#[gpui_kit::test]
fn the_paper_row_reports_what_the_driver_lists(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        open_print_dialog(&content, window, cx);
        // The driver's answer sits inside the open Paper list, so the inspector
        // keeps its fixed height when a printer names many sizes.
        window.click("print-paper", cx);
        window.render_frame(cx);

        // Nothing asked: the row says it has no list rather than claiming the
        // chosen paper is missing.
        set_driver_papers(window, &content, cx, None, None);
        assert!(
            window.try_find("print-paper-unknown").is_some(),
            "an unanswered platform says so on its own row"
        );
        assert!(window.try_find("print-paper-reported").is_none());
        assert!(window.try_find("print-paper-error").is_none());

        set_driver_papers(
            window,
            &content,
            cx,
            Some(vec![
                ("A4 (210 x 297 mm)".to_string(), 210.0, 297.0),
                ("Letter".to_string(), 216.0, 279.0),
            ]),
            None,
        );
        assert!(
            window.try_find("print-paper-reported").is_some(),
            "the driver's own sizes are listed"
        );
        assert!(window.try_find("print-paper-unknown").is_none());

        // The driver answered with nothing: that is the one case that earns the
        // "not listed on this printer" tail, so no extra row appears.
        set_driver_papers(window, &content, cx, Some(Vec::new()), None);
        assert!(window.try_find("print-paper-reported").is_none());
        assert!(window.try_find("print-paper-unknown").is_none());

        set_driver_papers(
            window,
            &content,
            cx,
            None,
            Some("Failed to enumerate printer papers".into()),
        );
        assert!(
            window.try_find("print-paper-error").is_some(),
            "a failed enumeration reports its reason rather than an empty list"
        );
    })
    .expect("test window remains open");
}

/// Picking a printer asks that printer's driver, because another printer has a
/// different list. Only Windows has an enumerator; there the answer is either
/// the sizes or the reason it failed, never a silent nothing.
#[cfg(windows)]
#[gpui_kit::test]
fn picking_a_printer_asks_its_driver_for_its_paper_sizes(cx: &mut gpui_kit::TestAppContext) {
    let directory = TestDirectory::new();
    let (window, content, _source) = mount_fixture_app(cx, &directory);
    cx.update_window(window.into(), |_, window, cx| {
        open_print_dialog(&content, window, cx);
        content.update(cx, |app, cx| {
            app.state_for_test().update(cx, |state, _| {
                state.print_papers = None;
                state.print_papers_error = None;
            });
        });
        window.render_frame(cx);

        window.click("print-printer-trigger", cx);
        window.render_frame(cx);
        window.click("print-printer-1", cx);
        window.render_frame(cx);

        let state = content.read(cx).state_for_test().read(cx);
        assert_eq!(
            state.print_printer, 1,
            "the pick selects the second printer"
        );
        assert!(
            state.print_papers.is_some() || state.print_papers_error.is_some(),
            "the pick enumerates the new printer's papers, or records why it could not"
        );
    })
    .expect("test window remains open");
}
