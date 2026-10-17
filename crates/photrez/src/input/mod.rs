//! # input - pointer and brush chain
//!
//! Owns: pointer/brush input policy - turning window events into tool actions
//! and `photrez-core` commands.
//!
//! Must not own: painting (`render/`), widget layout (`views/`), or document
//! mutation outside core commands.
