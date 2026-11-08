//! `cargo xtask fv-scoreboard` - formal-verification scoreboard generated from
//! source (#12904), so the numbers are not hand-counted.
//!
//! Read-only on product source; deterministic (sorted walk, no clocks, no
//! network, no subprocess). Advisory only: it never gates.
//!
//! Definitions (do not change silently):
//! - a real harness is a `#[kani::proof]` attribute that begins a line after
//!   comments and string/char literals are masked, in a `.rs` file under
//!   `crates/` or `xtask/`. Directories named `target`, `fixtures`, `testdata`
//!   or `test_data` are skipped. Mentions inside strings (including raw
//!   strings) and comments are not counted.
//! - a harness module is a distinct file holding at least one real harness.
//! - `kani::cover!`, `kani::assume(` and `#[kani::unwind(` are counted
//!   anywhere in masked text of the same files.
//! - the walk covers the checkout's `.rs` files, not `git ls-files`; untracked
//!   `.rs` files under the roots would be counted.
//! - known undercounts, deliberately not solved: `#[cfg(kani)] #[kani::proof]`
//!   or two attributes on one line, `cfg_attr(kani, kani::proof)` and
//!   `proof_for_contract` are not counted. `macro_rules!` bodies are scanned
//!   like code, so a harness inside one is counted.
//! - parity with `kani_lane::discover_harnesses` (the lane runner): it matches
//!   any line starting `#[kani::proof` (so `proof_for_contract` too), requires
//!   a following `fn`, and only skips `r#"` raw strings; this scan masks all
//!   comments and string/char literals but needs the exact `#[kani::proof]`.
//!   They agree on ordinary harness text (see the parity test).
//! - whether a lane is a required check is a repository setting and is never
//!   derived from source.

use std::fs;
use std::path::{Path, PathBuf};

use crate::normalize_path;

const ROOTS: [&str; 2] = ["crates", "xtask"];
const SKIP_DIRS: [&str; 4] = ["target", "fixtures", "testdata", "test_data"];
const REPORT_PATH: &str = "target/ripr/reports/fv-scoreboard.md";
/// (path, pending note). A pending suite is expected to be absent until the
/// named change lands; every other suite must exist (see the suites test).
const SUITES: [(&str, Option<&str>); 4] = [
    ("crates/ripr/tests/stage_metamorphic.rs", None),
    ("xtask/src/output_properties.rs", None),
    ("crates/ripr/src/analysis/diff/parse.rs", None),
    (
        "crates/ripr/src/analysis/diff/differential_tests.rs",
        Some("pending #13129"),
    ),
];

#[derive(Debug, Default, PartialEq, Eq)]
struct KaniCounts {
    harnesses: usize,
    modules: usize,
    covers: usize,
    assumes: usize,
    unwinds: usize,
}

fn blank(c: char) -> char {
    if c == '\n' { '\n' } else { ' ' }
}

/// Replaces comments, string literals and char literals with spaces
/// (newlines kept) so pattern scans see code only.
fn mask_non_code(source: &str) -> String {
    let chars: Vec<char> = source.chars().collect();
    let mut out = String::with_capacity(source.len());
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        if c == '/' && next == Some('/') {
            while i < chars.len() && chars[i] != '\n' {
                out.push(' ');
                i += 1;
            }
        } else if c == '/' && next == Some('*') {
            let mut depth = 0usize;
            while i < chars.len() {
                if chars[i] == '/' && chars.get(i + 1) == Some(&'*') {
                    depth += 1;
                    out.push_str("  ");
                    i += 2;
                } else if chars[i] == '*' && chars.get(i + 1) == Some(&'/') {
                    depth = depth.saturating_sub(1);
                    out.push_str("  ");
                    i += 2;
                    if depth == 0 {
                        break;
                    }
                } else {
                    out.push(blank(chars[i]));
                    i += 1;
                }
            }
        } else if let Some((start, hashes)) = raw_string_open(&chars, i) {
            // Raw string: ends at a quote followed by `hashes` hashes.
            let mut end = chars.len();
            let mut j = start;
            while j < chars.len() {
                if chars[j] == '"' && (0..hashes).all(|k| chars.get(j + 1 + k) == Some(&'#')) {
                    end = j + 1 + hashes;
                    break;
                }
                j += 1;
            }
            out.extend(chars[i..end].iter().map(|&ch| blank(ch)));
            i = end;
        } else if c == '"' {
            out.push(' ');
            i += 1;
            while i < chars.len() {
                let ch = chars[i];
                if ch == '\\' {
                    out.push(' ');
                    if let Some(&escaped) = chars.get(i + 1) {
                        out.push(blank(escaped));
                    }
                    i += 2;
                } else {
                    out.push(blank(ch));
                    i += 1;
                    if ch == '"' {
                        break;
                    }
                }
            }
        } else if c == '\'' {
            // Char literal (`'x'`, `'\n'`) versus lifetime (`'a`).
            let literal_len = if next == Some('\\') {
                // Skip the escaped char (`'`, `\`, `n`, `u`, `x`) before
                // looking for the closing quote; `\u{..}` / `\x41` tails
                // contain no quote.
                chars
                    .get(i + 3..)
                    .and_then(|rest| rest.iter().position(|&ch| ch == '\''))
                    .map(|p| p + 4)
            } else if chars.get(i + 2) == Some(&'\'') {
                Some(3)
            } else {
                None
            };
            match literal_len {
                Some(len) => {
                    out.extend(std::iter::repeat_n(' ', len));
                    i += len;
                }
                None => {
                    out.push(c);
                    i += 1;
                }
            }
        } else {
            out.push(c);
            i += 1;
        }
    }
    out
}

/// If a raw string literal opens at `i` (`r"`, `r#"`, `br#"`), returns the
/// index just after the opening quote and the hash count.
fn raw_string_open(chars: &[char], i: usize) -> Option<(usize, usize)> {
    if i > 0 && (chars[i - 1].is_alphanumeric() || chars[i - 1] == '_') {
        return None;
    }
    let mut j = i;
    if chars.get(j) == Some(&'b') {
        j += 1;
    }
    if chars.get(j) != Some(&'r') {
        return None;
    }
    j += 1;
    let mut hashes = 0;
    while chars.get(j) == Some(&'#') {
        hashes += 1;
        j += 1;
    }
    (chars.get(j) == Some(&'"')).then_some((j + 1, hashes))
}

/// Counts `token` where the preceding char is not an identifier char or `:`,
/// so `my_kani::assume(` and `foo::kani::cover!` are not counted.
fn count_token(masked: &str, token: &str) -> usize {
    masked
        .match_indices(token)
        .filter(|(at, _)| {
            !masked[..*at]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_alphanumeric() || c == '_' || c == ':')
        })
        .count()
}

fn count_file(source: &str) -> KaniCounts {
    let masked = mask_non_code(source);
    let harnesses = masked
        .lines()
        .filter(|line| line.trim_start().starts_with("#[kani::proof]"))
        .count();
    KaniCounts {
        harnesses,
        modules: usize::from(harnesses > 0),
        covers: count_token(&masked, "kani::cover!"),
        assumes: count_token(&masked, "kani::assume("),
        unwinds: count_token(&masked, "#[kani::unwind("),
    }
}

fn add(total: &mut KaniCounts, part: &KaniCounts) {
    total.harnesses += part.harnesses;
    total.modules += part.modules;
    total.covers += part.covers;
    total.assumes += part.assumes;
    total.unwinds += part.unwinds;
}

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries =
        fs::read_dir(dir).map_err(|err| format!("failed to read {}: {err}", dir.display()))?;
    for entry in entries {
        let path = entry
            .map_err(|err| format!("failed to read entry in {}: {err}", dir.display()))?
            .path();
        if path.is_dir() {
            let skipped = path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| SKIP_DIRS.contains(&name));
            if !skipped {
                rust_files(&path, out)?;
            }
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
    Ok(())
}

fn measure_kani(root: &Path) -> Result<KaniCounts, String> {
    let mut files = Vec::new();
    for sub in ROOTS {
        let dir = root.join(sub);
        if !dir.is_dir() {
            return Err(format!(
                "scan root {} is missing; refusing to report zero Kani counts as measured",
                dir.display()
            ));
        }
        rust_files(&dir, &mut files)?;
    }
    files.sort();
    let mut total = KaniCounts::default();
    for file in files {
        let source = fs::read_to_string(&file)
            .map_err(|err| format!("failed to read {}: {err}", file.display()))?;
        add(&mut total, &count_file(&source));
    }
    Ok(total)
}

/// Counts workflow step entries: list items starting `name:`, `uses:` or `run:`.
fn count_workflow_steps(workflow: &str) -> usize {
    workflow
        .lines()
        .map(str::trim_start)
        .filter(|line| {
            ["- name:", "- uses:", "- run:"]
                .iter()
                .any(|prefix| line.starts_with(prefix))
        })
        .count()
}

fn render(root: &Path) -> Result<String, String> {
    let kani = measure_kani(root)?;
    let mut md = String::new();
    md.push_str("# FV scoreboard (generated from source)\n\n");
    md.push_str("Status values: `measured (by reading)` = counted from source by `cargo xtask fv-scoreboard`; `not_measured` = not derivable from source or not run. Definitions are in `xtask/src/reports/fv_scoreboard.rs`.\n\n");
    md.push_str("Known undercounts (not solved): `#[cfg(kani)] #[kani::proof]` or two attributes on one line, `cfg_attr(kani, kani::proof)` and `proof_for_contract` are not counted; `macro_rules!` bodies are counted.\n\n");
    md.push_str("## Kani\n\n| Metric | Value | Status |\n|---|---|---|\n");
    let rows = [
        ("Real `#[kani::proof]` harnesses", kani.harnesses),
        ("Modules (files) containing harnesses", kani.modules),
        ("`kani::cover!`", kani.covers),
        ("`kani::assume(`", kani.assumes),
        ("`#[kani::unwind(`", kani.unwinds),
    ];
    for (label, value) in rows {
        md.push_str(&format!("| {label} | {value} | measured (by reading) |\n"));
    }
    md.push_str("| Kani lane is a required check | not_measured (settings) | not_measured |\n");
    md.push_str(
        "\n## Property suites\n\n| Suite | Present | `proptest!` blocks | Status |\n|---|---|---|---|\n",
    );
    for (suite, pending) in SUITES {
        match fs::read_to_string(root.join(suite)) {
            Ok(source) => {
                let blocks = mask_non_code(&source).matches("proptest!").count();
                md.push_str(&format!(
                    "| `{suite}` | present | {blocks} | measured (by reading) |\n"
                ));
            }
            Err(_) => {
                let note = pending.map_or(String::new(), |p| format!(" ({p})"));
                md.push_str(&format!(
                    "| `{suite}`{note} | file absent | n/a | not_measured |\n"
                ));
            }
        }
    }
    md.push_str("\n## Fuzz lane\n\n| Metric | Value | Status |\n|---|---|---|\n");
    match fs::read_to_string(root.join(".github/workflows/fuzz.yml")) {
        Ok(workflow) => md.push_str(&format!(
            "| `.github/workflows/fuzz.yml` steps | {} | measured (by reading) |\n",
            count_workflow_steps(&workflow)
        )),
        Err(_) => md.push_str("| `.github/workflows/fuzz.yml` steps | absent | not_measured |\n"),
    }
    Ok(md)
}

pub(crate) fn fv_scoreboard() -> Result<(), String> {
    let root = crate::repo_root()?;
    let md = render(&root)?;
    print!("{md}");
    let path = write_report(&root, &md)?;
    println!("wrote {}", normalize_path(&path));
    Ok(())
}

fn write_report(root: &Path, md: &str) -> Result<PathBuf, String> {
    let path = root.join(REPORT_PATH);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create {}: {err}", normalize_path(parent)))?;
    }
    fs::write(&path, md)
        .map_err(|err| format!("failed to write {}: {err}", normalize_path(&path)))?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_harness_is_counted() {
        let source = "#[cfg(kani)]\nmod p {\n    #[kani::proof]\n    #[kani::unwind(4)]\n    fn h() {\n        kani::assume(true);\n        kani::cover!(true);\n    }\n}\n";
        assert_eq!(
            count_file(source),
            KaniCounts {
                harnesses: 1,
                modules: 1,
                covers: 1,
                assumes: 1,
                unwinds: 1
            }
        );
    }

    #[test]
    fn string_and_comment_mentions_are_not_counted() {
        let source = "// #[kani::proof]\n/* \n#[kani::proof]\n*/\nconst A: &str = \"\n#[kani::proof]\n\";\nconst B: &str = r#\"\n#[kani::proof]\nkani::cover!(x)\n\"#;\nfn f<'a>(_: &'a str) { let _c = '\"'; }\n";
        assert_eq!(count_file(source), KaniCounts::default());
    }

    #[test]
    fn real_harness_after_literals_is_still_counted() {
        let source = "const A: &str = \"#[kani::proof]\";\n#[kani::proof]\nfn h() {}\n";
        assert_eq!(count_file(source).harnesses, 1);
    }

    #[test]
    fn escaped_char_literals_do_not_swallow_following_code() {
        let source = "fn f() { g('\\'', '\"'); let _b = '\\\\'; let _u = '\\u{41}'; let _x = '\\x41'; }\n#[kani::proof]\nfn h() {}\n";
        assert_eq!(count_file(source).harnesses, 1);
        let masked = mask_non_code(source);
        assert!(masked.contains("#[kani::proof]"));
        assert!(!masked.contains("41"));
    }

    #[test]
    fn char_array_with_quote_chars_does_not_hide_a_harness() {
        let source = "fn f(){ let _=['\\'','\"']; }\n#[kani::proof]\nfn h(){}\n";
        assert_eq!(count_file(source).harnesses, 1);
    }

    #[test]
    fn qualified_or_prefixed_tokens_are_not_counted() {
        let source = "fn f() { my_kani::assume(x); foo::kani::cover!(y); kani::assume(z); kani::cover!(w); }\n";
        let counts = count_file(source);
        assert_eq!((counts.assumes, counts.covers), (1, 1));
    }

    #[test]
    fn harness_count_agrees_with_lane_discovery_on_ordinary_text() {
        let text = "#[cfg(kani)]\nmod p {\n    #[kani::proof]\n    #[kani::unwind(2)]\n    fn a() {}\n\n    #[kani::proof]\n    fn b() {}\n}\nconst S: &str = r#\"\n#[kani::proof]\nfn fake() {}\n\"#;\n";
        let lane = crate::kani_lane::discover_harnesses(&[("f.rs".to_string(), text.to_string())]);
        assert_eq!(lane.len(), 2);
        assert_eq!(count_file(text).harnesses, lane.len());
    }

    fn temp_tree(name: &str) -> Result<PathBuf, String> {
        let root =
            std::env::temp_dir().join(format!("fv-scoreboard-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).map_err(|err| err.to_string())?;
        Ok(root)
    }

    #[test]
    fn missing_scan_root_is_an_error_not_zero_measured() -> Result<(), String> {
        let root = temp_tree("missing")?;
        fs::create_dir_all(root.join("crates")).map_err(|err| err.to_string())?;
        let result = render(&root);
        let _ = fs::remove_dir_all(&root);
        match result {
            Err(err) => {
                assert!(err.contains("missing"), "{err}");
                Ok(())
            }
            Ok(md) => Err(format!("expected an error, rendered: {md}")),
        }
    }

    #[test]
    fn render_and_report_path_are_relative_to_the_given_root_not_cwd() -> Result<(), String> {
        let root = temp_tree("rooted")?;
        fs::create_dir_all(root.join("crates/c")).map_err(|err| err.to_string())?;
        fs::create_dir_all(root.join("xtask")).map_err(|err| err.to_string())?;
        fs::write(
            root.join("crates/c/h.rs"),
            "#[kani::proof]\nfn a() {}\n#[kani::proof]\nfn b() {}\n",
        )
        .map_err(|err| err.to_string())?;
        let md = render(&root)?;
        assert!(
            md.contains("| Real `#[kani::proof]` harnesses | 2 |"),
            "{md}"
        );
        assert!(
            md.contains("| Modules (files) containing harnesses | 1 |"),
            "{md}"
        );
        let path = write_report(&root, &md)?;
        assert_eq!(path, root.join(REPORT_PATH));
        assert!(path.is_file());
        let _ = fs::remove_dir_all(&root);
        Ok(())
    }

    #[test]
    fn every_configured_suite_exists_or_is_marked_pending() -> Result<(), String> {
        let root = crate::repo_root()?;
        for (suite, pending) in SUITES {
            assert!(
                root.join(suite).is_file() || pending.is_some(),
                "suite {suite} is absent and not marked pending"
            );
        }
        let md = render(&root)?;
        let any_absent = SUITES.iter().any(|(s, _)| !root.join(s).is_file());
        assert_eq!(md.contains("file absent"), any_absent);
        Ok(())
    }

    #[test]
    fn workflow_steps_are_counted() {
        let yml =
            "steps:\n  - uses: a\n    with:\n      x: 1\n  - name: n\n    run: r\n  - run: z\n";
        assert_eq!(count_workflow_steps(yml), 3);
    }
}
