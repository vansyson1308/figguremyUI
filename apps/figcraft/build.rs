//! Windows: embed the icon and version info in figguremyui.exe and size its main thread's stack
//! (no-op elsewhere).
fn main() {
    println!("cargo::rustc-check-cfg=cfg(windows_slim_errors)");
    println!("cargo::rerun-if-changed=../../assets/app-icon/figguremyui.ico");
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os == "windows" {
        let mut res = winresource::WindowsResource::new();
        let ico = "../../assets/app-icon/figguremyui.ico";
        if std::path::Path::new(ico).exists() {
            res.set_icon(ico);
        }
        res.set("ProductName", "Figgure My UI");
        res.set("FileDescription", "Figgure My UI");
        // A fork of FigCraft: not presented as made by the upstream team.
        res.set("CompanyName", "Figgure My UI contributors (github.com/vansyson1308/figguremyUI)");
        if let Err(e) = res.compile() {
            println!("cargo::warning=winresource: {e}");
        }
        // Windows starts the main thread with 1 MB of stack, Linux and macOS with 8 MB. Layers
        // nest up to 256 deep in an export, and that recursion needs the same room everywhere.
        let msvc = std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
        println!("cargo::rustc-link-arg-bins={}", if msvc { "/STACK:8388608" } else { "-Wl,--stack,8388608" });
    }
    // The commit to show in `--version`: FIGGUREMYUI_BUILD_SHA, else FigCraft's FIGCRAFT_BUILD_SHA.
    let sha = std::env::var("FIGGUREMYUI_BUILD_SHA").or_else(|_| std::env::var("FIGCRAFT_BUILD_SHA")).unwrap_or_default();
    println!("cargo::rustc-env=FIGGUREMYUI_BUILD_SHA={sha}");
    println!("cargo::rerun-if-env-changed=FIGGUREMYUI_BUILD_SHA");
    println!("cargo::rerun-if-env-changed=FIGCRAFT_BUILD_SHA");
}
