use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

const BOUNDED_RASTER: &[u8] = include_bytes!("../../tests/fixtures/bounded-raster.ptz");
const EXPECTED_CANONICAL_RGBA: &[u8] = include_bytes!("../../tests/fixtures/bounded-raster.rgba");
static NEXT_TEST_DIRECTORY: AtomicU64 = AtomicU64::new(0);

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        for _ in 0..32 {
            let id = NEXT_TEST_DIRECTORY.fetch_add(1, Ordering::Relaxed);
            let path =
                std::env::temp_dir().join(format!("photrez-app-open-{}-{id}", std::process::id()));
            match std::fs::create_dir(&path) {
                // Resolve the scratch root: a temp directory can sit behind a
                // symbolic link, which the file boundary rejects anywhere in a
                // path, so the boundary must be handed the link-free spelling.
                Ok(()) => return Self(std::fs::canonicalize(&path).unwrap_or(path)),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => panic!("create test directory: {error}"),
            }
        }
        panic!("could not allocate a unique test directory");
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// The bridge must return what the production reader parses, not a value
/// of its own. RED proof: stub `probe_document_size()` to return
/// `(1.0, 1.0)` and this test fails while the window number changes.
#[test]
fn bridge_returns_core_parsed_dimensions() {
    assert_eq!(probe_document_size(), (1748.0, 1240.0));
}

/// `App::new` seeds `AppState` from `shot::active()`.
#[test]
fn seeded_state_matches_shot() {
    let state = AppState::seeded(Some("dock-closed"));
    assert!(!state.dock_open);
}

#[test]
fn startup_preparation_loads_bounded_fixture_into_canvas() {
    let directory = TestDirectory::new();
    let fixture_path = directory.path().join("bounded-raster.ptz");
    std::fs::write(&fixture_path, BOUNDED_RASTER).expect("fixture is copied to temp path");
    let file_io = LocalFileIo::new(directory.path(), directory.path().join("approvals.json"))
        .expect("local file boundary opens");

    let (prepared, canvas) = App::prepare_startup_document(&fixture_path, &file_io)
        .expect("production startup preparation accepts fixture");
    let loaded = services::document::load_document_candidate(&file_io, &fixture_path)
        .expect("production loader accepts approved fixture");

    assert_eq!(prepared.document_id(), "parity-doc");
    assert_eq!(loaded.document_id(), prepared.document_id());
    assert_eq!((prepared.display_width, prepared.display_height), (2, 1));
    let (width, height, rgba) = loaded.canonical_rgba();
    assert_eq!((width, height), (2, 1));
    assert_eq!(rgba.len(), EXPECTED_CANONICAL_RGBA.len());
    assert_eq!(rgba, EXPECTED_CANONICAL_RGBA);
    assert_eq!(canvas.tile_count(), 1);
}

#[gpui_kit::test]
fn open_argument_composes_local_approval_loader_and_canvas(cx: &mut gpui_kit::TestAppContext) {
    use gpui_kit::base::Root;
    use gpui_kit::{px, size, AppContext, Bounds, Point, WindowBounds, WindowOptions};
    use std::ffi::OsString;

    let directory = TestDirectory::new();
    let initial_path = directory.path().join("initial.ptz");
    let rejected_path = directory.path().join("rejected.ptz");
    let later_path = directory.path().join("later.ptz");
    std::fs::write(&initial_path, BOUNDED_RASTER).expect("initial fixture is copied");
    std::fs::write(&rejected_path, b"not a PTZ archive").expect("invalid candidate is written");
    std::fs::write(&later_path, BOUNDED_RASTER).expect("later fixture is copied");

    let settings = FileSettings::at(directory.path().join("settings.json"));
    let approvals_path = settings.trusted_paths_path();
    let startup_args = crate::parse_startup_args([
        OsString::from("--open"),
        initial_path.as_os_str().to_owned(),
    ]);
    let open_path = startup_args.open_path.expect("--open path is parsed");
    assert!(
        services::settings::user_cache_root().is_some(),
        "the app host provides its normal cache root"
    );

    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::views::title_bar::init(&cx.svg_renderer());
        load_fonts(cx);
    });

    let settings_for_app = settings.clone();
    let (window, content) = cx.update(|cx| {
        let bounds = Bounds {
            origin: Point::default(),
            size: size(px(1366.), px(820.)),
        };
        let (window, content) = gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            cx,
            move |window, cx| {
                cx.new(|cx| App::new(window, Some(settings_for_app), Some(open_path), cx))
            },
        )
        .expect("open test window");
        (window.downcast::<Root>().expect("base root"), content)
    });

    let approved = |path: &Path| {
        let paths: Vec<String> = serde_json::from_slice(
            &std::fs::read(&approvals_path).expect("local approvals persist"),
        )
        .expect("local approvals are JSON");
        let canonical = std::fs::canonicalize(path).expect("approved fixture is canonical");
        assert!(paths.contains(&canonical.to_string_lossy().into_owned()));
    };

    cx.update_window(window.into(), |_, _window, cx| {
        content.update(cx, |app, cx| {
            let active = app
                .active_document
                .as_ref()
                .expect("startup document is active");
            assert_eq!(active.doc_id, "parity-doc");
            assert_eq!(active.canvas.tile_count(), 1);
            approved(&initial_path);

            let error = app
                .open_startup_path(&rejected_path, cx)
                .expect_err("malformed candidate is rejected");
            assert!(error.starts_with("document load failed:"));
            let active = app
                .active_document
                .as_ref()
                .expect("failed open preserves active");
            assert_eq!(active.doc_id, "parity-doc");
            assert_eq!(active.canvas.tile_count(), 1);

            app.open_startup_path(&later_path, cx)
                .expect("later valid fixture opens");
            let active = app
                .active_document
                .as_ref()
                .expect("later document is active");
            assert_eq!(active.doc_id, "parity-doc");
            assert_eq!(active.canvas.tile_count(), 1);
            approved(&later_path);
        });
    })
    .expect("test window remains open");
}

/// Mount the production root in a deterministic 1366x820 test window.
fn mount_app(
    cx: &mut gpui_kit::TestAppContext,
    open_path: Option<PathBuf>,
    file_io: Option<Arc<dyn FileIo>>,
) -> (gpui_kit::WindowHandle<gpui_kit::base::Root>, Entity<App>) {
    mount_app_sized(cx, open_path, file_io, 1366.0)
}

/// Mount the production root at an explicit window width.
fn mount_app_sized(
    cx: &mut gpui_kit::TestAppContext,
    open_path: Option<PathBuf>,
    file_io: Option<Arc<dyn FileIo>>,
    width: f32,
) -> (gpui_kit::WindowHandle<gpui_kit::base::Root>, Entity<App>) {
    use gpui_kit::base::Root;
    use gpui_kit::{px, size, AppContext, Bounds, Point, WindowBounds, WindowOptions};

    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::views::title_bar::init(&cx.svg_renderer());
        load_fonts(cx);
    });

    cx.update(|cx| {
        let bounds = Bounds {
            origin: Point::default(),
            size: size(px(width), px(820.)),
        };
        let (window, content) = gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            cx,
            move |window, cx| {
                cx.new(|cx| match (open_path, file_io) {
                    (Some(path), Some(file_io)) => {
                        App::new_with_file_io(window, None, Some(path), file_io, cx)
                    }
                    (None, Some(file_io)) => App::new_with_file_io(window, None, None, file_io, cx),
                    _ => App::new(window, None, None, cx),
                })
            },
        )
        .expect("open test window");
        (window.downcast::<Root>().expect("base root"), content)
    })
}

/// Mount an app whose active document is the bounded fixture, backed by a
/// real approval store so the save path can be exercised end to end.
fn mount_fixture_app(
    cx: &mut gpui_kit::TestAppContext,
    directory: &TestDirectory,
) -> (
    gpui_kit::WindowHandle<gpui_kit::base::Root>,
    Entity<App>,
    PathBuf,
) {
    mount_fixture_app_sized(cx, directory, 1366.0)
}

/// Mount the bounded fixture at the target viewport width (1536 CSS px, the
/// 1920x1080 client area at 125% scaling).
fn mount_fixture_app_sized(
    cx: &mut gpui_kit::TestAppContext,
    directory: &TestDirectory,
    width: f32,
) -> (
    gpui_kit::WindowHandle<gpui_kit::base::Root>,
    Entity<App>,
    PathBuf,
) {
    let source = directory.path().join("source.ptz");
    std::fs::write(&source, BOUNDED_RASTER).expect("fixture is copied");
    let file_io = Arc::new(
        LocalFileIo::new(directory.path(), directory.path().join("approvals.json"))
            .expect("local file boundary opens"),
    );
    file_io.approve_path(&source).expect("fixture is approved");

    let (window, content) = mount_app_sized(cx, Some(source.clone()), Some(file_io), width);
    (window, content, source)
}

/// Mount an app with a real settings store (locale/recent persistence).
fn mount_settings_app(
    cx: &mut gpui_kit::TestAppContext,
    settings: FileSettings,
    directory: &TestDirectory,
) -> (gpui_kit::WindowHandle<gpui_kit::base::Root>, Entity<App>) {
    let file_io = Arc::new(
        LocalFileIo::new(directory.path(), directory.path().join("approvals.json"))
            .expect("local file boundary opens"),
    );
    mount_app_with_settings(cx, Some(settings), Some(file_io))
}

fn mount_app_with_settings(
    cx: &mut gpui_kit::TestAppContext,
    settings: Option<FileSettings>,
    file_io: Option<Arc<dyn FileIo>>,
) -> (gpui_kit::WindowHandle<gpui_kit::base::Root>, Entity<App>) {
    use gpui_kit::base::Root;
    use gpui_kit::{px, size, AppContext, Bounds, Point, WindowBounds, WindowOptions};

    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::views::title_bar::init(&cx.svg_renderer());
        load_fonts(cx);
    });
    cx.update(|cx| {
        let bounds = Bounds {
            origin: Point::default(),
            size: size(px(1366.), px(820.)),
        };
        let (window, content) = gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            cx,
            move |window, cx| {
                cx.new(|cx| {
                    App::new_with_file_io(
                        window,
                        settings,
                        None,
                        file_io.expect("settings mount passes a file boundary"),
                        cx,
                    )
                })
            },
        )
        .expect("open test window");
        (window.downcast::<Root>().expect("base root"), content)
    })
}

/// Ctrl+S must reach the session's SaveStream: the archive is rewritten
/// from the loaded core document and the dirty revision becomes saved.
///
/// RED proof: remove the key binding or the root `on_action` and the
/// pressed shortcut leaves the session at revision 1 / saved 0 and the
/// deleted archive is not recreated.
#[path = "tests/color_picker.rs"]
mod color_picker;

#[path = "tests/layers_footer.rs"]
mod layers_footer;

#[path = "tests/option_bar.rs"]
mod option_bar;

#[path = "tests/commands.rs"]
mod commands;

#[path = "tests/save_as_cancel.rs"]
mod save_as_cancel;

#[path = "tests/crop.rs"]
mod crop;

#[path = "tests/adjustments.rs"]
mod adjustments;

#[path = "tests/empty_state.rs"]
mod empty_state;

#[path = "tests/brush_menu.rs"]
mod brush_menu;

#[path = "tests/documents.rs"]
mod documents;

#[path = "tests/input.rs"]
mod input;

#[path = "tests/layers_panel.rs"]
mod layers_panel;

#[path = "tests/menus.rs"]
mod menus;

#[path = "tests/rail.rs"]
mod rail;

#[path = "tests/selection.rs"]
mod selection;

#[path = "tests/tools.rs"]
mod tools;

#[path = "tests/transform.rs"]
mod transform;

#[path = "tests/history.rs"]
mod history;

#[path = "tests/viewport.rs"]
mod viewport;

#[path = "tests/cross_doc.rs"]
mod cross_doc;
