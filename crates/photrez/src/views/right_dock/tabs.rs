use super::*;

/// One tab bar: a 38px band with a full-width bottom border, holding
/// content-width `nav` buttons.
///
/// `active` is the id of the currently lit tab; `tabs` are the ids in order.
/// `write` is the per-dock mutation: it resolves the clicked id back to the
/// owning enum and writes it onto [`AppState`], returning whether the
/// selection changed. The label is derived from the id via [`tab_label`]
/// rather than stored beside it.
pub(super) fn tab_bar(
    active: &'static str,
    tabs: &'static [&'static str],
    write: fn(&mut AppState, &str),
    state: &Entity<AppState>,
    cx: &Context<crate::app::App>,
) -> Div {
    let bar = div()
        .w_full()
        .h(px(TAB_BAR_H))
        .flex_shrink_0()
        .flex()
        .items_center()
        .overflow_hidden()
        .border_b(px(SPLIT_BORDER))
        .border_color(color::BORDER)
        .bg(color::PANEL);

    let mut row = h_flex().h_full().min_w(px(0.0)).items_center();
    for &id in tabs {
        row = row.child(tab_button(id, id == active, write, state, cx));
    }
    bar.child(row)
}

/// Writes the inspector tab named by `id` onto the live state.
///
/// Unknown ids are ignored: the tab bar only ever passes its own ids.
pub(super) fn write_inspector_tab(state: &mut AppState, id: &str) {
    if let Some(tab) = inspector_tab_from_id(id) {
        state.inspector_tab = tab;
    }
}

/// Writes the layer tab named by `id` onto the live state.
///
/// Unknown ids are ignored: the tab bar only ever passes its own ids.
pub(super) fn write_layer_tab(state: &mut AppState, id: &str) {
    if let Some(tab) = layer_tab_from_id(id) {
        state.layer_tab = tab;
    }
}

pub(super) fn inspector_tab_from_id(id: &str) -> Option<InspectorTab> {
    match id {
        "properties" => Some(InspectorTab::Properties),
        "adjustments" => Some(InspectorTab::Adjustments),
        "presets" => Some(InspectorTab::Presets),
        _ => None,
    }
}

pub(super) fn layer_tab_from_id(id: &str) -> Option<LayerTab> {
    match id {
        "layers" => Some(LayerTab::Layers),
        "history" => Some(LayerTab::History),
        _ => None,
    }
}

/// The tab's visible label in the active locale.
pub(super) fn tab_label(id: &str) -> String {
    tab_label_for(crate::i18n::locale(), id)
}

/// The tab's visible label in `locale`: the `panels.<id>` catalog entry, or
/// the id with its first character capitalized when the catalog has no such
/// key. Both tab bars only pass their own ids, and every one of those has an
/// entry; the fallback keeps an id rendered outside the bars from showing a
/// key.
///
/// The locale is an argument rather than a process-global read so the label
/// mapping is testable without racing the app-wide locale.
pub(super) fn tab_label_for(locale: crate::i18n::Locale, id: &str) -> String {
    let key = format!("panels.{id}");
    let translated = crate::i18n::t_in(locale, &key);
    if translated != key {
        return translated;
    }
    let mut chars = id.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

pub(super) fn inspector_tab_id(tab: &InspectorTab) -> &'static str {
    match tab {
        InspectorTab::Properties => "properties",
        InspectorTab::Adjustments => "adjustments",
        InspectorTab::Presets => "presets",
    }
}

pub(super) fn layer_tab_id(tab: &LayerTab) -> &'static str {
    match tab {
        LayerTab::Layers => "layers",
        LayerTab::History => "history",
    }
}

/// A full-height, positioned button that centres 11.5px medium-weight text with
/// 12px of horizontal inset, plus the 2px ACCENT underline under it when active.
///
/// The underline is an absolutely positioned overlay on the button's bottom
/// edge and takes no layout space, so the active label centres on exactly the
/// same line as the inactive ones - the reference draws it as an absolute
/// after-pseudo for the same reason. Stacking the label above an in-flow 2px
/// bar centres the active label a pixel high, which a mounted test pins.
///
/// The click handler writes the clicked tab id back onto the live state
/// through `write` and notifies the owning root, so the next frame lights the
/// new tab and swaps the panel behind it.
///
/// Returns `impl IntoElement` rather than a concrete type: `.test_support()`
/// wraps the element in the toolkit's observer in test builds and returns it
/// unchanged otherwise, so the concrete type differs per build. `id` is
/// `'static` because the tab ids are all string literals.
fn tab_button(
    id: &'static str,
    active: bool,
    write: fn(&mut AppState, &str),
    state: &Entity<AppState>,
    cx: &Context<crate::app::App>,
) -> impl IntoElement {
    let label = tab_label(id);
    // Each button captures its own copy of the state handle; the handler runs
    // on the owning App and re-enters the state entity through it.
    let state = state.clone();
    let on_click = cx.listener(move |_, _, _, cx: &mut Context<crate::app::App>| {
        state.update(cx, |s, _| write(s, id));
        cx.notify();
    });
    let mut btn = h_flex()
        .relative()
        .h_full()
        .flex_shrink_0()
        .id(id)
        .test_support()
        .on_click(on_click)
        .child(
            h_flex()
                .id(format!("{id}-label"))
                .test_support()
                .px(px(TAB_PAD_X))
                .text_size(px(TAB_TEXT))
                .font_family(ui::FONT_UI)
                .font_weight(FontWeight(500.))
                .text_color(if active { color::TEXT } else { color::TEXT_DIM })
                .child(label),
        );
    if active {
        btn = btn.child(
            div()
                .absolute()
                .bottom(px(0.))
                .left(px(0.))
                .right(px(0.))
                .h(px(UNDERLINE_H))
                .bg(color::ACCENT),
        );
    } else {
        // The 2%-white hover fill and the plain-text hover colour are both
        // painted effects, not tokens. No token holds 2% white; use hsla inline.
        btn = btn.hover(|s| s.bg(hsla(0.0, 0.0, 1.0, 0.02)).text_color(color::TEXT));
    }
    btn
}
