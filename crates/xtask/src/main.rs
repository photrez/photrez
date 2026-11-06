//! Workspace automation for the Photrez editor.
//!
//! Commands:
//!
//! ```text
//! xtask dev [-- <app args>]
//!     Build and run the app with the gnu linker directory on the child PATH
//!     (the link chain needs it for ld's DLLs and for windres' bare gcc; plain
//!     `cargo run` works when the directory is already on PATH).
//!
//! xtask set-version <semver>
//!     Rewrite the app crate version in one place and confirm it through
//!     cargo metadata. A lower release number is refused, and a refused
//!     read-back restores the manifest bytes it replaced.
//! ```
//!
//! Both commands act on this repository only: they read the manifests they
//! change and write nothing outside the working tree.

mod dev;
mod version;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match args.first().map(String::as_str) {
        None | Some("-h") | Some("--help") => {
            print_help();
            0
        }
        Some("dev") => dev::run(&args[1..]),
        Some("set-version") => version::cmd_set_version(&args[1..]),
        Some(other) => {
            eprintln!("xtask: unknown command `{other}`\n");
            print_help();
            2
        }
    };
    std::process::exit(code);
}

fn print_help() {
    println!(
        "xtask - workspace automation for the Photrez editor\n\
         \n\
         USAGE:\n\
         \x20 xtask dev [-- <app args>]\n\
         \x20     Build and run the app with the gnu linker directory on the\n\
         \x20     child PATH.\n\
         \x20 xtask set-version <semver>\n\
         \x20     Rewrite the app crate version in one place and confirm it\n\
         \x20     through cargo metadata. A lower release number is refused,\n\
         \x20     and a refused read-back restores the manifest bytes it\n\
         \x20     replaced.\n\
         \n\
         See `cargo dev` and `cargo ui` in the root Cargo.toml for the aliases\n\
         the development loop uses."
    );
}
