//! Locale preference: apply the startup detection order and persist changes
//! from the Settings dialog.

use gpui_kit::Context;

use crate::i18n::{self, Locale};
use crate::services::settings::SettingsStore;

use super::App;

impl App {
    pub(crate) fn apply_startup_locale(&mut self) {
        let store = self
            .settings
            .as_ref()
            .map(|settings| settings as &dyn SettingsStore);
        i18n::set_locale(i18n::initial_locale(store));
    }

    pub(crate) fn set_locale(&mut self, locale: Locale, cx: &mut Context<Self>) {
        i18n::set_locale(locale);
        if let Some(settings) = &self.settings {
            settings.write(i18n::LOCALE_KEY, locale.code());
        }
        cx.notify();
    }
}
