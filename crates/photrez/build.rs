//! The `i18n!` macro reads the locale files while the crate compiles, and it
//! does not tell Cargo that it read them. Without this script, a change in a
//! catalog file alone leaves the old text in the binary.

fn main() {
    // Cargo scans the whole directory tree, so one line covers every locale
    // file, now and later.
    println!("cargo:rerun-if-changed=locales");
}
