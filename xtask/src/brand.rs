//! The old product name kept out of what people see (`cargo xtask brand`).
//!
//! This build is Figgure My UI, a fork of FigCraft. The word `FigCraft` (or `Figcraft`, letter case
//! as written) must not come back into user-visible text:
//! the UI catalogs (`crates/ui-egui/locales/*.toml`, keys and values), Rust string literals in
//! shipped source under `apps/` and `crates/*/src/` (test files, `tests/` folders and
//! `#[cfg(test)]` items are skipped), the web app's page and assets, the packaging metadata, the
//! README and `docs/**/*.md`.
//!
//! Allowed: the lowercase and upper-case machine identifiers that stay (crate names
//! `figcraft-*`, the `.figcraft` extension, `figcraft.widget…` keys, old `FIGCRAFT_…` variable
//! names; the match is case-sensitive by design), revision-history table rows in docs, the files
//! in [`ALLOWED_FILES`], and any line that carries a marker saying why: `brand: compat` (an old
//! name that must stay) or `brand: attribution` (credit to the upstream project). A marker on a
//! comment-only line also covers the line below it.

use std::path::{Path, PathBuf};

/// Markers that allow the old name on a line.
const MARKERS: [&str; 2] = ["brand: compat", "brand: attribution"];

/// Files that may name the old product, and why (printed with the result).
const ALLOWED_FILES: &[(&str, &str)] = &[
    ("NOTICE", "keeps upstream's notice and says where the fork comes from"),
    ("LICENSE-MIT, LICENSE-APACHE", "the licence texts, unchanged"),
    ("ATTRIBUTION.md", "credits upstream's work and assets"),
    ("docs/migration.md", "lists the old names next to the new ones"),
    ("docs/design/book-to-product.md", "a research note written before the rename"),
];

/// How a file is read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// Only string literals count, outside test items.
    Rust,
    /// Every line except `#` comment lines (TOML catalogs).
    Toml,
    /// Every line; table rows under a "… history" heading are allowed.
    Markdown,
    /// Every line.
    Text,
}

/// Scans the repository under `root`; prints each hit as `file:line: text` and returns whether
/// there were none.
pub fn check(root: &Path) -> bool {
    let mut hits = Vec::new();
    for (path, kind) in files(root) {
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        let name = path.strip_prefix(root).map(|r| r.to_string_lossy().replace('\\', "/")).unwrap_or_default();
        if ALLOWED_FILES.iter().any(|(f, _)| *f == name) {
            continue;
        }
        for (line, source) in scan(&text, kind) {
            hits.push(format!("{name}:{line}: {}", source.trim()));
        }
    }
    eprintln!("brand: allowed by file:");
    for (f, why) in ALLOWED_FILES {
        eprintln!("  {f}: {why}");
    }
    eprintln!("brand: also allowed: lowercase/upper-case identifiers (the match is case-sensitive: `FigCraft`, `Figcraft`),");
    eprintln!("  revision-history table rows, and lines marked `brand: compat` or `brand: attribution`");
    if hits.is_empty() {
        eprintln!("brand: ok");
        return true;
    }
    eprintln!("brand: the old product name in user-visible text ({} lines); say Figgure My UI, or mark why it stays:", hits.len());
    for h in &hits {
        eprintln!("{h}");
    }
    false
}

/// The files the guard reads, with how to read each.
fn files(root: &Path) -> Vec<(PathBuf, Kind)> {
    let mut out = Vec::new();
    let mut add = |dir: &Path, pick: &dyn Fn(&Path) -> Option<Kind>| {
        let mut found = Vec::new();
        walk(dir, &mut |p| {
            if let Some(kind) = pick(p) {
                found.push((p.to_path_buf(), kind));
            }
        });
        found.sort_by(|a, b| a.0.cmp(&b.0));
        out.extend(found);
    };
    add(&root.join("crates/ui-egui/locales"), &|p| (ext(p) == "toml").then_some(Kind::Toml));
    add(&root.join("apps"), &|p| match ext(p) {
        "rs" => (!test_source(p)).then_some(Kind::Rust),
        // The web app's page and its other assets; the manifests are not shown to anyone.
        _ if p.file_name().is_some_and(|f| f == "Cargo.toml") => None,
        _ => p.components().any(|c| c.as_os_str() == "figcraft-web").then_some(Kind::Text),
    });
    if let Ok(rd) = std::fs::read_dir(root.join("crates")) {
        let mut crates: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
        crates.sort();
        for c in crates {
            add(&c.join("src"), &|p| (ext(p) == "rs" && !test_source(p)).then_some(Kind::Rust));
        }
    }
    add(&root.join("packaging"), &|_| Some(Kind::Text));
    add(&root.join("docs"), &|p| (ext(p) == "md").then_some(Kind::Markdown));
    out.push((root.join("README.md"), Kind::Markdown));
    out
}

fn ext(p: &Path) -> &str {
    p.extension().and_then(|e| e.to_str()).unwrap_or("")
}

/// A test file: in a `tests` folder, or named like `tests.rs` / `foo_tests.rs`.
fn test_source(p: &Path) -> bool {
    p.components().any(|c| c.as_os_str() == "tests") || p.file_stem().is_some_and(|s| s.to_string_lossy().contains("tests"))
}

fn walk(dir: &Path, f: &mut dyn FnMut(&Path)) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let Ok(ft) = e.file_type() else { continue };
        let p = e.path();
        if ft.is_dir() && !ft.is_symlink() {
            walk(&p, f);
        } else if ft.is_file() {
            f(&p);
        }
    }
}

/// `(line number, source line)` for each line of `text` that names the old product where it isn't
/// allowed.
fn scan(text: &str, kind: Kind) -> Vec<(usize, String)> {
    let lines: Vec<&str> = text.lines().collect();
    let marked = |i: usize| {
        let here = lines.get(i).is_some_and(|l| MARKERS.iter().any(|m| l.contains(m)));
        let above = i.checked_sub(1).and_then(|j| lines.get(j)).is_some_and(|l| {
            let t = l.trim_start();
            (t.starts_with("//") || t.starts_with('#') || t.starts_with("<!--")) && MARKERS.iter().any(|m| t.contains(m))
        });
        here || above
    };
    let mut flagged: Vec<usize> = match kind {
        Kind::Rust => rust_literals(text).into_iter().filter(|(_, s)| names_old_product(s)).map(|(l, _)| l).collect(),
        Kind::Toml => lines.iter().enumerate().filter(|(_, l)| !l.trim_start().starts_with('#') && names_old_product(l)).map(|(i, _)| i).collect(),
        Kind::Text => lines.iter().enumerate().filter(|(_, l)| names_old_product(l)).map(|(i, _)| i).collect(),
        Kind::Markdown => {
            let mut history = false;
            let mut out = Vec::new();
            for (i, l) in lines.iter().enumerate() {
                if l.starts_with('#') {
                    history = l.to_ascii_lowercase().contains("history");
                }
                if !(history && l.starts_with('|')) && names_old_product(l) {
                    out.push(i);
                }
            }
            out
        }
    };
    flagged.dedup();
    flagged.into_iter().filter(|&i| !marked(i)).map(|i| (i + 1, lines.get(i).copied().unwrap_or_default().to_string())).collect()
}

/// Whether `s` names the old product: the word `FigCraft` or `Figcraft` (letter case as written,
/// also inflected: "FigCraftin", "FigCraft's"), not inside a longer word. Lowercase and upper-case
/// machine identifiers (`figcraft-engine`, `.figcraft`, `FIGCRAFT_…`) are not the name people read.
pub(crate) fn names_old_product(s: &str) -> bool {
    ["FigCraft", "Figcraft"]
        .iter()
        .any(|word| s.match_indices(word).any(|(i, _)| !s.get(..i).and_then(|before| before.chars().next_back()).is_some_and(char::is_alphanumeric)))
}

/// The text of every string literal in Rust `source` outside `#[cfg(test)]` items (and none for a
/// file that is `#![cfg(test)]`), split per line: `(0-based line, text on that line)`.
fn rust_literals(source: &str) -> Vec<(usize, String)> {
    let mut lx = Lexer { c: source.chars().collect(), i: 0, line: 0, depth: 0, test_item: None, out: Vec::new() };
    while lx.i < lx.c.len() {
        if lx.starts("#![cfg(test)]") {
            return Vec::new();
        }
        lx.step();
    }
    lx.out
}

/// A small Rust lexer: enough to tell string literals from comments, char literals and lifetimes,
/// and to follow bracket depth (to skip `#[cfg(test)]` items).
struct Lexer {
    c: Vec<char>,
    i: usize,
    line: usize,
    depth: usize,
    /// While skipping a test item: the bracket depth it started at.
    test_item: Option<usize>,
    out: Vec<(usize, String)>,
}

impl Lexer {
    fn at(&self, i: usize) -> char {
        self.c.get(i).copied().unwrap_or('\0')
    }

    fn starts(&self, s: &str) -> bool {
        self.starts_at(self.i, s)
    }

    fn starts_at(&self, i: usize, s: &str) -> bool {
        s.chars().enumerate().all(|(k, ch)| self.at(i + k) == ch)
    }

    /// Reads one token (or one character) at `i`.
    fn step(&mut self) {
        let ch = self.at(self.i);
        if ch == '\n' {
            self.line += 1;
            self.i += 1;
        } else if self.starts("//") {
            while self.i < self.c.len() && self.at(self.i) != '\n' {
                self.i += 1;
            }
        } else if self.starts("/*") {
            self.block_comment();
        } else if self.starts("#[cfg(test)]") {
            self.i += "#[cfg(test)]".len();
            self.test_item = self.test_item.or(Some(self.depth));
        } else if ch == '"' {
            let start = self.i + 1;
            let mut end = start;
            while end < self.c.len() && self.at(end) != '"' {
                end += if self.at(end) == '\\' { 2 } else { 1 };
            }
            let end = end.min(self.c.len());
            self.take(start, end);
            self.i = end + 1;
        } else if ch == '\'' {
            self.char_or_lifetime();
        } else if ch.is_alphabetic() || ch == '_' {
            self.word();
        } else {
            self.punct(ch);
            self.i += 1;
        }
    }

    /// A (nested) block comment.
    fn block_comment(&mut self) {
        let mut nest = 0usize;
        while self.i < self.c.len() {
            if self.starts("/*") {
                nest += 1;
                self.i += 2;
            } else if self.starts("*/") {
                nest = nest.saturating_sub(1);
                self.i += 2;
                if nest == 0 {
                    return;
                }
            } else {
                if self.at(self.i) == '\n' {
                    self.line += 1;
                }
                self.i += 1;
            }
        }
    }

    /// A char literal ('a', '\n', '"'), else a lifetime or label.
    fn char_or_lifetime(&mut self) {
        if self.at(self.i + 1) == '\\' {
            self.i += 2;
            while self.i < self.c.len() && self.at(self.i) != '\'' && self.at(self.i) != '\n' {
                self.i += 1;
            }
            self.i += 1;
        } else if self.at(self.i + 2) == '\'' {
            self.i += 3;
        } else {
            self.i += 1;
        }
    }

    /// An identifier, or the prefix of a raw string (`r"…"`, `r#"…"#`, `br…`, `cr…`). A `b'x'`
    /// byte literal is left to the char-literal arm.
    fn word(&mut self) {
        let start = self.i;
        while self.i < self.c.len() && (self.at(self.i).is_alphanumeric() || self.at(self.i) == '_') {
            self.i += 1;
        }
        let word: String = self.c.get(start..self.i).map(|w| w.iter().collect()).unwrap_or_default();
        if !matches!(word.as_str(), "r" | "br" | "cr") {
            return;
        }
        let mut hashes = 0;
        while self.at(self.i + hashes) == '#' {
            hashes += 1;
        }
        if self.at(self.i + hashes) != '"' {
            return;
        }
        let body = self.i + hashes + 1;
        let close: String = std::iter::once('"').chain(std::iter::repeat_n('#', hashes)).collect();
        let mut j = body;
        while j < self.c.len() && !self.starts_at(j, &close) {
            j += 1;
        }
        self.take(body, j);
        self.i = j + close.chars().count();
    }

    /// Brackets and item ends: a test item ends with its block, at a `;` or `,` at its own depth,
    /// or when its enclosing block ends.
    fn punct(&mut self, ch: char) {
        match ch {
            '{' | '(' | '[' => self.depth += 1,
            '}' | ')' | ']' => {
                self.depth = self.depth.saturating_sub(1);
                if self.test_item.is_some_and(|d| (ch == '}' && self.depth == d) || self.depth < d) {
                    self.test_item = None;
                }
            }
            ';' | ',' if self.test_item == Some(self.depth) => self.test_item = None,
            _ => {}
        }
    }

    /// Keeps the literal's characters from `from` to `to` (exclusive), one entry per line, unless
    /// they are in a test item; counts its lines either way.
    fn take(&mut self, from: usize, to: usize) {
        let keep = self.test_item.is_none();
        let mut cur = String::new();
        for k in from..to {
            let ch = self.at(k);
            if ch == '\n' {
                if keep && !cur.is_empty() {
                    self.out.push((self.line, std::mem::take(&mut cur)));
                }
                cur.clear();
                self.line += 1;
            } else {
                cur.push(ch);
            }
        }
        if keep && !cur.is_empty() {
            self.out.push((self.line, cur));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_name_is_found_but_identifiers_are_not() {
        for hit in ["FigCraft", "Figcraft file", "(FigCraft)", "FigCraftin", "FigCraft's", "FigCraft가", "About FigCraft"] {
            assert!(names_old_product(hit), "{hit}");
        }
        for ok in [
            "figcraft-engine",
            "figcraft_engine::cmd",
            "figcraft.thumbnail",
            "Untitled.figcraft",
            "FIGCRAFT_CONFIG_DIR",
            "figcraft://join",
            "Figgure My UI",
            "myFigCraft",
            "",
        ] {
            assert!(!names_old_product(ok), "{ok}");
        }
    }

    #[test]
    fn rust_counts_string_literals_only() {
        let src = "// FigCraft in a comment\n/* FigCraft /* nested */ still */\nlet a = \"Welcome to FigCraft\";\nlet b = 'x'; fn f<'a>(s: &'a str) {}\nlet c = r#\"raw \"FigCraft\" \"#;\nlet d = '\"'; let e = \"ok\";\n";
        let lines: Vec<usize> = scan(src, Kind::Rust).into_iter().map(|(l, _)| l).collect();
        assert_eq!(lines, vec![3, 5]);
    }

    #[test]
    fn rust_skips_test_items_and_test_files() {
        let src = "fn shipped() -> &'static str { \"one\" }\n#[cfg(test)]\nmod tests {\n    fn t() { let _ = \"FigCraft\"; }\n}\nfn later() -> &'static str { \"FigCraft\" }\n#[cfg(test)]\nconst X: &str = \"FigCraft\";\nconst Y: &str = \"FigCraft\";\n";
        let lines: Vec<usize> = scan(src, Kind::Rust).into_iter().map(|(l, _)| l).collect();
        assert_eq!(lines, vec![6, 9]);
        assert!(scan("#![cfg(test)]\nconst A: &str = \"FigCraft\";\n", Kind::Rust).is_empty());
        assert!(test_source(Path::new("crates/x/src/foo_tests.rs")));
        assert!(test_source(Path::new("apps/figcraft-cli/tests/it/main.rs")));
        assert!(!test_source(Path::new("crates/x/src/testing.rs")));
    }

    #[test]
    fn a_marker_allows_its_line_and_the_line_below_a_comment() {
        let src =
            "let a = \"Based on FigCraft\"; // brand: attribution\n// brand: compat (the old name)\nlet b = \"FigCraft\";\nlet c = \"FigCraft\";\n";
        let lines: Vec<usize> = scan(src, Kind::Rust).into_iter().map(|(l, _)| l).collect();
        assert_eq!(lines, vec![4]);
        let toml =
            "# FigCraft in a comment\n\"Based on FigCraft\" = \"Based on FigCraft\" # brand: attribution\n\"About FigCraft\" = \"About FigCraft\"\n";
        let lines: Vec<usize> = scan(toml, Kind::Toml).into_iter().map(|(l, _)| l).collect();
        assert_eq!(lines, vec![3]);
    }

    #[test]
    fn markdown_allows_revision_history_rows() {
        let md = "# FigCraft guide\n| a | FigCraft |\n## Revision history\n| 2026-01-01 | FigCraft 1.0 |\nFigCraft prose under it\n## Next\n| FigCraft |\n";
        let lines: Vec<usize> = scan(md, Kind::Markdown).into_iter().map(|(l, _)| l).collect();
        assert_eq!(lines, vec![1, 2, 5, 7]);
    }
}
