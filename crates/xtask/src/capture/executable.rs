use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// resolve_exe
// ---------------------------------------------------------------------------

/// Search the usual mingw `bin` candidates: msys2, Git for Windows, then
/// scoop's msys2. Returns the first directory containing `gcc.exe`.
#[cfg(windows)]
fn find_mingw_bin() -> Result<PathBuf, String> {
    let home = std::env::var("USERPROFILE")
        .map_err(|_| "USERPROFILE is not set in the environment".to_string())?;
    let candidates = [
        PathBuf::from(r"C:\msys64\mingw64\bin"),
        PathBuf::from(r"C:\Program Files\Git\mingw64\bin"),
        PathBuf::from(home)
            .join("scoop")
            .join("apps")
            .join("msys2")
            .join("current")
            .join("mingw64")
            .join("bin"),
    ];
    for c in &candidates {
        if c.join("gcc.exe").is_file() {
            return Ok(c.clone());
        }
    }
    Err("No GNU toolchain found. Tried C:\\msys64\\mingw64\\bin, \
         C:\\Program Files\\Git\\mingw64\\bin, and \
         <USERPROFILE>\\scoop\\apps\\msys2\\current\\mingw64\\bin. \
         Install MSYS2, then: pacman -S mingw-w64-x86_64-gcc"
        .to_string())
}

/// Locate the photrez debug executable, building it if necessary.
///
/// Order: the explicit `--exe` path; else `target/debug/photrez.exe` in the
/// one root target directory (the repository root is found by walking up from
/// this crate's location); else `cargo build -p photrez` in the workspace,
/// with the executable read out of `--message-format=json` compiler-artifact
/// rows. The mingw `bin` directory (msys64 / Git for Windows / scoop) is put
/// on PATH for the build: the gnu link chain needs it for ld's DLLs and for
/// windres' bare gcc lookup (see the `dev` command's rationale).
pub fn resolve_exe(explicit: Option<&Path>) -> Result<PathBuf, String> {
    // 1. Explicit path - trust but verify it exists.
    if let Some(p) = explicit {
        if p.exists() {
            return Ok(p.to_path_buf());
        }
        return Err(format!("explicit exe not found: {}", p.display()));
    }

    // 2. Default debug artefact location in the root target dir.
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| "cannot resolve xtask parent dir (CARGO_MANIFEST_DIR)".to_string())?
        .parent()
        .ok_or_else(|| "cannot resolve repository root from xtask location".to_string())?;
    let exe_name = if std::env::consts::EXE_EXTENSION.is_empty() {
        "photrez".to_string()
    } else {
        format!("photrez.{}", std::env::consts::EXE_EXTENSION)
    };
    let default_exe = repo_root.join("target").join("debug").join(&exe_name);
    if default_exe.exists() {
        return Ok(default_exe);
    }

    // 3. Fresh build via cargo build --message-format=json.
    let mut cmd = std::process::Command::new("cargo");
    cmd.current_dir(repo_root);
    #[cfg(windows)]
    {
        let mingw = find_mingw_bin()?;
        let old_path = std::env::var("PATH").unwrap_or_default();
        cmd.env("PATH", format!("{};{}", mingw.display(), old_path));
    }
    cmd.args(["build", "-p", "photrez", "--message-format=json"]);
    let output = cmd
        .output()
        .map_err(|e| format!("failed to spawn cargo build: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("cargo build failed:\n{stderr}"));
    }

    // Parse compiler-artifact JSON rows to find the photrez binary path.
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let msg: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if msg.get("reason").and_then(|v| v.as_str()) != Some("compiler-artifact") {
            continue;
        }
        let target = match msg.get("target").and_then(|v| v.as_object()) {
            Some(t) => t,
            None => continue,
        };
        let is_bin = target
            .get("kind")
            .and_then(|v| v.as_array())
            .map(|kinds| kinds.iter().any(|k| k.as_str() == Some("bin")))
            .unwrap_or(false);
        if !is_bin {
            continue;
        }
        if msg.get("package").and_then(|v| v.as_str()) != Some("photrez") {
            continue;
        }
        if let Some(exe) = msg.get("executable").and_then(|v| v.as_str()) {
            let p = PathBuf::from(exe);
            if p.exists() {
                return Ok(p);
            }
        }
    }
    Err("cargo build succeeded but no photrez executable was found in the artifact output".into())
}
