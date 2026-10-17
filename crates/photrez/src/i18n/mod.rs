//! # i18n - locale entity and message catalog
//!
//! Owns: the locale entity and the message catalog that resolves keys to
//! user-visible strings.
//!
//! Must not own: user-visible strings hardcoded anywhere else in the crate -
//! every text the user reads goes through this catalog.
