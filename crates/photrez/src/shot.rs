//! Shot selection from `--shot <id>`.
//!
//! A capture harness launches the app once per named UI state and screenshots
//! it. The `--shot <id>` CLI argument (parsed in `main.rs`) selects which state
//! to render; with no argument the app renders its default state.
//!
//! # Why a module-level static rather than GPUI state
//!
//! The idiomatic answer would be a `Global` in the app context, but every
//! component's `render()` here takes no `cx` - the ports are plain
//! `fn render() -> impl IntoElement`. Threading a context through all of them
//! to carry one startup-once value would cost that, and the value is fixed for
//! the process's lifetime. A `OnceLock` costs one lookup and no allocation
//! when unset.
//!
//! Components ask for what they care about by id (`shot::is("dock-history")`)
//! or by family (`shot::starts_with("menu-")`); nothing here knows what a shot
//! *means*, because adding a state should not require editing this file.

#![allow(dead_code)] // `is`/`starts_with` land before the modules that call them; the blanket drops as consumers arrive

use std::sync::OnceLock;

/// The shot id this process was asked to render, or `None` when unset -
/// meaning "render the default state", which is what every launch without
/// `--shot` does.
static REQUESTED: OnceLock<Option<String>> = OnceLock::new();

/// Records the requested shot id. A subsequent call is silently ignored so
/// the first selector wins; this value is fixed for the process lifetime.
pub fn set_active(id: String) {
    let _ = REQUESTED.set(Some(id));
}

/// Returns the shot id this process was asked to render, or `None` when no
/// shot was selected.
///
/// The backing `Option<String>` lives in a static `OnceLock` and is never
/// replaced, so the returned `&'static str` is safe to hold without cloning.
pub fn active() -> Option<&'static str> {
    REQUESTED.get().and_then(|v| v.as_deref())
}

/// Whether this process was asked to render exactly `id`.
///
/// Comparing the raw ids keeps call sites readable at the point of use -
/// `if shot::is("dock-history")` says what it means next to the branch it
/// controls, where a parsed enum would have to be threaded down to it.
pub fn is(id: &str) -> bool {
    active() == Some(id)
}

/// Whether the requested id begins with `prefix` - for families where the
/// member matters but the suffix does not, such as `menu-file`, `menu-edit`.
///
/// An empty prefix would match everything, which is never intended, so it
/// returns false rather than degenerating into a no-op filter.
pub fn starts_with(prefix: &str) -> bool {
    !prefix.is_empty() && matches(active(), prefix)
}

/// Core matching logic: returns true when `active` is `Some` and starts with
/// `query`. Exact match is a degenerate case - a string is a prefix of itself.
fn matches(active: Option<&str>, query: &str) -> bool {
    active.is_some_and(|a| a.starts_with(query))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_exact() {
        assert!(matches(Some("dock-history"), "dock-history"));
    }

    #[test]
    fn matches_prefix() {
        assert!(matches(Some("menu-file"), "menu-"));
    }

    #[test]
    fn prefix_matches_multiple_ids() {
        assert!(matches(Some("menu-file"), "menu-"));
        assert!(matches(Some("menu-edit"), "menu-"));
    }

    #[test]
    fn matches_none_is_false() {
        assert!(!matches(None, "anything"));
        assert!(!matches(None, ""));
    }

    #[test]
    fn matches_empty_query_matches_nonempty() {
        assert!(matches(Some("dock-history"), ""));
    }

    #[test]
    fn shot_semantics() {
        set_active("dock-history".to_string());
        // A second set is ignored.
        set_active("should-not-stick".to_string());
        assert_eq!(active(), Some("dock-history"));
        // is() is exact: matches the full id, not a prefix.
        assert!(is("dock-history"));
        assert!(!is("dock-"));
        assert!(!is("should-not-stick"));
        // starts_with() is prefix-based.
        assert!(starts_with("dock-"));
        assert!(!starts_with("should"));
        assert!(!starts_with(""));
    }
}
