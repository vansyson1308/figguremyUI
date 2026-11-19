//! The text layout cache: equal texts share a layout, unequal ones never do.

use std::sync::Arc;

use figcraft_doc::{LetterSpacing, TextData};

use crate::{clear_layout_cache, height_cached, layout, layout_cached, layout_generation};

#[test]
fn equal_texts_share_a_layout_and_unequal_ones_never_do() {
    let g0 = layout_generation();
    let t1 = TextData::new("Cached text");
    let a = layout_cached(&t1, Some(100.0), None);
    let b = layout_cached(&t1.clone(), Some(100.0), None);
    // Fonts loading elsewhere may clear the cache in between; then a fresh layout is right too.
    assert!(Arc::ptr_eq(&a, &b) || layout_generation() != g0, "a cache hit");
    // Same characters, family, style and size (the cheap fingerprint), different spacing.
    let mut t2 = t1.clone();
    t2.style.letter_spacing = LetterSpacing::Pixels(5.0);
    assert!(!Arc::ptr_eq(&a, &layout_cached(&t2, Some(100.0), None)));
    // Another box is another layout.
    assert!(!Arc::ptr_eq(&a, &layout_cached(&t1, Some(50.0), None)));
    // Texts that never equal themselves (NaN) are laid out each time without piling up.
    let mut nan = t1.clone();
    nan.paragraph_spacing = f64::NAN;
    for _ in 0..50 {
        let _ = layout_cached(&nan, None, None);
    }
}

#[test]
fn clearing_bumps_the_generation() {
    let g = layout_generation();
    clear_layout_cache();
    assert!(layout_generation() > g);
}

#[test]
fn a_cached_height_is_the_height_of_the_layout() {
    let mut t = TextData::new("A cached height, long enough to wrap onto more than one line at the narrow width");
    for w in [40.0, 120.0, 1000.0] {
        let want = layout(&t, Some(w), None).height;
        assert_eq!(height_cached(&t, w), want, "width {w}");
        assert_eq!(height_cached(&t, w), want, "width {w}, cached");
    }
    // Another text at a width it was measured at is measured anew.
    t.style.font_size *= 2.0;
    assert_eq!(height_cached(&t, 120.0), layout(&t, Some(120.0), None).height);
    clear_layout_cache();
    assert_eq!(height_cached(&t, 120.0), layout(&t, Some(120.0), None).height);
}
