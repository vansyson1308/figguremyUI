//! The URL scheme of the app's own links: invite links (`figguremyui://join?…`) and the markers the
//! app puts on the clipboard (`figguremyui://node/…`, `…/clipboard/…`, `…/comment/…`).
//!
//! The app writes [`SCHEME`] only. Links and clipboard markers written by FigCraft, which this app
//! is a fork of, use [`OLD_SCHEME`]; they are still read when pasted or passed on the command line,
//! so an existing invite or copied layer keeps working. The desktop integration (the desktop
//! entry, Info.plist, the Windows registry) registers only [`SCHEME`], so this app never takes over
//! the other app's links.

/// The scheme the app writes.
pub const SCHEME: &str = "figguremyui";

/// The scheme FigCraft's links use, still read (brand: compat).
pub const OLD_SCHEME: &str = "figcraft"; // brand: compat

/// `figguremyui://`: the start of every link the app writes.
pub const PREFIX: &str = "figguremyui://";

/// Whether `scheme` (without `://`) is one of the app's own, in any letter case.
pub fn is_own(scheme: &str) -> bool {
    scheme.eq_ignore_ascii_case(SCHEME) || scheme.eq_ignore_ascii_case(OLD_SCHEME)
}

/// What follows `figguremyui://` or `figcraft://` at the start of `link` (the scheme in any letter
/// case); `None` for any other link.
pub fn strip(link: &str) -> Option<&str> {
    let (scheme, rest) = link.split_once("://")?;
    is_own(scheme).then_some(rest)
}

/// What follows `<scheme>://<path>` at the start of `text` for either scheme (the scheme in any
/// letter case, the path as given): `strip_path("figcraft://node/12", "node/")` is `Some("12")`.
pub fn strip_path<'a>(text: &'a str, path: &str) -> Option<&'a str> {
    strip(text)?.strip_prefix(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_schemes_are_read_and_only_the_new_one_is_written() {
        assert_eq!(PREFIX, format!("{SCHEME}://"));
        for link in ["figguremyui://join?x=1", "FiggureMyUI://join?x=1", "figcraft://join?x=1", "FigCraft://join?x=1"] {
            assert_eq!(strip(link), Some("join?x=1"), "{link}");
        }
        for other in ["https://join?x=1", "figguremyui:/join", "join?x=1", "", "figcrafty://join"] {
            assert_eq!(strip(other), None, "{other}");
        }
        assert_eq!(strip_path("figcraft://node/12", "node/"), Some("12"));
        assert_eq!(strip_path("figguremyui://node/12", "node/"), Some("12"));
        assert_eq!(strip_path("figguremyui://comment/3", "node/"), None);
    }
}
