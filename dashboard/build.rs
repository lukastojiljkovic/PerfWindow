use std::path::{Path, PathBuf};

fn main() {
    // The published, self-contained sensord output. PerfWindow ships it as
    // sibling files and spawns sensord.exe directly, so this build must run
    // after `dotnet publish` has produced it. Folder publish on purpose: a
    // single-file bundle self-extracts (and gets Defender-rescanned) inside
    // the SCM service-start window, and its compressed assemblies load as
    // private memory instead of shareable file mappings.
    let publish_dir = Path::new("../sensord/src/bin/Release/net8.0-windows/win-x64/publish");
    if !publish_dir.join("sensord.exe").exists() {
        panic!(
            "sensord.exe not found in {} — run: dotnet publish sensord/src -c Release -r win-x64 --self-contained",
            publish_dir.display()
        );
    }
    println!("cargo:rerun-if-changed={}", publish_dir.display());

    // Copy the publish output next to the PerfWindow.exe cargo is about to
    // produce, so the build output is a runnable pair — both a dev build and
    // the Inno Setup installer expect sensord.exe (plus its runtime) as
    // direct siblings of PerfWindow.exe.
    match output_bin_dir() {
        Some(dir) => copy_publish_dir(publish_dir, &dir),
        None => println!(
            "cargo:warning=could not locate the cargo output directory; \
             sensord was not placed next to PerfWindow.exe"
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

/// Copies every file from the sensord publish directory flat into `dest_dir`.
/// The .NET host resolves the runtime from the directory sensord.exe lives
/// in, so the layout must stay flat with sensord.exe a sibling of
/// PerfWindow.exe.
fn copy_publish_dir(src_dir: &Path, dest_dir: &Path) {
    let entries = std::fs::read_dir(src_dir)
        .unwrap_or_else(|e| panic!("read publish dir {}: {e}", src_dir.display()));
    for entry in entries {
        let entry = entry.unwrap_or_else(|e| panic!("read publish dir entry: {e}"));
        let src = entry.path();
        if src.is_dir() {
            // Self-contained win-x64 publish is flat; a subdirectory would
            // mean output this copy does not ship — surface it loudly.
            println!(
                "cargo:warning=unexpected directory in sensord publish output, not copied: {}",
                src.display()
            );
            continue;
        }
        let name = entry.file_name();
        let dest = dest_dir.join(&name);
        if let Err(e) = std::fs::copy(&src, &dest) {
            // A running sensord can hold the destination locked; reusing it
            // is tolerable for an up-to-date file, but a stale sensord.exe
            // must never ship silently.
            if !dest.exists() {
                panic!("copy {} to {}: {e}", src.display(), dest.display());
            }
            if name == "sensord.exe" && dest_older_than_src(&dest, &src) {
                panic!(
                    "sensord.exe at {} is locked and OLDER than the freshly published one — \
                     stop the running sensord (sc.exe stop PerfWindowSensor) and rebuild",
                    dest.display()
                );
            }
            println!(
                "cargo:warning=reusing existing {} ({e})",
                name.to_string_lossy()
            );
        }
    }
}

/// True when `dest` is provably older than `src`, or when either mtime is
/// unreadable — an unverifiable binary is treated as stale rather than
/// trusted.
fn dest_older_than_src(dest: &Path, src: &Path) -> bool {
    let mtime = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    match (mtime(dest), mtime(src)) {
        (Some(d), Some(s)) => d < s,
        _ => true,
    }
}

/// The directory cargo places the final binary in (`target/<profile>`, or
/// `target/<triple>/<profile>`), derived from `OUT_DIR`, which always ends
/// `…/<profile>/build/<pkg>-<hash>/out`.
fn output_bin_dir() -> Option<PathBuf> {
    let out_dir = std::env::var_os("OUT_DIR")?;
    Path::new(&out_dir)
        .ancestors()
        .nth(3)
        .map(Path::to_path_buf)
}
