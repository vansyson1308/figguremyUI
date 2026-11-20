//! The app's identity: the updater looks for this fork's releases only, under its own name, and
//! a repository with no releases yet is "up to date", not an error.

use crate::{APP_ID, UPDATE_APP};

#[test]
fn the_updater_knows_the_app_by_its_own_name() {
    assert_eq!((UPDATE_APP.id, UPDATE_APP.name, UPDATE_APP.repo), ("figguremyui", "Figgure My UI", "vansyson1308/figguremyUI"));
    assert_eq!(UPDATE_APP.developer, "Figgure My UI contributors");
    assert_eq!(UPDATE_APP.bundle_id(), APP_ID);
    assert_eq!(APP_ID, "io.github.vansyson1308.figguremyui");
    assert!(UPDATE_APP.former_ids.is_empty() && UPDATE_APP.former_names.is_empty(), "never upstream's assets or installs");
    assert_eq!(UPDATE_APP.stems().collect::<Vec<_>>(), ["figguremyui"]);
    let b = crate::theme::BRAND;
    assert_eq!(UPDATE_APP.accent, Some([b.r(), b.g(), b.b()]), "the prompt's accent is the brand colour");
    assert!(UPDATE_APP.icon_png.is_some_and(|png| png.starts_with(b"\x89PNG")));
}

#[test]
fn no_releases_yet_is_nothing_to_offer_not_an_error() {
    // What GitHub answers for a repository without releases: an empty list.
    assert_eq!(craft_update::parse_github_release_list("[]", UPDATE_APP.repo, false).map(|r| r.is_none()), Ok(true));
}
