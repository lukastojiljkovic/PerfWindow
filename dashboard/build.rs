use std::path::Path;

fn main() {
    // Copy the published sensord executable into OUT_DIR so src/ipc/process.rs
    // can include_bytes! it with a stable path. Publish sensord before building.
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR");
    let sensord =
        Path::new("../sensord/src/bin/Release/net8.0-windows/win-x64/publish/sensord.exe");
    let dest = Path::new(&out_dir).join("sensord.exe");

    if !sensord.exists() {
        panic!(
            "sensord.exe not found at {} — run: dotnet publish sensord/src -c Release -r win-x64 --self-contained",
            sensord.display()
        );
    }
    std::fs::copy(sensord, &dest).expect("copy sensord.exe");
    println!("cargo:rerun-if-changed={}", sensord.display());

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
