//! The message shown when the window can't open: it carries the error, a next step and where to
//! report it, instead of the app quitting silently (#261).

use figcraft_ui_egui::dialogs::window_failed as message;
use figcraft_ui_egui::i18n::tr;

const ERROR: &str = "WGPU error: Failed to create surface for any enabled backend: {}";
const BACKEND_HINT: &str = "WGPU_BACKEND is set and names the graphics API to use; unsetting it lets Figgure My UI choose.";
const DRIVER_HINT: &str = if cfg!(windows) {
    "Updating the graphics driver often helps. To use OpenGL only, start Figgure My UI from a Command Prompt with:"
} else {
    "Updating the graphics driver often helps. WGPU_BACKEND (vulkan, gl, metal) names another graphics API to try."
};

#[test]
fn the_message_carries_the_error_and_where_to_report_it() {
    let text = message(ERROR, false, None);
    assert!(text.contains(ERROR), "the error itself: {text}");
    assert!(text.contains("https://github.com/vansyson1308/figguremyUI/issues"), "where to report it: {text}");
}

#[test]
fn without_wgpu_backend_it_suggests_the_driver_and_another_graphics_api() {
    let text = message(ERROR, false, None);
    assert!(text.contains(tr(DRIVER_HINT)), "{text}");
    assert!(text.contains("WGPU_BACKEND"), "another graphics API to try: {text}");
    assert!(!text.contains(tr(BACKEND_HINT)), "{text}");
}

#[cfg(windows)]
#[test]
fn on_windows_it_shows_how_to_start_with_opengl() {
    let text = message(ERROR, false, None);
    assert!(text.contains("set WGPU_BACKEND=gl"), "a release build has no console to look this up from: {text}");
}

#[test]
fn with_wgpu_backend_set_it_suggests_unsetting_it() {
    let text = message(ERROR, true, None);
    assert!(text.contains(tr(BACKEND_HINT)), "{text}");
    assert!(!text.contains(tr(DRIVER_HINT)), "the chosen backend is the likelier cause: {text}");
}

#[test]
fn after_every_attempt_failed_it_names_what_was_tried_and_no_other_api() {
    let text = message(ERROR, false, Some("Direct3D 12, Vulkan, OpenGL"));
    assert!(text.contains(ERROR), "the last error: {text}");
    assert!(text.contains("Direct3D 12, Vulkan, OpenGL"), "what was tried: {text}");
    assert!(!text.contains(tr(DRIVER_HINT)), "every API was tried already: {text}");
    assert!(text.contains("https://github.com/vansyson1308/figguremyUI/issues"), "where to report it: {text}");
}

mod missing_libraries {
    use std::ffi::OsStr;
    use std::path::Path;

    use crate::startup::{LIBRARIES, loader_finds, missing, missing_text, named_library, run_window};

    /// The panic xkbcommon-dl raises inside winit when libxkbcommon-x11 isn't installed (F-16).
    const XKB_PANIC: &str = "Library libxkbcommon-x11.so could not be loaded.";

    #[test]
    fn a_window_layer_message_names_its_library() {
        assert_eq!(named_library(XKB_PANIC).map(|l| l.package), Some("libxkbcommon-x11-0"));
        assert_eq!(named_library("failed to load libEGL.so.1").map(|l| l.package), Some("libegl1"));
        assert_eq!(named_library("libGL.so.1: cannot open shared object file").map(|l| l.package), Some("libgl1"));
        assert_eq!(named_library("index out of bounds"), None);
    }

    #[test]
    fn the_loader_finds_a_library_by_its_cache_or_its_path() {
        let cache = b"\0libGL.so.1\0/lib/x86_64-linux-gnu/libGL.so.1\0libxkbcommon.so.0\0";
        let nothing = |_: &Path| false;
        assert!(loader_finds("libGL.so.1", None, Some(cache), &nothing));
        assert!(!loader_finds("libEGL.so.1", None, Some(cache), &nothing));
        assert!(!loader_finds("libxkbcommon-x11.so.0", None, Some(cache), &nothing), "libxkbcommon.so.0 is another library");
        let there = |p: &Path| p == Path::new("/opt/x/libEGL.so.1");
        assert!(loader_finds("libEGL.so.1", Some(OsStr::new("/nope:/opt/x")), Some(cache), &there));
        assert!(loader_finds("libEGL.so.1", None, None, &nothing), "no cache to read: nothing says it is missing");
        let found: Vec<_> = missing(&|f| f == "libGL.so.1" || f == "libwayland-client.so.0").iter().map(|l| l.file).collect();
        assert_eq!(found, ["libxkbcommon-x11.so.0", "libEGL.so.1"]);
        assert_eq!(LIBRARIES.len(), 4);
    }

    #[test]
    fn the_message_names_the_library_its_package_and_where_to_read_more() {
        let lib = named_library(XKB_PANIC).unwrap();
        let text = missing_text(&[lib], Some(XKB_PANIC));
        assert!(text.contains("libxkbcommon-x11.so.0 isn't installed"), "{text}");
        assert!(text.contains("sudo apt-get install libxkbcommon-x11-0"), "{text}");
        assert!(text.contains("docs/install-for-agents.md") && text.contains("README"), "{text}");
        assert!(text.contains(XKB_PANIC), "what the window layer said: {text}");
        let two = missing_text(&LIBRARIES.iter().skip(2).collect::<Vec<_>>(), None);
        assert!(two.contains("libGL.so.1, libEGL.so.1 aren't installed") && two.contains("install libgl1 libegl1"), "{two}");
    }

    /// The panic a missing library raises while the window starts becomes the message; any other
    /// panic, or one after the window began, goes on unwinding as before.
    #[test]
    fn only_a_missing_library_at_startup_is_caught() {
        let caught = run_window(|| true, || panic!("{XKB_PANIC}"));
        assert!(caught.as_ref().is_err_and(|t| t.contains("libxkbcommon-x11-0")), "{caught:?}");
        assert_eq!(run_window(|| true, || 7), Ok(7));
        for (starting, text) in [(true, "something else broke"), (false, XKB_PANIC)] {
            let unwound = std::panic::catch_unwind(|| run_window(move || starting, || panic!("{text}")));
            let payload = unwound.expect_err("resumed");
            assert_eq!(crate::startup::panic_text(payload.as_ref()), text);
        }
    }
}
