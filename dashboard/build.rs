use std::path::{Path, PathBuf};

fn main() {
    // The published, self-contained sensord executable. PerfWindow ships it as
    // a sibling file and spawns it directly — it is no longer embedded, so this
    // build must run after `dotnet publish` has produced it.
    let sensord =
        Path::new("../sensord/src/bin/Release/net8.0-windows/win-x64/publish/sensord.exe");
    if !sensord.exists() {
        panic!(
            "sensord.exe not found at {} — run: dotnet publish sensord/src -c Release -r win-x64 --self-contained",
            sensord.display()
        );
    }
    println!("cargo:rerun-if-changed={}", sensord.display());

    // Copy sensord.exe next to the PerfWindow.exe cargo is about to produce, so
    // the build output is a runnable pair — both a dev build and the Inno Setup
    // installer expect the two executables side by side.
    match output_bin_dir() {
        Some(dir) => {
            let dest = dir.join("sensord.exe");
            if let Err(e) = std::fs::copy(sensord, &dest) {
                if dest.exists() {
                    println!("cargo:warning=reusing existing sensord.exe ({e})");
                } else {
                    panic!("copy sensord.exe to {}: {e}", dest.display());
                }
            }
        }
        None => println!(
            "cargo:warning=could not locate the cargo output directory; \
             sensord.exe was not placed next to PerfWindow.exe"
        ),
    }

    // Embed the application manifest (requests elevation) and the icon into
    // PerfWindow.exe via the Windows resource compiler.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rerun-if-changed=assets/PerfWindow.manifest");
        println!("cargo:rerun-if-changed=assets/PerfWindow.ico");
        let mut res = winresource::WindowsResource::new();
        res.set_manifest_file("assets/PerfWindow.manifest");
        res.set_icon("assets/PerfWindow.ico");
        res.compile().expect("embed Windows resources");
    }
}

/// The directory cargo places the final binary in (`target/<profile>`, or
/// `target/<triple>/<profile>`), derived from `OUT_DIR`, which always ends
/// `…/<profile>/build/<pkg>-<hash>/out`.
fn output_bin_dir() -> Option<PathBuf> {
    let out_dir = std::env::var_os("OUT_DIR")?;
    Path::new(&out_dir).ancestors().nth(3).map(Path::to_path_buf)
}
