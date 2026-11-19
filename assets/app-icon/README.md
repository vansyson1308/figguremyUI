# Figgure My UI app icon

A **screen card with a check**: a cream outline card with two text lines (the interface being
designed) and an orange card with a check mark over its corner (the design verified), on a navy
tile. "Design and verify", in four simple shapes.

## Palette

Exactly three colours:

| Colour | Hex | Used for |
|---|---|---|
| Navy | `#1d2440` | the full-bleed field, and the check mark |
| Cream | `#f3eee3` | the screen card and its text lines |
| Figgure orange (brand colour) | `#ff7a4d` | the check card; also the app's brand colour in its interface (`BRAND` in `crates/ui-egui/src/theme.rs`) |

Navy on the orange reaches 5.8:1, so the check mark (and any text the app draws on the brand
colour) passes WCAG AA's 4.5:1.

## Geometry

A 512-unit tile (`viewBox="0 0 512 512"`), rounded square with `rx=112`, no border. The macOS
renders pad it onto Apple's 824/1024 icon grid; Windows and Linux use the tile edge to edge, with
its rounded corners, like the other Crafting Apps.

## Provenance

Original work made for Figgure My UI from simple geometric shapes; no third-party material, no
other product's icon. License: see `LICENSE.txt`. It replaces the chameleon icon of FigCraft, which
this app is a fork of (that drawing stays in the repository history only).

## Files

- `figguremyui.svg`: canonical master.
- `figguremyui-small.svg`: the same drawing with heavier strokes, for small sizes; also the
  hicolor scalable icon.
- `figguremyui-<size>.png` (16–1024): renders on the macOS grid; `packaging/macos/package.sh`
  builds the `.icns` from them.
- `figguremyui.ico`: Windows icon, 16–256 px, embedded in the `.exe` by `apps/figcraft/build.rs`.
- `hicolor/<size>/apps/io.github.vansyson1308.figguremyui.png` (16–512) and
  `hicolor/scalable/...svg`: Linux icon theme; the 256 px one is also the runtime window icon off
  macOS, the 128 px one the update prompt's.

## Regenerate

Replace `figguremyui.svg` (and `figguremyui-small.svg`), then run `cargo xtask icon`. It needs
`resvg` (`cargo install resvg`).
