//! Where the settings live (#1320): a portable copy keeps them next to the executable, taking the
//! ones it had before; without a marker they live in the platform folder's `FiggureMyUI`, which
//! starts with a copy of FigCraft's preferences. Every folder here is a temporary one: the probe
//! sees no real environment, executable or profile.

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use craft_settings::Probe;
use figcraft_engine::Prefs;

use crate::services::{Desktop, settings_dir, write_prefs};

/// A fresh temporary folder with an `app` folder (the executable's) and an `appdata` folder (the
/// platform's config folder).
fn root(name: &str) -> PathBuf {
    let r = std::env::temp_dir().join(format!("figcraft-settings-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&r);
    std::fs::create_dir_all(r.join("app")).unwrap();
    std::fs::create_dir_all(r.join("appdata")).unwrap();
    r
}

/// What the rules see: the executable in `root/app`, the platform folder `platform`, and
/// `FIGGUREMYUI_CONFIG_DIR` set to `config_dir` when given.
fn probe(root: &Path, platform: Option<PathBuf>, config_dir: Option<&'static str>) -> Probe {
    probe_env(root, platform, config_dir.map(|v| ("FIGGUREMYUI_CONFIG_DIR", v)))
}

/// [`probe`] with the variable `var` (name, value) set.
fn probe_env(root: &Path, platform: Option<PathBuf>, var: Option<(&'static str, &'static str)>) -> Probe {
    Probe {
        env: Box::new(move |name| var.filter(|(n, _)| *n == name).map(|(_, v)| OsString::from(v))),
        exe_dir: Some(root.join("app")),
        platform_config_dir: platform,
    }
}

/// Preferences with a name to recognise them by.
fn prefs(name: &str) -> Prefs {
    Prefs { user_name: name.into(), ..Prefs::default() }
}

/// The user name of the preferences the app starts with from `dir`.
fn loaded_name(dir: PathBuf) -> Option<String> {
    Desktop::load_from(Some(dir), Arc::new(Mutex::new(None))).prefs.map(|p| p.user_name)
}

#[test]
fn without_a_marker_the_settings_live_in_the_platform_folder() {
    let root = root("installed");
    let own = root.join("appdata").join("FiggureMyUI");
    write_prefs(&own, &prefs("Ada"));
    let dir = settings_dir(&probe(&root, Some(root.join("appdata")), None));
    assert_eq!(dir, Some(own.clone()), "the app's own folder");
    assert_eq!(loaded_name(own), Some("Ada".into()), "an existing user's preferences are read");
    assert!(!root.join("app").join("config").exists(), "nothing is written next to the executable");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_portable_marker_keeps_the_settings_next_to_the_executable_and_takes_the_old_ones() {
    let root = root("portable");
    let old = root.join("appdata").join("FiggureMyUI");
    write_prefs(&old, &prefs("Ada"));
    std::fs::write(root.join("app").join("portable"), "").unwrap();
    let portable = root.join("app").join("config");
    let dir = settings_dir(&probe(&root, Some(root.join("appdata")), None));
    assert_eq!(dir, Some(portable.clone()));
    assert_eq!(loaded_name(portable.clone()), Some("Ada".into()), "the copy starts with the settings it had");
    assert_eq!(loaded_name(old.clone()), Some("Ada".into()), "the installed app's stay where they were");
    // Later starts keep the portable copy's own settings.
    write_prefs(&portable, &prefs("Grace"));
    assert_eq!(settings_dir(&probe(&root, Some(root.join("appdata")), None)), Some(portable.clone()));
    assert_eq!(loaded_name(portable), Some("Grace".into()), "never overwritten by the old ones again");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_new_portable_copy_starts_without_settings() {
    let root = root("fresh");
    std::fs::write(root.join("app").join("portable"), "").unwrap();
    let portable = root.join("app").join("config");
    assert_eq!(settings_dir(&probe(&root, Some(root.join("appdata")), None)), Some(portable.clone()));
    assert_eq!(loaded_name(portable), None, "a first run");
    // No platform folder at all: the marker still decides.
    assert_eq!(settings_dir(&probe(&root, None, None)), Some(root.join("app").join("config")));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn the_config_dir_variable_names_the_folder_and_empty_means_none() {
    let root = root("env");
    std::fs::write(root.join("app").join("portable"), "").unwrap();
    let named = settings_dir(&probe(&root, Some(root.join("appdata")), Some("elsewhere")));
    assert_eq!(named, Some(PathBuf::from("elsewhere")), "ahead of the marker");
    assert_eq!(settings_dir(&probe(&root, Some(root.join("appdata")), Some(""))), None, "isolated: nothing read or written");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn without_a_platform_folder_there_are_no_settings() {
    let root = root("nowhere");
    assert_eq!(settings_dir(&probe(&root, None, None)), None, "not the current directory");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_new_settings_folder_starts_with_a_copy_of_figcrafts_preferences() {
    let root = root("migrate");
    let figcraft = root.join("appdata").join("FigCraft");
    write_prefs(&figcraft, &prefs("Ada"));
    std::fs::create_dir_all(figcraft.join("Recovery")).unwrap();
    let before = std::fs::read(figcraft.join("state.json")).unwrap();
    let own = root.join("appdata").join("FiggureMyUI");
    assert_eq!(settings_dir(&probe(&root, Some(root.join("appdata")), None)), Some(own.clone()));
    assert_eq!(loaded_name(own.clone()), Some("Ada".into()), "copied on the first start");
    assert_eq!(std::fs::read(figcraft.join("state.json")).unwrap(), before, "FigCraft's own file is untouched");
    assert!(!own.join("Recovery").exists(), "recovery copies are not moved over");
    // Once the app has its own preferences, FigCraft's are never copied again.
    write_prefs(&own, &prefs("Grace"));
    write_prefs(&figcraft, &prefs("Linus"));
    settings_dir(&probe(&root, Some(root.join("appdata")), None));
    assert_eq!(loaded_name(own), Some("Grace".into()));
    assert_eq!(loaded_name(figcraft), Some("Linus".into()));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn figcrafts_preferences_that_dont_read_are_not_copied() {
    let root = root("migrate-bad");
    let figcraft = root.join("appdata").join("FigCraft");
    std::fs::create_dir_all(&figcraft).unwrap();
    for bad in ["not json", r#"{"prefs": 3}"#, r#"{"other": {}}"#] {
        std::fs::write(figcraft.join("state.json"), bad).unwrap();
        let own = settings_dir(&probe(&root, Some(root.join("appdata")), None)).unwrap();
        assert!(!own.join("state.json").exists(), "{bad}: the app starts with the defaults");
        assert_eq!(std::fs::read_to_string(figcraft.join("state.json")).unwrap(), bad, "left as it is");
    }
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn figcrafts_config_dir_variable_counts_when_the_new_one_is_unset() {
    let root = root("old-env");
    let appdata = Some(root.join("appdata"));
    let old = settings_dir(&probe_env(&root, appdata.clone(), Some(("FIGCRAFT_CONFIG_DIR", "elsewhere"))));
    assert_eq!(old, Some(PathBuf::from("elsewhere")));
    assert_eq!(settings_dir(&probe_env(&root, appdata.clone(), Some(("FIGCRAFT_CONFIG_DIR", "")))), None, "empty: isolated");
    let new = settings_dir(&probe(&root, appdata, Some("mine")));
    assert_eq!(new, Some(PathBuf::from("mine")));
    let _ = std::fs::remove_dir_all(&root);
}
