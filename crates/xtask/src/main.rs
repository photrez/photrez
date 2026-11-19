//! Workspace automation for the Photrez editor.
//!
//! Commands:
//!
//! ```text
//! xtask doctor
//!     Print rustc, cargo, bacon and linker availability, no writes
//!
//! xtask dev [-- <app args>]
//!     Build and run the app with the gnu linker directory on the child PATH
//!     (the link chain needs it for ld's DLLs and for windres' bare gcc; plain
//!     `cargo run` works when the directory is already on PATH).
//!
//! xtask dev-ui [-- <app args>]
//!     Same as dev, with the dev-ui profile (app crate at opt-level 1).
//!
//! xtask mingw-dir
//!     Print the MinGW directory the windows-gnu toolchain needs.
//!
//! xtask watch [bacon job]
//!     Rebuild and restart on save through bacon (default job: run).
//!
//! xtask ci
//!     Run the workspace gate: format, clippy, core and workspace tests, docs.
//!
//! xtask set-version <semver>
//!     Rewrite the app crate version in one place and confirm it through
//!     cargo metadata. A lower release number is refused, and a refused
//!     read-back restores the manifest bytes it replaced.
//! ```
//!
//! `ci` wraps the same check, clippy, test and doc commands the runner uses.
//! The app aliases in
//! `.cargo/config.toml` (`cargo dev`, `cargo ui`) and these subcommands are
//! the entry points for humans and agents alike; they read the manifests they
//! change and write nothing outside the working tree.

mod ci;
mod dev;
mod run;
mod version;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match args.first().map(String::as_str) {
        None | Some("-h") | Some("--help") => {
            print_help();
            0
        }
        Some("ci") => ci::run_ci(),
        Some("doctor") => run::doctor(),
        Some("mingw-dir") => dev::print_mingw_dir(),
        Some("watch") => run::watch(&args[1..]),
        Some("dev") => dev::run(&args[1..]),
        Some("dev-ui") => dev::run_ui(&args[1..]),
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
         \x20 xtask doctor\n\
         \x20     Print toolchain, bacon and linker availability.\n\
         \x20 xtask dev [-- <app args>]\n\
         \x20     Build and run the app with the gnu linker directory on the\n\
         \x20     child PATH.\n\
         \x20 xtask dev-ui [-- <app args>]\n\
         \x20     Same as dev, with the dev-ui profile.\n\
         \x20 xtask mingw-dir\n\
         \x20     Print the MinGW directory the windows-gnu toolchain needs.\n\
         \x20 xtask watch [bacon job]\n\
         \x20     Rebuild and restart on save through bacon (job: run).\n\
         \x20 xtask set-version <semver>\n\
         \x20     Rewrite the app crate version in one place and confirm it\n\
         \x20     through cargo metadata. A lower number is refused, and a\n\
         \x20     refused read-back restores the manifest bytes it replaced.\n\
         \x20 xtask ci\n\
         \x20     Run the workspace gate: format, clippy, core and workspace\n\
         \x20     tests, and docs.\n\
         \n\
         See CONTRIBUTING.md for the full command list."
    );
}
