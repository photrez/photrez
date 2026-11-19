//! What each tool writes when a gesture commits: selection, move, shape, text,
//! gradient, bucket and eyedropper, through the one region helper.

use super::*;

impl App {
    pub(super) fn apply_selection(
        &mut self,
        rect: (f64, f64, f64, f64),
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let (x, y, width, height) = rect;
        let shape = match self.state.read(cx).selection_shape {
            SelectionShape::Rect => CoreSelectionShape::Rect,
            SelectionShape::Ellipse => CoreSelectionShape::Ellipse,
        };
        let selection = SelectionState {
            x,
            y,
            width,
            height,
            angle: 0.0,
            shape: Some(shape),
            inverted: None,
        };
        let doc_id = self.active_doc_id()?;
        self.apply_core_command(&doc_id, Command::SetSelection { selection }, false, cx)
    }

    pub(super) fn apply_move(
        &mut self,
        start: (f64, f64),
        end: (f64, f64),
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let (id, x, y, scale_x, scale_y, rotation, flip_h, flip_v) =
            self.active_layer_transform(cx)?;
        let transform = TransformPatch {
            x: x + end.0 - start.0,
            y: y + end.1 - start.1,
            scale_x,
            scale_y,
            rotation,
            flip_h: Some(flip_h),
            flip_v: Some(flip_v),
        };
        let doc_id = self.active_doc_id()?;
        self.apply_core_command(&doc_id, Command::TransformLayer { id, transform }, true, cx)
    }

    pub(super) fn apply_shape(
        &mut self,
        start: (f64, f64),
        end: (f64, f64),
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let (x, y, width, height) = normalized_rect(start, end);
        let (doc_w, doc_h) = self.active_document_size()?;
        let (
            kind,
            fill_enabled,
            stroke_enabled,
            stroke_width,
            stroke_color,
            radius,
            arrow_head,
            color,
        ) = {
            let state = self.state.read(cx);
            (
                shape_tool_kind(state.shape_kind),
                state.shape_fill,
                state.shape_stroke,
                state.shape_stroke_width,
                hex_color(state.shape_stroke_color),
                state.shape_radius,
                state.shape_arrow_head,
                hex_color(state.fg_color),
            )
        };
        let params = ShapeParams {
            kind,
            width,
            height,
            radius: radius.clamp(0.0, width.min(height) / 2.0),
            fill: ShapeFill {
                kind: if fill_enabled {
                    ShapeFillKind::Solid
                } else {
                    ShapeFillKind::None
                },
                color,
            },
            stroke: ShapeStroke {
                enabled: stroke_enabled,
                color: stroke_color,
                width: stroke_width.max(1.0),
            },
            arrow_head,
        };
        let id = next_layer_id();
        let doc_id = self.active_doc_id()?;
        let result = self.apply_core_command(
            &doc_id,
            Command::AddLayer {
                id: id.clone(),
                name: "Shape".to_string(), // i18n-exempt: layer name set by the shape tool
                width: doc_w,
                height: doc_h,
                index: 0,
                x: Some(x),
                y: Some(y),
                layer_type: Some(LayerType::Shape),
                shape_params: Some(params),
                text_data: None,
            },
            true,
            cx,
        );
        if result.is_ok() {
            // The shape tool selects the layer it just created; reproject with the
            // new core id so the Properties/Inspector fields follow it too.
            self.project_layers_with_selection(Some(id.as_str()), cx);
        }
        result
    }

    pub(super) fn apply_text(
        &mut self,
        at: (f64, f64),
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let (doc_w, doc_h) = self.active_document_size()?;
        let (color, font_size, font_weight, italic, align, stroke) = {
            let state = self.state.read(cx);
            (
                hex_color(state.fg_color),
                state.text_font_size,
                state.text_font_weight,
                state.text_italic,
                state.text_align,
                photrez_core::document::layer_params::TextStroke {
                    width: state.text_stroke_width,
                    color: crate::palette::to_hex(state.text_stroke_color),
                    align: Some(state.text_stroke_align),
                },
            )
        };
        let data = TextData {
            content: String::new(),
            font_family: "Arial".to_string(), // i18n-exempt: technical token
            font_size,
            font_weight,
            font_style: if italic {
                photrez_core::document::layer_params::TextFontStyle::Italic
            } else {
                photrez_core::document::layer_params::TextFontStyle::Normal
            },
            align: match align {
                crate::state::TextAlignTool::Left => {
                    photrez_core::document::layer_params::TextAlign::Left
                }
                crate::state::TextAlignTool::Center => {
                    photrez_core::document::layer_params::TextAlign::Center
                }
                crate::state::TextAlignTool::Right => {
                    photrez_core::document::layer_params::TextAlign::Right
                }
            },
            color,
            stroke,
            ..Default::default()
        };
        let id = next_layer_id();
        let edit_id = id.clone();
        let doc_id = self.active_doc_id()?;
        self.apply_core_command(
            &doc_id,
            Command::AddLayer {
                id,
                name: format!("Text at {:.0},{:.0}", at.0, at.1),
                width: doc_w,
                height: doc_h,
                index: 0,
                x: Some(at.0),
                y: Some(at.1),
                layer_type: Some(LayerType::Text),
                shape_params: None,
                text_data: Some(data.clone()),
            },
            true,
            cx,
        )?;
        self.begin_text_edit(edit_id, at.0, at.1, data, cx);
        Ok(())
    }

    pub(super) fn selection_fill_mask(
        &self,
        cx: &Context<Self>,
    ) -> (bool, i32, i32, i32, i32, u32, bool) {
        let Some(doc_id) = self.active_document.as_ref().map(|doc| doc.doc_id.clone()) else {
            return (false, 0, 0, 0, 0, 0, false);
        };
        let Some(selection) = self
            .sessions
            .get(&doc_id)
            .and_then(|session| session.loaded_document().content_snapshot())
            .and_then(|content| content.selection.clone())
        else {
            return (false, 0, 0, 0, 0, 0, false);
        };
        let _ = cx;
        let shape = match selection.shape {
            Some(CoreSelectionShape::Ellipse) => 1,
            _ => 0,
        };
        (
            true,
            selection.x.round() as i32,
            selection.y.round() as i32,
            selection.width.round() as i32,
            selection.height.round() as i32,
            shape,
            selection.inverted.unwrap_or(false),
        )
    }

    pub(super) fn apply_gradient(
        &mut self,
        start: (f64, f64),
        end: (f64, f64),
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let target = self
            .active_paint_target(false, cx)
            .map_err(|error| format!("Gradient: {error}"))?;
        let (fg, bg, radial, reverse, fg_transparent) = {
            let state = self.state.read(cx);
            (
                state.fg_color,
                state.bg_color,
                state.gradient_radial,
                state.gradient_reverse,
                state.gradient_fg_transparent,
            )
        };
        let (from, to) = if fg_transparent {
            (fg, [fg[0], fg[1], fg[2], 0])
        } else if reverse {
            (bg, fg)
        } else {
            (fg, bg)
        };
        let (has_mask, mx, my, mw, mh, shape, inverted) = self.selection_fill_mask(cx);
        let filled = photrez_core::raster::kernel::gradient_fill(
            &target.base_pixels,
            target.width,
            target.height,
            u32::from(radial),
            start.0,
            start.1,
            end.0,
            end.1,
            &[0.0, 1.0],
            &[
                from[0], from[1], from[2], from[3], to[0], to[1], to[2], to[3],
            ],
            has_mask,
            mx,
            my,
            mw,
            mh,
            shape,
            inverted,
        );
        self.apply_pixel_region(target, filled, "Gradient", cx)
    }

    pub(super) fn fill_bucket(&mut self, x: f64, y: f64, cx: &mut Context<Self>) {
        let result = (|| {
            let target = self
                .active_paint_target(false, cx)
                .map_err(|error| format!("Fill: {error}"))?;
            let (color, tolerance, contiguous) = {
                let state = self.state.read(cx);
                (
                    state.fg_color,
                    state.fill_tolerance.min(255),
                    state.fill_contiguous,
                )
            };
            let (has_mask, mx, my, mw, mh, shape, inverted) = self.selection_fill_mask(cx);
            let sx = (x.max(0.0) as u32).min(target.width.saturating_sub(1));
            let sy = (y.max(0.0) as u32).min(target.height.saturating_sub(1));
            let filled = photrez_core::raster::kernel::flood_fill(
                &target.base_pixels,
                target.width,
                target.height,
                sx,
                sy,
                color[0] as u32,
                color[1] as u32,
                color[2] as u32,
                color[3] as u32,
                tolerance,
                has_mask,
                mx,
                my,
                mw,
                mh,
                shape,
                inverted,
                contiguous,
            );
            self.apply_pixel_region(target, filled, "Fill", cx)
        })();
        if let Err(error) = result {
            self.set_status_error(Some(error), cx);
        }
    }

    pub(super) fn sample_eyedropper(&mut self, x: f64, y: f64, cx: &mut Context<Self>) {
        let result = (|| {
            let doc_id = self.active_doc_id()?;
            let (doc_w, doc_h) = self.active_document_size()?;
            let px = (x.max(0.0) as u32).min((doc_w as u32).saturating_sub(1));
            let py = (y.max(0.0) as u32).min((doc_h as u32).saturating_sub(1));
            let composite = {
                let loaded = self
                    .sessions
                    .get(&doc_id)
                    .ok_or_else(|| "active session is missing".to_string())?
                    .loaded_document();
                let content = loaded
                    .content_snapshot()
                    .ok_or_else(|| "loaded content is missing".to_string())?;
                photrez_core::render::compositor::composite_layers_in_region_with_font_data(
                    content,
                    &loaded.registry,
                    Some(crate::INTER),
                    photrez_core::render::compositor::CompositeRegion {
                        x: px,
                        y: py,
                        width: 1,
                        height: 1,
                    },
                )
                .map_err(|error| error.to_string())?
            };
            if composite.region.width == 0 || composite.region.height == 0 {
                return Err("the eyedropper is outside the document".to_string());
            }
            let rgba = &composite.pixels[0..4];
            let color = [rgba[0], rgba[1], rgba[2], rgba[3]];
            let auto_copy = self.state.read(cx).auto_copy_hex;
            self.state.update(cx, |state, _| state.fg_color = color);
            if auto_copy {
                cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string(hex_color(color)));
            }
            Ok(())
        })();
        if let Err(error) = result {
            self.set_status_error(Some(error), cx);
        } else {
            self.set_status_error(None, cx);
        }
        cx.notify();
    }

    pub(super) fn apply_pixel_region(
        &mut self,
        target: PaintTarget,
        filled: Vec<u8>,
        label: &str,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        if filled == target.base_pixels {
            return Ok(());
        }
        let PaintTarget {
            doc_id,
            layer_id,
            width,
            height,
            ..
        } = target;
        let committed = {
            let session = self
                .sessions
                .get_mut(&doc_id)
                .ok_or_else(|| "active session is missing".to_string())?;
            session.loaded_document_mut().registry.write_region(
                &doc_id,
                &layer_id,
                0,
                0,
                width as usize,
                height as usize,
                filled,
            )
        };
        if committed.is_none() {
            return Err("pixel region write was refused".to_string());
        }
        self.sessions
            .mark_history_dirty(&doc_id)
            .map_err(|error| format!("dirty state failed: {error:?}"))?;
        if let Some(session) = self.sessions.get_mut(&doc_id) {
            session
                .loaded_document_mut()
                .registry
                .set_history_label(&doc_id, label);
        }
        self.refresh_after_edit(cx)
    }
}
