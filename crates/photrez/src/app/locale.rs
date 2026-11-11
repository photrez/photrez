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
        // Many test apps mount in parallel, and the locale tests serialize the
        // process-wide locale through `TEST_LOCALE_LOCK`. A test mount must not
        // reset it; production windows apply their stored preference.
        #[cfg(not(test))]
        i18n::set_locale(self.startup_locale());
    }

    pub(crate) fn set_locale(&mut self, locale: Locale, cx: &mut Context<Self>) {
        i18n::set_locale(locale);
        if let Some(settings) = &self.settings {
            settings.write(i18n::LOCALE_KEY, locale.code());
        }
        cx.notify();
    }

    /// The Settings language dropdown (`LanguageSwitcher`). The list floats
    /// over the dialog from the shell's shared dropdown host; the
    /// inline-expansion predecessor never received hits as a popup.
    pub(crate) fn choose_settings_language(&mut self, locale: Locale, cx: &mut Context<Self>) {
        self.set_locale(locale, cx);
        self.state.update(cx, |state, _| {
            state.open_select = None;
        });
        cx.notify();
    }

    pub(crate) fn close_settings(&mut self, cx: &mut Context<Self>) {
        self.state.update(cx, |state, _| {
            state.active_dialog = None;
            state.open_select = None;
        });
        cx.notify();
    }
}
