//! When the window can't open.
//!
//! Release builds on Windows have no console, so an error from `eframe::run_native` (no usable
//! graphics adapter, a surface or device the driver refuses) went to a standard error nobody sees
//! and the app quit without a word (#261). Now it is logged and shown in a message box.
//!
//! On Linux and the BSDs the window and its graphics load system libraries when the app starts
//! ([`LIBRARIES`]). One that isn't installed made winit panic with a backtrace (libxkbcommon-x11),
//! or the graphics fail with an error that named nothing (libEGL) (F-16). Now the message names the
//! library, its package and where to read more, and the app exits with status 1.

use std::panic::PanicHookInfo;
use std::path::Path;
use std::sync::{Arc, Mutex};

/// Log the failure and, unless `quiet` (started for an agent with `--control`: a modal box would
/// keep the caller waiting, and it reads standard error), show it in a message box. `tried`: the
/// graphics APIs the app started again with before this failure ([`crate::graphics::Plan::finish`]).
pub fn failed(error: &eframe::Error, quiet: bool, tried: Option<&str>) {
    // In the interface language: the text and its catalog entries are the UI crate's.
    let mut text = figcraft_ui_egui::dialogs::window_failed(&error.to_string(), std::env::var_os("WGPU_BACKEND").is_some(), tried);
    // When the graphics failed, a library the window loads that isn't there is the likeliest cause,
    // and says what to do (not when there is no display to open a window on at all).
    let missing = if matches!(error, eframe::Error::Wgpu(_)) { missing(&installed_libraries()) } else { Vec::new() };
    if !missing.is_empty() {
        text = format!("{}\n\n{text}", missing_text(&missing, None));
    }
    say(&text, quiet);
}

/// Tell the person why the window didn't open: on standard error and, unless `quiet` (see
/// [`failed`]), in a message box.
pub fn say(text: &str, quiet: bool) {
    // Standard error even when RUST_LOG turned errors off.
    if log::log_enabled!(log::Level::Error) {
        log::error!("{text}");
    } else {
        eprintln!("figguremyui: {text}");
    }
    if quiet {
        return;
    }
    let _ = rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title("Figgure My UI")
        .set_description(text)
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
}

/// A system library the window or its graphics load when the app starts (Linux and the BSDs).
#[derive(Debug, PartialEq, Eq)]
pub struct Library {
    /// The file the dynamic loader looks for.
    pub file: &'static str,
    /// The Debian and Ubuntu package that installs it.
    pub package: &'static str,
}

/// What the desktop app loads when it starts, as the README's Linux libraries and
/// `docs/install-for-agents.md` list them.
pub const LIBRARIES: [Library; 4] = [
    Library { file: "libxkbcommon-x11.so.0", package: "libxkbcommon-x11-0" },
    Library { file: "libwayland-client.so.0", package: "libwayland-client0" },
    Library { file: "libGL.so.1", package: "libgl1" },
    Library { file: "libEGL.so.1", package: "libegl1" },
];

/// The library of [`LIBRARIES`] that `text` (a panic message or an error of the window or graphics
/// layer, such as "Library libxkbcommon-x11.so could not be loaded.") names, by its `lib….so` name.
pub fn named_library(text: &str) -> Option<&'static Library> {
    LIBRARIES.iter().find(|l| l.file.find(".so").and_then(|end| l.file.get(..end + 3)).is_some_and(|so| text.contains(so)))
}

/// The libraries of [`LIBRARIES`] that `installed` (whether the loader finds a file) says are missing.
pub fn missing(installed: &dyn Fn(&str) -> bool) -> Vec<&'static Library> {
    LIBRARIES.iter().filter(|l| !installed(l.file)).collect()
}

/// Whether the dynamic loader finds library `file`: in a folder of `ld_path` (`LD_LIBRARY_PATH`,
/// `exists` saying whether a file is there) or by name in its cache (`/etc/ld.so.cache`, `cache`).
/// Without a cache to read nothing says it is missing, so it counts as found.
pub fn loader_finds(file: &str, ld_path: Option<&std::ffi::OsStr>, cache: Option<&[u8]>, exists: &dyn Fn(&Path) -> bool) -> bool {
    let Some(cache) = cache else { return true };
    let in_path = ld_path.is_some_and(|p| std::env::split_paths(p).any(|dir| exists(&dir.join(file))));
    in_path || cache.windows(file.len()).any(|w| w == file.as_bytes())
}

/// What the loader of this system finds ([`loader_finds`] with this process's `LD_LIBRARY_PATH`
/// and the loader's cache).
#[cfg(all(unix, not(target_os = "macos")))]
fn installed_libraries() -> impl Fn(&str) -> bool {
    /// The cache is a few hundred kilobytes; anything far larger isn't one.
    const MAX_CACHE: u64 = 64 << 20;
    let path = Path::new("/etc/ld.so.cache");
    let cache = std::fs::metadata(path).ok().filter(|m| m.is_file() && m.len() <= MAX_CACHE).and_then(|_| std::fs::read(path).ok());
    let ld_path = std::env::var_os("LD_LIBRARY_PATH");
    move |file| loader_finds(file, ld_path.as_deref(), cache.as_deref(), &|p| p.is_file())
}

/// macOS and Windows load nothing from [`LIBRARIES`].
#[cfg(not(all(unix, not(target_os = "macos"))))]
fn installed_libraries() -> impl Fn(&str) -> bool {
    |_| true
}

/// Where to read about the libraries.
pub const READ_MORE: &str =
    "the README (\"On Linux the app needs the usual desktop libraries\") and docs/install-for-agents.md (section 1, and 11. Troubleshooting)";

/// The message for `libs` missing, with what the window layer said (`detail`) when there is that.
pub fn missing_text(libs: &[&Library], detail: Option<&str>) -> String {
    let files: Vec<&str> = libs.iter().map(|l| l.file).collect();
    let packages: Vec<&str> = libs.iter().map(|l| l.package).collect();
    let (what, verb) = if libs.len() == 1 { ("library", "isn't") } else { ("libraries", "aren't") };
    let mut text = format!(
        "the window can't start: the system {what} {} {verb} installed. On Debian or Ubuntu: sudo apt-get install {} (other systems name the packages differently). How to check and install what the desktop app needs: {READ_MORE}. The headless figguremyui-cli needs none of them.",
        files.join(", "),
        packages.join(" ")
    );
    if let Some(d) = detail {
        text.push_str(&format!("\n(The window layer said: {})", d.trim()));
    }
    text
}

/// The text a panic carries.
pub fn panic_text(payload: &(dyn std::any::Any + Send)) -> String {
    payload.downcast_ref::<&str>().map(ToString::to_string).or_else(|| payload.downcast_ref::<String>().cloned()).unwrap_or_default()
}

type Hook = Box<dyn Fn(&PanicHookInfo<'_>) + Send + Sync>;

/// Run the window (`run`: `eframe::run_native`) so that a system library its window or graphics
/// layer can't load ends with [`missing_text`] in `Err` rather than a panic and its backtrace
/// (F-16). winit loads libxkbcommon-x11 as its event loop is made and panics (an `expect` inside
/// `xkbcommon-dl`) when it isn't there: no `Result` reaches eframe or the app, so the panic is
/// caught here, at the boundary, with `catch_unwind`. Only that panic is quiet, and only while
/// `starting` (the window hasn't begun a frame); any other panic goes to the hook that was set,
/// as before, and goes on unwinding from here.
pub fn run_window<T>(starting: impl Fn() -> bool + Send + Sync + 'static, run: impl FnOnce() -> T) -> Result<T, String> {
    let quiet: Arc<Mutex<Option<String>>> = Arc::default();
    let previous: Arc<Hook> = Arc::new(std::panic::take_hook());
    {
        let (quiet, previous) = (quiet.clone(), previous.clone());
        std::panic::set_hook(Box::new(move |info| {
            let text = panic_text(info.payload());
            if starting()
                && named_library(&text).is_some()
                && let Ok(mut q) = quiet.lock()
            {
                *q = Some(text);
                return;
            }
            (*previous)(info);
        }));
    }
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(run));
    // Ours goes, with its share of the hook before it, which is set again.
    drop(std::panic::take_hook());
    if let Ok(previous) = Arc::try_unwrap(previous) {
        std::panic::set_hook(previous);
    }
    match result {
        Ok(v) => Ok(v),
        Err(payload) => match quiet.lock().ok().and_then(|mut q| q.take()) {
            Some(text) => Err(missing_text(&named_library(&text).into_iter().collect::<Vec<_>>(), Some(&text))),
            None => std::panic::resume_unwind(payload),
        },
    }
}
