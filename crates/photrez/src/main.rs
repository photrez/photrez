//! # photrez - the native application entry point
//!
//! A thin binary over [`photrez::run`]: the startup composition lives in the
//! library so every entry point starts the app through exactly one path.

#![windows_subsystem = "windows"]

fn main() {
    photrez::run();
}
