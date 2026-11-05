//! Building the app: the constructors and the long `new_inner` that wires state,
//! actions, focus and the startup document together.

use super::*;

impl App {
    /// The cache a launch may recover from: only a settings-backed one owns the
    /// real user cache directory.
    pub(super) fn startup_cache_root(settings: &Option<FileSettings>) -> Option<PathBuf> {
        settings
            .as_ref()
            .and_then(|_| services::settings::user_cache_root())
    }

    /// Creates a new `App`, seeding `AppState` from the active shot id,
    /// building the demo document's display snapshot through core, and
    /// subscribing to the window geometry it will persist.
    pub fn new(
        window: &mut Window,
        settings: Option<FileSettings>,
        open_path: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) -> Self {
        let cache_root = Self::startup_cache_root(&settings);
        Self::new_inner(window, settings, open_path, None, cache_root, cx)
    }

    #[cfg(test)]
    pub(crate) fn new_with_file_io(
        window: &mut Window,
        settings: Option<FileSettings>,
        open_path: Option<PathBuf>,
        file_io: Arc<dyn FileIo>,
        cx: &mut Context<Self>,
    ) -> Self {
        let cache_root = Self::startup_cache_root(&settings);
        Self::new_inner(window, settings, open_path, Some(file_io), cache_root, cx)
    }

    /// A mount whose startup recovery reads a cache the test owns. Without this
    /// seam the explicit-open-then-recovery order only runs when the process has
    /// a real user cache directory, so no mounted test could drive it.
    #[cfg(test)]
    pub(crate) fn new_with_cache_root(
        window: &mut Window,
        settings: Option<FileSettings>,
        open_path: Option<PathBuf>,
        file_io: Arc<dyn FileIo>,
        cache_root: PathBuf,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::new_inner(
            window,
            settings,
            open_path,
            Some(file_io),
            Some(cache_root),
            cx,
        )
    }

    pub(super) fn new_inner(
        window: &mut Window,
        settings: Option<FileSettings>,
        open_path: Option<PathBuf>,
        file_io: Option<Arc<dyn FileIo>>,
        cache_root: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) -> Self {
        let shot_id = crate::shot::active();
        let mut state = if shot_id.is_none() {
            AppState::empty()
        } else {
            AppState::seeded(shot_id)
        };
        // A `--shot` launch seeds the demo document without a session, so the
        // tab strip's projection from real sessions would be empty while the
        // document is on screen. The captured shell has one open document, so
        // seed its display-only tab row to match.
        if shot_id.is_some() {
            state.seed_shot_tab();
        }
        // The demo document display snapshot belongs to the demo state: an
        // empty-* launch renders no document, so its chrome (navigator
        // thumbnail, canvas area, status-bar stats) must not show one.
        let demo_canvas = if state.has_document {
            let (width, height, rgba) = services::document::blank_white_document();
            match CanvasImage::from_rgba(width, height, rgba) {
                Ok(canvas) => Some(canvas),
                Err(error) => {
                    eprintln!(
                        "photrez: the demo document was refused by the display seam: {error:?}"
                    );
                    None
                }
            }
        } else {
            None
        };

        cx.bind_keys([
            KeyBinding::new("ctrl-n", NewDocumentAction, None),
            KeyBinding::new("ctrl-shift-p", ToggleSidePanelsAction, None),
            KeyBinding::new("ctrl-s", SaveDocument, None),
            KeyBinding::new("ctrl-shift-s", SaveDocumentAs, None),
            KeyBinding::new("ctrl-o", OpenDocument, None),
            KeyBinding::new("ctrl-alt-e", ExportDocument, None),
            KeyBinding::new("ctrl-p", PrintDocument, None),
            KeyBinding::new("ctrl-w", CloseDocument, None),
        ]);
        cx.bind_keys([
            KeyBinding::new("v", input::SetCanvasTool(ToolId::Move), Some("Canvas")),
            KeyBinding::new("m", input::SetCanvasTool(ToolId::Selection), Some("Canvas")),
            KeyBinding::new("c", input::SetCanvasTool(ToolId::Crop), Some("Canvas")),
            KeyBinding::new(
                "i",
                input::SetCanvasTool(ToolId::Eyedropper),
                Some("Canvas"),
            ),
            KeyBinding::new("b", input::SetCanvasTool(ToolId::Brush), Some("Canvas")),
            KeyBinding::new("e", input::SetCanvasTool(ToolId::Eraser), Some("Canvas")),
            KeyBinding::new(
                "g",
                input::SetCanvasTool(ToolId::PaintBucket),
                Some("Canvas"),
            ),
            KeyBinding::new(
                "shift-g",
                input::SetCanvasTool(ToolId::Gradient),
                Some("Canvas"),
            ),
            KeyBinding::new("u", input::SetCanvasTool(ToolId::Shape), Some("Canvas")),
            KeyBinding::new("t", input::SetCanvasTool(ToolId::Text), Some("Canvas")),
            KeyBinding::new("shift-m", input::ToggleSelectionShape, Some("Canvas")),
            KeyBinding::new("escape", input::CancelCanvasGesture, Some("Canvas")),
            KeyBinding::new("ctrl-z", history::UndoDocument, Some("Canvas")),
            KeyBinding::new("ctrl-shift-z", history::RedoDocument, Some("Canvas")),
            KeyBinding::new("ctrl-y", history::RedoDocument, Some("Canvas")),
            KeyBinding::new("ctrl-0", viewport::FitToScreenAction, Some("Canvas")),
            KeyBinding::new("ctrl-1", viewport::ActualSizeAction, Some("Canvas")),
            KeyBinding::new(
                "ctrl-alt-0",
                viewport::ZoomToSelectionAction,
                Some("Canvas"),
            ),
            KeyBinding::new("ctrl-=", viewport::ZoomInAction, Some("Canvas")),
            KeyBinding::new("ctrl-shift-=", viewport::ZoomInAction, Some("Canvas")),
            KeyBinding::new("ctrl--", viewport::ZoomOutAction, Some("Canvas")),
            KeyBinding::new("ctrl-shift--", viewport::ZoomOutAction, Some("Canvas")),
            KeyBinding::new("enter", crop::ApplyCropAction, Some("Canvas")),
            KeyBinding::new("left", crop::NudgeCropLeft, Some("Canvas")),
            KeyBinding::new("right", crop::NudgeCropRight, Some("Canvas")),
            KeyBinding::new("up", crop::NudgeCropUp, Some("Canvas")),
            KeyBinding::new("down", crop::NudgeCropDown, Some("Canvas")),
            KeyBinding::new("shift-left", crop::NudgeCropLeftLarge, Some("Canvas")),
            KeyBinding::new("shift-right", crop::NudgeCropRightLarge, Some("Canvas")),
            KeyBinding::new("shift-up", crop::NudgeCropUpLarge, Some("Canvas")),
            KeyBinding::new("shift-down", crop::NudgeCropDownLarge, Some("Canvas")),
            KeyBinding::new("ctrl-j", layers::DuplicateLayerAction, Some("Canvas")),
            KeyBinding::new("ctrl-shift-n", NewLayerAction, Some("Canvas")),
            KeyBinding::new("ctrl-alt-a", SelectAllLayersAction, Some("Canvas")),
            KeyBinding::new("ctrl-shift-alt-e", StampVisibleAction, Some("Canvas")),
            KeyBinding::new(
                "ctrl-shift-;",
                tool_settings::ToggleSnapAction,
                Some("Canvas"),
            ),
            KeyBinding::new("ctrl-e", layers::MergeLayerDownAction, Some("Canvas")),
            KeyBinding::new(
                "ctrl-shift-e",
                layers::FlattenDocumentAction,
                Some("Canvas"),
            ),
            KeyBinding::new("ctrl-a", selection::SelectAllAction, Some("Canvas")),
            KeyBinding::new("ctrl-d", selection::DeselectAction, Some("Canvas")),
            KeyBinding::new(
                "ctrl-t",
                selection::ToggleSelectionEditModeAction,
                Some("Canvas"),
            ),
            KeyBinding::new(
                "ctrl-shift-i",
                selection::InvertSelectionAction,
                Some("Canvas"),
            ),
            KeyBinding::new("ctrl-c", selection::CopySelectionAction, Some("Canvas")),
            KeyBinding::new("ctrl-x", selection::CutSelectionAction, Some("Canvas")),
            KeyBinding::new("ctrl-v", selection::PasteSelectionAction, Some("Canvas")),
            KeyBinding::new("delete", selection::DeleteSelectionAction, Some("Canvas")),
            KeyBinding::new(
                "backspace",
                selection::DeleteSelectionAction,
                Some("Canvas"),
            ),
            KeyBinding::new("[", input::DecreaseBrushSize, Some("Canvas")),
            KeyBinding::new("]", input::IncreaseBrushSize, Some("Canvas")),
            KeyBinding::new("shift-[", input::DecreaseBrushHardness, Some("Canvas")),
            KeyBinding::new("shift-]", input::IncreaseBrushHardness, Some("Canvas")),
        ]);

        let mut window_watch = Vec::new();
        // Resizes (and any bounds notification) are throttled; the settle
        // points always write.
        window_watch.push(cx.observe_window_bounds(window, |this, window, _cx| {
            this.remember_bounds(window);
            this.persist_bounds(false);
        }));
        // Focus loss is the settle point that follows a move, which the
        // platform reports no event for: sample and write there.
        window_watch.push(cx.observe_window_activation(window, |this, window, cx| {
            if !window.is_window_active() {
                // Losing the window is the lost-capture signal: never commit a
                // stroke whose release may land in another window.
                this.release_canvas_pointer(window);
                this.cancel_canvas_gesture(cx);
                this.release_navigation(cx);
            }
            this.remember_bounds(window);
            this.persist_bounds(true);
        }));
        window_watch.push(cx.on_app_quit(|this, _cx| {
            // Multi-document quit: dirty sessions that are not the one the
            // close prompt handled are written to the recovery store so the
            // next launch can restore them.
            if let Some(root) = this.autosave_cache_root.clone() {
                let _ = this.autosave_dirty_sessions(&root);
            }
            this.persist_bounds(true);
            async {}
        }));
        let close_weak = cx.weak_entity();
        window.on_window_should_close(cx, move |_window, cx| {
            close_weak
                .update(cx, |app, cx| app.should_close_window(cx))
                .unwrap_or(true)
        });

        // Each inline numeric field is a tab stop of its own, so Tab lands on
        // the row the keyboard points at and the keystrokes follow real focus.
        let num_field_foci: Vec<_> = crate::app::num_field::NUM_FIELD_IDS
            .iter()
            .map(|_| cx.focus_handle().tab_stop(true))
            .collect();
        let slider_foci: Vec<_> = (0..crate::state::SliderTarget::COUNT)
            .map(|_| cx.focus_handle().tab_stop(true))
            .collect();
        let move_snap_focus = cx.focus_handle().tab_stop(true);
        let move_auto_select_focus = cx.focus_handle().tab_stop(true);
        let move_transform_focus = cx.focus_handle().tab_stop(true);
        // Dialog text fields are tab stops too, so Tab lands on the field and
        // the keystrokes follow real focus instead of a session index.
        let new_document_field_foci: Vec<_> = (0..crate::state::AppState::NEW_DOCUMENT_FIELD_COUNT)
            .map(|_| cx.focus_handle().tab_stop(true))
            .collect();
        let resize_field_foci: Vec<_> = (0..crate::state::AppState::RESIZE_FIELD_COUNT)
            .map(|_| cx.focus_handle().tab_stop(true))
            .collect();
        let rename_field_focus = cx.focus_handle().tab_stop(true);
        let dialog_trap_focus = cx.focus_handle();
        let mut app = Self {
            state: cx.new(|_| {
                let mut state = state;
                state.num_field_foci = num_field_foci;
                state.slider_foci = slider_foci;
                state.move_snap_focus = Some(move_snap_focus);
                state.move_auto_select_focus = Some(move_auto_select_focus);
                state.move_transform_focus = Some(move_transform_focus);
                state.new_document_field_foci = new_document_field_foci;
                state.resize_field_foci = resize_field_foci;
                state.rename_field_focus = Some(rename_field_focus);
                state.dialog_trap_focus = Some(dialog_trap_focus);
                state
            }),
            canvas_focus_handle: cx.focus_handle().tab_stop(true),
            rename_focus: cx.focus_handle(),
            resize_focus: cx.focus_handle(),
            new_document_focus: cx.focus_handle(),
            dialog_focus: cx.focus_handle(),
            shell_focus_before_dialog: None,
            dialog_was_open: false,
            pending_new_document_focus: false,
            picker_focus: cx.focus_handle(),
            demo_canvas,
            active_document: None,
            document_tabs: Vec::new(),
            tab_cameras: HashMap::new(),
            layer_thumbs: Vec::new(),
            recent_files: Vec::new(),
            dropped_paths: Vec::new(),
            dropped_position: None,
            crop_drag: None,
            move_marquee: None,
            transform_drag: None,
            selection_rotation_drag: None,
            selection_edit_mode: false,
            group_transform_drag: None,
            pending_transform_drag_update: None,
            transform_drag_frame_pending: false,
            transform_drag_frame_generation: 0,
            snap_guides: Vec::new(),
            hover_pos: None,
            last_cursor: crate::input::CursorPolicy::SystemDefault,
            edge_pointer: None,
            edge_scroll_active: false,
            edge_scroll_generation: 0,
            edge_scroll_task: None,
            live: Some(restorable_geometry(window)),
            saved: None,
            saved_at: None,
            settings,
            sessions: SessionRegistry::default(),
            file_io,
            retired_textures: Vec::new(),
            print_backend: Arc::new(PrintService::default()),
            close_after_save: false,
            close_after_dirty: false,
            pending_window_close: false,
            pending_tab_close: None,
            input: crate::input::InputState::new(),
            clipboard: None,
            brush_tips: crate::input::BrushTipCache::default(),
            paint_smoother: input::PaintSmoother::default(),
            brush_axis_lock: None,
            brush_stroke_start: None,
            brush_stroke_start_screen: None,
            alt_sampling: false,
            brush_adjust: None,
            canvas_capture: Rc::new(Cell::new(None)),
            canvas_bounds: Rc::new(Cell::new(None)),
            checker_cache: Rc::new(RefCell::new(CheckerCache::default())),
            navigator_bounds: Rc::new(Cell::new(None)),
            space_pressed: false,
            pointer_alt: false,
            pan: None,
            slider_dragging: false,
            #[cfg(feature = "input-measurement")]
            measurement: measurement::InputMeasurement::default(),
            pending_gesture: None,
            _window_watch: window_watch,
            #[cfg(test)]
            full_composites: Cell::new(0),
            #[cfg(test)]
            preview_pushes: Cell::new(0),
            autosave_task: None,
            autosave_cache_root: None,
        };
        #[cfg(feature = "input-measurement")]
        app.measurement.share_bounds(Rc::clone(&app.canvas_bounds));
        let blur = cx.on_blur(&app.canvas_focus_handle, window, |this, _window, cx| {
            this.release_navigation(cx);
        });
        app._window_watch.push(blur);
        // Only a settings-backed launch owns the real app cache; a test mount
        // injects its own root through `new_with_cache_root`.
        app.autosave_cache_root = cache_root;
        app.apply_startup_locale();
        app.start_autosave_timer(cx);
        app.load_recent_files(cx);
        // The shortcuts are window-global: they must fire wherever focus sits,
        // so they are registered on the app rather than on a focusable element.
        register_global_action::<SaveDocument>(cx, App::save_document_action);
        register_global_action::<NewDocumentAction>(cx, App::new_document_action);
        register_global_action::<ToggleSidePanelsAction>(cx, App::toggle_side_panels_action);
        register_global_action::<SaveDocumentAs>(cx, App::save_document_as_action);
        register_global_action::<OpenDocument>(cx, App::open_document_action);
        register_global_action::<ExportDocument>(cx, App::export_document_action);
        register_global_action::<PrintDocument>(cx, App::print_document_action);
        register_global_action::<CloseDocument>(cx, App::close_document_action);
        register_global_action::<input::SetCanvasTool>(cx, App::set_canvas_tool_action);
        register_global_action::<input::ToggleSelectionShape>(
            cx,
            App::toggle_selection_shape_action,
        );
        register_global_action::<input::DecreaseBrushSize>(cx, App::decrease_brush_size_action);
        register_global_action::<input::IncreaseBrushSize>(cx, App::increase_brush_size_action);
        register_global_action::<input::DecreaseBrushHardness>(
            cx,
            App::decrease_brush_hardness_action,
        );
        register_global_action::<input::IncreaseBrushHardness>(
            cx,
            App::increase_brush_hardness_action,
        );
        register_global_action::<input::CancelCanvasGesture>(cx, App::cancel_canvas_gesture_action);
        register_global_action::<history::UndoDocument>(cx, App::undo_document_action);
        register_global_action::<history::RedoDocument>(cx, App::redo_document_action);
        register_global_action::<viewport::ZoomInAction>(cx, App::zoom_in_action);
        register_global_action::<viewport::ZoomOutAction>(cx, App::zoom_out_action);
        register_global_action::<viewport::FitToScreenAction>(cx, App::fit_to_screen_action);
        register_global_action::<viewport::ActualSizeAction>(cx, App::actual_size_action);
        register_global_action::<viewport::ZoomToSelectionAction>(
            cx,
            App::zoom_to_selection_action,
        );
        register_global_action::<crop::ApplyCropAction>(cx, App::apply_crop_action);
        register_global_action::<crop::NudgeCropLeft>(cx, App::nudge_crop_left);
        register_global_action::<crop::NudgeCropRight>(cx, App::nudge_crop_right);
        register_global_action::<crop::NudgeCropUp>(cx, App::nudge_crop_up);
        register_global_action::<crop::NudgeCropDown>(cx, App::nudge_crop_down);
        register_global_action::<crop::NudgeCropLeftLarge>(cx, App::nudge_crop_left_large);
        register_global_action::<crop::NudgeCropRightLarge>(cx, App::nudge_crop_right_large);
        register_global_action::<crop::NudgeCropUpLarge>(cx, App::nudge_crop_up_large);
        register_global_action::<crop::NudgeCropDownLarge>(cx, App::nudge_crop_down_large);
        register_global_action::<layers::DuplicateLayerAction>(cx, App::duplicate_layer_action);
        register_global_action::<layers::NewLayerAction>(cx, App::new_layer_action);
        register_global_action::<layers::SelectAllLayersAction>(cx, App::select_all_layers_action);
        register_global_action::<layers::StampVisibleAction>(cx, App::stamp_visible_action);
        register_global_action::<tool_settings::ToggleSnapAction>(cx, App::toggle_snap_action);
        register_global_action::<layers::MergeLayerDownAction>(cx, App::merge_layer_down_action);
        register_global_action::<layers::FlattenDocumentAction>(cx, App::flatten_document_action);
        register_global_action::<layers::StampVisibleAction>(cx, App::stamp_visible_action);
        register_global_action::<selection::SelectAllAction>(cx, App::select_all_action);
        register_global_action::<selection::DeselectAction>(cx, App::deselect_action);
        register_global_action::<selection::ToggleSelectionEditModeAction>(
            cx,
            App::toggle_selection_edit_mode_action,
        );
        register_global_action::<selection::InvertSelectionAction>(
            cx,
            App::invert_selection_action,
        );
        register_global_action::<selection::CopySelectionAction>(cx, App::copy_selection_action);
        register_global_action::<selection::CutSelectionAction>(cx, App::cut_selection_action);
        register_global_action::<selection::PasteSelectionAction>(cx, App::paste_selection_action);
        register_global_action::<selection::DeleteSelectionAction>(
            cx,
            App::delete_selection_action,
        );
        if let Some(path) = open_path {
            let result = match app.file_io.clone() {
                Some(file_io) => app.open_startup_document(&path, file_io.as_ref(), cx),
                None => app.open_startup_path(&path, cx),
            };
            if let Err(error) = result {
                eprintln!("photrez: startup document was not opened: {error}");
            }
        }
        if shot_id.is_some() && app.state.read(cx).has_document && app.active_document.is_none() {
            app.install_shot_document(cx);
        }
        // A `real-dialog-*` launch opens the live dialog through the same
        // command the user's shortcut runs, so the capture shows the body the
        // `dialog-*` fixture stubs.
        match AppState::real_dialog_shot(shot_id) {
            Some(crate::state::DialogId::NewDocument) => app.open_new_document(cx),
            Some(crate::state::DialogId::Export) => app.export_document_action(&ExportDocument, cx),
            Some(_) | None => {}
        }
        // The recovery prompt comes after the explicit open: a document the
        // user asked for by name is on screen before an offer they did not ask
        // for can cover it.
        app.prepare_startup_recovery(cx);
        app
    }
}
