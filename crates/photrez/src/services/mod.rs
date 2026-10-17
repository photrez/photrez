//! # services - external dependencies behind traits
//!
//! Owns: the platform edges (filesystem, dialogs, OS integration) behind
//! traits so `app/` can inject them and tests can substitute them.
//!
//! Must not own: leaking concrete platform APIs into `views/` or
//! `components/`, or any domain logic (`photrez-core`).
