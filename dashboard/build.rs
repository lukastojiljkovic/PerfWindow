use std::path::Path;

fn main() {
    // Copy the published sensord executable into OUT_DIR so src/ipc/process.rs
    // can include_bytes! it with a stable path. Publish sensord before building.
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR");
    let sensord =
        Path::new("../sensord/src/bin/Release/net8.0-windows/win-x64/publish/sensord.exe");
    let dest = Path::new(&out_dir).join("sensord.exe");

    if sensord.exists() {
        std::fs::copy(sensord, &dest).expect("copy sensord.exe");
    } else if !dest.exists() {
        panic!(
            "sensord.exe not found at {} — run: dotnet publish sensord/src -c Release -r win-x64 --self-contained",
            sensord.display()
        );
    }
    println!("cargo:rerun-if-changed={}", sensord.display());
}
