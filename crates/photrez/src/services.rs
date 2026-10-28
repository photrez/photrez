//! # services - external dependencies behind traits
//!
//! Owns: the platform edges (filesystem, dialogs, OS integration) behind
//! traits so `app/` can inject them and tests can substitute them.
//!
//! Must not own: leaking concrete platform APIs into `views/` or
//! `components/`, or any domain logic (`photrez-core`).

#[allow(dead_code)]
pub(crate) mod autosave;
pub mod clipboard;
pub mod document;
pub mod error;
pub mod file_io;
#[allow(dead_code)]
pub(crate) mod fonts;
pub(crate) mod image_files;
#[allow(dead_code)]
pub(crate) mod print;
pub mod ptz_limits;
pub mod save_stream;
#[allow(dead_code)]
pub(crate) mod session;
pub mod settings;
pub mod window_state;

#[cfg(test)]
mod invalid_input;
