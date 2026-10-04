// SPDX-License-Identifier: AGPL-3.0-or-later
// Shared `.ptz` `document.json` fixtures, used by both the core writer tests
// (`ptz_document.rs`) and the Tauri command tests (`save_stream.rs`).
//
// ONE definition, imported by the core writer tests, the canonical model reader
// tests, and the desktop command tests, so none of them can drift apart. Every
// consumer here is `#[cfg(test)]`; `crates/core/Cargo.toml` keeps the module out
// of the shipped library behind the `ptz-test-fixtures` feature.

/// A real `document.json` dumped byte-for-byte from a running build
/// (`photrez-ptz` v3). Two raster layers. This is the regression fixture: it is
/// the shape the writer must keep producing for files already on disk.
pub const REAL_DUMPED_MODEL: &str = r##"{"id":"dump-1788754352188","name":"Dump","width":300,"height":200,"activeLayerId":"layer-ryyklho2","selection":{"x":10,"y":10,"width":50,"height":40,"angle":0,"shape":"rect","inverted":null},"viewport":{"panX":40,"panY":63.599995930989564,"zoom":2.6840000406901043,"rotation":0},"dirty":true,"layers":[{"id":"layer-ryyklho2","name":"Painted","type":"raster","visible":true,"opacity":0.75,"locked":true,"lockTransparency":true,"hasAdjustments":false,"baseImageBitmap":null,"blendMode":"multiply","transform":{"x":0,"y":0,"scaleX":1,"scaleY":1,"rotation":0,"flipH":false,"flipV":false},"width":300,"height":200,"imageBitmap":null},{"id":"layer-jhgyjahw","name":"Background","type":"raster","visible":true,"opacity":1,"locked":false,"isBackground":true,"lockPosition":true,"lockRotation":true,"hasAdjustments":false,"baseImageBitmap":null,"blendMode":"normal","transform":{"x":0,"y":0,"scaleX":1,"scaleY":1,"rotation":0,"flipH":false,"flipV":false},"width":300,"height":200,"imageBitmap":null}],"format":"photrez-ptz","version":3}"##;

/// A document carrying one layer of EVERY persisted content kind: raster with a
/// basic adjustment, shape, text, and adjustment.
///
/// The raster-only `REAL_DUMPED_MODEL` cannot prove the nested payloads
/// (`shapeParams`, `textData`, `basicAdjustment`) survive, because it does not
/// contain them — a writer that silently dropped `textData` passed every test
/// against it. Every value below is asserted literally in
/// `ptz_document.rs::nested_layer_payloads_survive_the_round_trip`.
pub const MIXED_LAYER_TYPES_MODEL: &str = r##"{"id":"mixed-1","name":"Mixed","width":800,"height":600,"activeLayerId":"t-1","selection":{"x":5,"y":6,"width":7,"height":8,"angle":9,"shape":"ellipse","inverted":true},"viewport":{"panX":1.5,"panY":2.5,"zoom":3.5,"rotation":4.5},"dirty":false,"layers":[{"id":"a-1","name":"Adjusted","type":"raster","visible":true,"opacity":0.5,"locked":false,"isBackground":true,"lockTransparency":true,"lockPosition":true,"lockRotation":true,"hasAdjustments":true,"basicAdjustment":{"brightness":12.5,"contrast":-33.25,"saturation":44.75},"baseImageBitmap":null,"blendMode":"color-dodge","transform":{"x":1.5,"y":-2.5,"scaleX":3.5,"scaleY":4.5,"rotation":5.5,"flipH":true,"flipV":true},"width":640,"height":480,"imageBitmap":null,"resourceId":null},{"id":"s-1","name":"Star","type":"shape","visible":false,"opacity":0.25,"locked":true,"isBackground":false,"lockTransparency":false,"lockPosition":false,"lockRotation":false,"hasAdjustments":false,"basicAdjustment":null,"baseImageBitmap":null,"blendMode":"soft-light","transform":{"x":-7.25,"y":8.5,"scaleX":1.25,"scaleY":0.75,"rotation":-45.5,"flipH":true,"flipV":false},"width":200,"height":150,"imageBitmap":null,"shapeParams":{"kind":"star","width":200,"height":150,"radius":11.5,"fill":{"kind":"solid","color":"#E15A17"},"stroke":{"enabled":true,"color":"#00FF00","width":3.5},"arrowHead":false},"textData":null},{"id":"t-1","name":"Title","type":"text","visible":true,"opacity":0.75,"locked":false,"isBackground":false,"lockTransparency":false,"lockPosition":false,"lockRotation":false,"hasAdjustments":false,"basicAdjustment":null,"baseImageBitmap":null,"blendMode":"hard-light","transform":{"x":2.5,"y":3.5,"scaleX":1,"scaleY":1,"rotation":0,"flipH":false,"flipV":false},"width":300,"height":40,"imageBitmap":null,"shapeParams":null,"textData":{"content":"Hello","fontFamily":"Inter","fontSize":72,"fontWeight":700,"fontStyle":"italic","color":"#123456","align":"center","lineHeight":2.5,"letterSpacing":3.5,"boxMode":"area","boxWidth":320,"boxHeight":44,"stroke":{"width":2.5,"color":"#ABCDEF","align":"inside"},"underline":true,"strikethrough":false,"uppercase":true}},{"id":"g-1","name":"Grade","type":"adjustment","visible":true,"opacity":1,"locked":false,"isBackground":false,"lockTransparency":false,"lockPosition":false,"lockRotation":false,"hasAdjustments":true,"basicAdjustment":{"brightness":-1.5,"contrast":2.5,"saturation":-3.5},"baseImageBitmap":null,"blendMode":"exclusion","transform":{"x":0,"y":0,"scaleX":1,"scaleY":1,"rotation":0,"flipH":false,"flipV":false},"width":800,"height":600,"imageBitmap":null,"shapeParams":null,"textData":null}],"format":"photrez-ptz","version":3}"##;

/// A text layer written by an OLDER build: `textData` is missing every field
/// added after the first text-layer release (`letterSpacing`, `boxMode`,
/// `boxWidth`, `boxHeight`, `stroke`, `underline`, `strikethrough`, `uppercase`).
///
/// This is the regression fixture for the strictness hazard: the loader never
/// normalises `textData` (`editorOpenImage.ts` -> `restoreSnapshot` raw-spreads
/// it), so such a file loads and renders correctly and then reaches the save
/// path with those fields genuinely absent. With no serde defaults on
/// `TextData` the save returned `E_VALIDATION` and wrote nothing.
pub const LEGACY_TEXT_LAYER_MODEL: &str = r##"{"id":"legacy-text","name":"Legacy","width":400,"height":300,"activeLayerId":null,"selection":null,"viewport":{"panX":0,"panY":0,"zoom":1,"rotation":0},"dirty":false,"layers":[{"id":"t-old","name":"Old Text","type":"text","visible":true,"opacity":1,"locked":false,"blendMode":"normal","transform":{"x":0,"y":0,"scaleX":1,"scaleY":1,"rotation":0,"flipH":false,"flipV":false},"width":200,"height":30,"textData":{"content":"legacy","fontFamily":"Arial","fontSize":36,"fontWeight":400,"fontStyle":"normal","color":"#000000","align":"left","lineHeight":1.2}}],"format":"photrez-ptz","version":3}"##;
