//! System text clipboard adapter and the existing eyedropper auto-copy setting.

use crate::services::settings::SettingsStore;
use gpui_kit::{App, ClipboardItem};

const AUTO_COPY_HEX_KEY: &str = "photrez_eyedropper_autocopy";

/// The external boundary for writing text to the system clipboard.
pub trait Clipboard: Send + Sync {
    fn write_text(&self, app: &App, text: &str);
}

/// GPUI-backed clipboard writer.
#[derive(Clone, Copy, Debug, Default)]
pub struct GpuiClipboard;

impl Clipboard for GpuiClipboard {
    fn write_text(&self, app: &App, text: &str) {
        app.write_to_clipboard(ClipboardItem::new_string(text.to_owned()));
    }
}

pub fn auto_copy_hex_enabled(settings: &dyn SettingsStore) -> bool {
    settings.read(AUTO_COPY_HEX_KEY).as_deref() == Some("true")
}

pub fn set_auto_copy_hex_enabled(settings: &dyn SettingsStore, enabled: bool) {
    settings.write(AUTO_COPY_HEX_KEY, if enabled { "true" } else { "false" });
}

#[cfg(test)]
mod tests {
    use super::{auto_copy_hex_enabled, set_auto_copy_hex_enabled, Clipboard, GpuiClipboard};
    use crate::services::settings::{FileSettings, SettingsStore};
    use gpui_kit::{ClipboardItem, TestAppContext};
    use std::path::PathBuf;

    fn settings_path(name: &str) -> PathBuf {
        let directory = std::env::temp_dir().join(format!(
            "photrez-clipboard-test-{}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&directory);
        directory.join("settings.json")
    }

    #[gpui_kit::test]
    fn gpui_adapter_writes_exact_foreground_hex(cx: &mut TestAppContext) {
        let text = "#12aBef";
        cx.update(|app| GpuiClipboard.write_text(app, text));

        assert_eq!(
            cx.read_from_clipboard(),
            Some(ClipboardItem::new_string(text.to_owned()))
        );
    }

    #[test]
    fn auto_copy_preference_defaults_off_and_persists_both_values() {
        let path = settings_path("persist");
        let settings = FileSettings::at(&path);
        assert!(!auto_copy_hex_enabled(&settings));

        set_auto_copy_hex_enabled(&settings, true);
        assert!(auto_copy_hex_enabled(&FileSettings::at(&path)));
        assert_eq!(
            settings.read("photrez_eyedropper_autocopy").as_deref(),
            Some("true")
        );

        set_auto_copy_hex_enabled(&settings, false);
        assert!(!auto_copy_hex_enabled(&FileSettings::at(&path)));
        assert_eq!(
            settings.read("photrez_eyedropper_autocopy").as_deref(),
            Some("false")
        );

        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn only_exact_true_enables_auto_copy_preference() {
        let path = settings_path("truthy");
        let settings = FileSettings::at(&path);

        for value in ["false", "True", "1", ""] {
            settings.write("photrez_eyedropper_autocopy", value);
            assert!(
                !auto_copy_hex_enabled(&settings),
                "value {value:?} must stay off"
            );
        }

        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }
}
