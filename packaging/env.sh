# shellcheck shell=bash
# Shared setup for the packaging scripts. Source it: `. "$(dirname "$0")/../env.sh"`.
#
# Exports:
#   ROOT                  workspace root
#   VERSION               [workspace.package] version from Cargo.toml (override: FIGGUREMYUI_VERSION)
#   DIST                  output directory for release artifacts (default: $ROOT/dist/release)
#   FIGGUREMYUI_BUILD_SHA git commit baked into the binaries (apps/figcraft/build.rs)
#   FIGGUREMYUI_BUILD_DATE UTC build date, YYYY-MM-DD
#   (The old FIGCRAFT_VERSION, FIGCRAFT_BUILD_SHA and FIGCRAFT_BUILD_DATE are read when the new
#   names are unset.)
#   CARGO_TARGET_DIR      cargo's target dir (default: $ROOT/target)
# Sets (not exported):
#   BINS                  the binaries to build and ship: figguremyui, plus figguremyui-cli when the
#                         workspace has apps/figcraft-cli
#   HAS_CLI               1 when figguremyui-cli is part of the build, else 0
#   CARGO_PKGS            `-p <package>` arguments for cargo build (the packages keep their
#                         `figcraft*` names: `figcraft` builds figguremyui, `figcraft-cli` figguremyui-cli)

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export ROOT

# The version lives in exactly one place: `[workspace.package] version` in the root Cargo.toml.
# (`cargo xtask version` prints the same thing; awk avoids compiling xtask here.)
workspace_version() {
  awk '
    /^\[/ { in_pkg = ($0 == "[workspace.package]") ; next }
    in_pkg && $1 == "version" { gsub(/[" ]/, "", $3); print $3; exit }
  ' "$ROOT/Cargo.toml"
  return $?
}

VERSION="${FIGGUREMYUI_VERSION:-${FIGCRAFT_VERSION:-$(workspace_version)}}"
if [[ -z "$VERSION" ]]; then
  echo "error: could not read [workspace.package] version from $ROOT/Cargo.toml" >&2
  exit 1
fi
export VERSION

DIST="${DIST:-$ROOT/dist/release}"
mkdir -p "$DIST"
export DIST

FIGGUREMYUI_BUILD_SHA="${FIGGUREMYUI_BUILD_SHA:-${FIGCRAFT_BUILD_SHA:-}}"
if [[ -z "$FIGGUREMYUI_BUILD_SHA" ]]; then
  FIGGUREMYUI_BUILD_SHA="$(git -C "$ROOT" rev-parse HEAD 2>/dev/null || true)"
fi
export FIGGUREMYUI_BUILD_SHA
export FIGGUREMYUI_BUILD_DATE="${FIGGUREMYUI_BUILD_DATE:-${FIGCRAFT_BUILD_DATE:-$(date -u +%Y-%m-%d)}}"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"

# The headless CLI ships next to the app once apps/figcraft-cli exists.
BINS=(figguremyui)
HAS_CLI=0
if [[ -f "$ROOT/apps/figcraft-cli/Cargo.toml" ]]; then
  BINS+=(figguremyui-cli)
  HAS_CLI=1
fi
CARGO_PKGS=(-p figcraft)
if [[ "$HAS_CLI" = 1 ]]; then CARGO_PKGS+=(-p figcraft-cli); fi

# The binary used for a `--version` smoke test: the CLI when shipped, else the app (which answers
# --version before opening a window).
smoke_bin() {
  if [[ "$HAS_CLI" = 1 ]]; then echo figguremyui-cli; else echo figguremyui; fi
  return 0
}

# Emit a GitHub Actions warning (plain stderr outside Actions).
# Run a command up to five times, 15 s apart: for steps that ask a remote service (Apple's
# notarization ticket service drops connections now and then).
retry() {
  local attempt
  for attempt in 1 2 3 4 5; do
    "$@" && return 0
    [[ "$attempt" = 5 ]] || { echo "retrying ($attempt/5): $*" >&2; sleep 15; }
  done
  return 1
}

warn() {
  if [[ -n "${GITHUB_ACTIONS:-}" ]]; then echo "::warning::$*"; else echo "warning: $*" >&2; fi
  return 0
}

# Copy licence and readme files that exist into a package directory.
copy_docs() {
  local dest="$1" f
  for f in README.md LICENSE LICENSE-MIT LICENSE-APACHE NOTICE COPYRIGHT; do
    if [[ -f "$ROOT/$f" ]]; then cp "$ROOT/$f" "$dest/"; fi
  done
  copy_font_licences "$dest"
  return $?
}

# Builds made with craft-fonts (CRAFT_FONTS_DIR, all official releases) embed its fonts, which
# are OFL-1.1: ship each font's licence as OFL-<family-dir>.txt. No-op without CRAFT_FONTS_DIR.
copy_font_licences() {
  local dest="$1" lic fam
  [[ -n "${CRAFT_FONTS_DIR:-}" ]] || return 0
  for lic in "$CRAFT_FONTS_DIR"/fonts/*/OFL.txt; do
    [[ -f "$lic" ]] || continue
    fam="$(basename "$(dirname "$lic")")"
    cp "$lic" "$dest/OFL-$fam.txt"
  done
}

# Portable SHA-256 of a file (prints just the hash).
sha256() {
  local file="$1"
  if command -v sha256sum >/dev/null; then sha256sum "$file" | cut -d' ' -f1; else shasum -a 256 "$file" | cut -d' ' -f1; fi
  return $?
}
