//! Apply Adjustment: bake the non-destructive adjustment riding a layer into
//! its pixels.

use gpui_kit::Context;

use photrez_core::protocol::{Command, RasterizePixels};

use super::App;

impl App {
    /// Bake the one selected layer's basic adjustment into its raster store.
    /// The panel and the layer context menu reach this. A layer whose
    /// `basicAdjustment` object is absent is a silent no-op, matching the
    /// reference editor's bake behavior for the flag-without-object case.
    pub(crate) fn apply_basic_adjustment(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        let (doc_id, layer) = self.selected_layer(cx)?;
        let Some(adjustment) = layer.basic_adjustment.clone() else {
            return Ok(());
        };
        let all_zero = adjustment.brightness == 0.0
            && adjustment.contrast == 0.0
            && adjustment.saturation == 0.0;
        let pixels = if all_zero {
            None
        } else {
            let canonical = self
                .sessions
                .get(&doc_id)
                .and_then(|session| {
                    session
                        .loaded_document()
                        .registry
                        .get_layer(&doc_id, &layer.id)
                })
                .ok_or_else(|| "the selected layer has no raster store".to_string())?;
            Some(RasterizePixels {
                width: f64::from(canonical.width),
                height: f64::from(canonical.height),
                rgba: photrez_core::raster::kernel::apply_basic_adjustment(
                    &canonical.pixels,
                    adjustment.brightness,
                    adjustment.contrast,
                    adjustment.saturation,
                ),
            })
        };
        self.apply_core_command(
            &doc_id,
            Command::ApplyBasicAdjustment {
                id: layer.id,
                pixels,
            },
            true,
            cx,
        )
    }

    pub(crate) fn apply_basic_adjustment_for_view(&mut self, cx: &mut Context<Self>) {
        let result = self.apply_basic_adjustment(cx);
        self.report_layer_op_for_view(result, cx);
    }
}
