use std::env;
use std::fs;
use std::path::Path;

fn main() {
    let target = env::var("TARGET").unwrap_or_default();
    if target.contains("windows-gnu") {
        println!("cargo:warning=MinGW target detected. Using custom manifest compiler workaround.");
        // tauri_build::build() generates ACL/capabilities (gen/ folder) but then
        // panics on tauri-winres because windres preprocessing fails on MinGW.
        // Use catch_unwind: the ACL cargo: directives are printed before the panic,
        // so Cargo still processes them. Our custom windres code below handles resources.
        let result = std::panic::catch_unwind(|| {
            tauri_build::build();
        });
        if result.is_err() {
            println!("cargo:warning=tauri_build::build() panicked on winres (expected on MinGW). ACL/capabilities already generated.");
        }
    } else if target.contains("windows") {
        tauri_build::build();
    }

    // Auto-copy WebView2Loader.dll and generate external manifest for Windows targets
    if target.contains("windows") {
        if let Ok(out_dir) = env::var("OUT_DIR") {
            let out_path = Path::new(&out_dir);
            // Go up 3 levels: out -> photrez-desktop-hash -> build -> debug/release
            if let Some(profile_dir) = out_path
                .parent()
                .and_then(|p| p.parent())
                .and_then(|p| p.parent())
            {
                // 1. Copy WebView2Loader.dll
                let build_dir = profile_dir.join("build");
                if build_dir.exists() {
                    // Look for webview2-com-sys output directory
                    if let Ok(entries) = fs::read_dir(&build_dir) {
                        for entry in entries.filter_map(Result::ok) {
                            let path = entry.path();
                            if path.is_dir() {
                                if let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) {
                                    if dir_name.starts_with("webview2-com-sys-") {
                                        let dll_path =
                                            path.join("out").join("x64").join("WebView2Loader.dll");
                                        if dll_path.exists() {
                                            let dest_path = profile_dir.join("WebView2Loader.dll");
                                            if let Err(e) = fs::copy(&dll_path, &dest_path) {
                                                println!("cargo:warning=Failed to copy WebView2Loader.dll: {:?}", e);
                                            } else {
                                                println!("cargo:warning=Successfully copied WebView2Loader.dll to profile directory.");
                                            }
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }

                // 2. Custom manifest compiler workaround for MinGW/GNU target to prevent COMCTL32 version mismatch
                if target.contains("windows-gnu") {
                    let manifest_content = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <assemblyIdentity
    version="1.0.0.0"
    processorArchitecture="*"
    name="photrez-desktop"
    type="win32"
  />
  <description>Photrez</description>
  <dependency>
    <dependentAssembly>
      <assemblyIdentity
        type="win32"
        name="Microsoft.Windows.Common-Controls"
        version="6.0.0.0"
        processorArchitecture="*"
        publicKeyToken="6595b64144ccf1df"
        language="*"
      />
    </dependentAssembly>
  </dependency>
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel
          level="asInvoker"
          uiAccess="false"
        />
      </requestedPrivileges>
    </security>
  </trustInfo>
  <compatibility xmlns="urn:schemas-microsoft-com:compatibility.v1">
    <application>
      <!-- Windows 10 and 11 -->
      <supportedOS Id="{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}"/>
    </application>
  </compatibility>
</assembly>
"#;
                    let manifest_path = out_path.join("app.manifest");
                    let rc_path = out_path.join("app.rc");
                    let obj_path = out_path.join("app.o");

                    if fs::write(&manifest_path, manifest_content).is_ok() {
                        // 1 is ID, 24 is RT_MANIFEST
                        if fs::write(&rc_path, "1 24 \"app.manifest\"\n").is_ok() {
                            // Compile app.rc using windres with --preprocessor=cat to bypass preprocessor bug!
                            let status = std::process::Command::new("windres")
                                .arg("--preprocessor=cat")
                                .arg("-i")
                                .arg(&rc_path)
                                .arg("-O")
                                .arg("coff")
                                .arg("-o")
                                .arg(&obj_path)
                                .current_dir(out_path)
                                .status();

                            match status {
                                Ok(s) if s.success() => {
                                    println!("cargo:warning=Successfully compiled manifest resource via windres --no-cpp");
                                    // Tell Cargo to link the compiled object file directly!
                                    println!("cargo:rustc-link-arg={}", obj_path.to_str().unwrap());
                                }
                                Ok(s) => {
                                    println!(
                                        "cargo:warning=windres failed with exit code: {:?}",
                                        s.code()
                                    );
                                }
                                Err(e) => {
                                    println!("cargo:warning=Could not execute windres: {:?}", e);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
