//! Locale preference: apply the startup detection order and persist changes
//! from the Settings dialog.

use gpui_kit::Context;

use crate::i18n::{self, Locale};
use crate::services::settings::SettingsStore;

use super::App;

impl App {
    /// The startup detection order applied to the store this app owns. Split
    /// out so the resolution can be asserted without reading the process-wide
    /// locale, which a concurrently mounted app also writes.
    pub(crate) fn startup_locale(&self) -> Locale {
        let store = self
            .settings
            .as_ref()
            .map(|settings| settings as &dyn SettingsStore);
        i18n::initial_locale(store)
    }

    pub(crate) fn apply_startup_locale(&mut self) {
        i18n::set_locale(self.startup_locale());
    }

    pub(crate) fn set_locale(&mut self, locale: Locale, cx: &mut Context<Self>) {
        i18n::set_locale(locale);
        if let Some(settings) = &self.settings {
            settings.write(i18n::LOCALE_KEY, locale.code());
        }
        cx.notify();
    }

    /// The Settings language dropdown (`LanguageSwitcher`). The list expands
    /// inline: a deferred popup inside a dialog overlay never receives hits.
    pub(crate) fn toggle_settings_language(&mut self, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _| {
            state.settings_language_open = !state.settings_language_open;
        });
        cx.notify();
    }

    pub(crate) fn choose_settings_language(&mut self, locale: Locale, cx: &mut Context<Self>) {
        self.set_locale(locale, cx);
        self.state.update(cx, |state, _| {
            state.settings_language_open = false;
        });
        cx.notify();
    }

    pub(crate) fn close_settings(&mut self, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _| {
            state.active_dialog = None;
            state.settings_language_open = false;
        });
        cx.notify();
    }
}
