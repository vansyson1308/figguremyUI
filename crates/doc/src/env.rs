//! The app's environment variables: `FIGGUREMYUI_<NAME>`, with FigCraft's `FIGCRAFT_<NAME>` still
//! read when the new one isn't set (this app is a fork of FigCraft; a setup that names the old
//! variables keeps working). Every crate that reads one goes through [`var_os`], so the two names
//! always agree. Build-time inputs (`option_env!`) follow the same rule at their call sites.

use std::ffi::OsString;

/// The prefix of the app's variables.
pub const PREFIX: &str = "FIGGUREMYUI_";
/// The prefix FigCraft's variables have, still read (brand: compat).
pub const OLD_PREFIX: &str = "FIGCRAFT_";

/// `FIGGUREMYUI_<name>` from the process environment, else `FIGCRAFT_<name>` (`name` like
/// `"NO_SYSTEM_FONTS"`). A variable set to the empty string counts as set.
pub fn var_os(name: &str) -> Option<OsString> {
    var_with(name, |n: &str| std::env::var_os(n))
}

/// [`var_os`] as a string; `None` when neither is set or the value isn't UTF-8.
pub fn var(name: &str) -> Option<String> {
    var_os(name).and_then(|v| v.into_string().ok())
}

/// [`var_os`] over another environment (`get` reads one variable by its full name), for tests and
/// for callers that read the environment through a probe.
pub fn var_with(name: &str, get: impl Fn(&str) -> Option<OsString>) -> Option<OsString> {
    get(&format!("{PREFIX}{name}")).or_else(|| get(&format!("{OLD_PREFIX}{name}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(vars: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<OsString> {
        move |n| vars.iter().find(|(k, _)| *k == n).map(|(_, v)| OsString::from(v))
    }

    #[test]
    fn the_new_name_wins_and_the_old_one_is_still_read() {
        let both = env(&[("FIGGUREMYUI_X", "new"), ("FIGCRAFT_X", "old")]);
        assert_eq!(var_with("X", both), Some("new".into()));
        assert_eq!(var_with("X", env(&[("FIGCRAFT_X", "old")])), Some("old".into()));
        assert_eq!(var_with("X", env(&[("FIGGUREMYUI_X", "")])), Some("".into()), "empty is set");
        assert_eq!(var_with("X", env(&[("FIGCRAFT_Y", "other")])), None);
    }
}
