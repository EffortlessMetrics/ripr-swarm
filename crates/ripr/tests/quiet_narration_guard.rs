//! Source-scan guard: stderr narration literals must go through `cli/progress.rs`
//! (#12103). #11955 owns which lines count as narration.
//!
//! The catalog runtime half lives in `cli_progress.rs` because a new
//! process-spawn site here would need a process-policy allowlist row, which
//! this slice is forbidden to touch.

use std::path::{Path, PathBuf};

const SINK: &str = "cli/progress.rs";

struct Exemption {
    relative: &'static str,
    reason: &'static str,
}

/// Live emitters of the narration literals that are not the shared sink.
/// Keep this list short; each row needs a reason.
const EXEMPTIONS: &[Exemption] = &[
    Exemption {
        relative: "lsp/progress_stages.rs",
        reason: "reads `ripr progress:` lines from the CLI sink; it does not emit them",
    },
    Exemption {
        relative: "cli/commands/agent.rs",
        reason: "existing `ripr: wrote` eprintln sites already honor progress_silenced (#11598); commands/agent.rs is contended and out of this slice",
    },
];

fn crate_src() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn walk_rust_files(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries =
        std::fs::read_dir(dir).map_err(|error| format!("read {}: {error}", dir.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("read {}: {error}", dir.display()))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|error| format!("stat {}: {error}", path.display()))?;
        if file_type.is_dir() {
            walk_rust_files(&path, files)?;
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) != Some("rs") {
            continue;
        }
        files.push(path);
    }
    Ok(())
}

fn relative_src(path: &Path) -> Result<String, String> {
    let src = crate_src();
    path.strip_prefix(&src)
        .map(|rel| rel.to_string_lossy().replace('\\', "/"))
        .map_err(|_prefix| format!("{} is not under {}", path.display(), src.display()))
}

fn skip_file(relative: &str) -> bool {
    let name = relative.rsplit('/').next().unwrap_or(relative);
    relative == SINK
        || name.contains("tests")
        || EXEMPTIONS.iter().any(|row| row.relative == relative)
}

fn strip_cfg_test_items(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut rest = src;
    while !rest.is_empty() {
        if let Some(index) = rest.find("#[cfg(test)]") {
            out.push_str(&rest[..index]);
            rest = &rest[index + "#[cfg(test)]".len()..];
            rest = skip_ws(rest);
            while rest.starts_with("#[") {
                rest = skip_attribute(rest);
                rest = skip_ws(rest);
            }
            rest = skip_item(rest);
            continue;
        }
        out.push_str(rest);
        break;
    }
    out
}

fn skip_ws(src: &str) -> &str {
    src.trim_start()
}

fn skip_attribute(src: &str) -> &str {
    if !src.starts_with("#[") {
        return src;
    }
    let body = &src[2..];
    match find_matching(body, '[', ']') {
        Some(end) => &body[end + 1..],
        None => "",
    }
}

fn skip_item(src: &str) -> &str {
    if let Some(brace) = src.find('{') {
        if let Some(semi) = src.find(';')
            && semi < brace
        {
            return src.get(semi + 1..).unwrap_or("");
        }
        let body = &src[brace + 1..];
        return match find_matching(body, '{', '}') {
            Some(end) => &body[end + 1..],
            None => "",
        };
    }
    match src.find(';') {
        Some(end) => src.get(end + 1..).unwrap_or(""),
        None => "",
    }
}

fn find_matching(src: &str, open: char, close: char) -> Option<usize> {
    let mut depth = 1u32;
    let mut in_string = false;
    let mut in_char = false;
    let mut escaped = false;
    let mut bytes = 0usize;
    for ch in src.chars() {
        let start = bytes;
        bytes += ch.len_utf8();
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        if in_char {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '\'' {
                in_char = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '\'' => in_char = true,
            c if c == open => depth = depth.saturating_add(1),
            c if c == close => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Some(start);
                }
            }
            _ => {}
        }
    }
    None
}

fn macro_calls<'a>(src: &'a str, name: &str) -> Vec<&'a str> {
    let needle = format!("{name}!");
    let mut calls = Vec::new();
    let mut rest = src;
    while let Some(index) = rest.find(&needle) {
        let after = &rest[index + needle.len()..];
        let after = skip_ws(after);
        if !after.starts_with('(') {
            rest = &rest[index + needle.len()..];
            continue;
        }
        let body = &after[1..];
        match find_matching(body, '(', ')') {
            Some(end) => {
                calls.push(&body[..end]);
                rest = &body[end + 1..];
            }
            None => break,
        }
    }
    calls
}

fn writes_stderr(args: &str) -> bool {
    let head = args.split(',').next().unwrap_or(args);
    head.contains("stderr")
}

fn contains_narration_literal(args: &str) -> bool {
    args.contains("\"ripr progress:")
        || args.contains("\"ripr: wrote ")
        || args.contains("\"wrote ")
}

/// Production-site scan: prefix constants, formatters, and stderr macros.
/// A `format!("{PROGRESS_PREFIX}...")` / `eprintln!("{line}")` split is a
/// hit because the constant left the sink (#12103 Devin review). Whole-file
/// string search is avoided: `#[cfg(test)]` stripping is item-shaped, so
/// leftover test asserts can still mention the prefix as expected output.
fn production_violations(relative: &str, production: &str) -> Vec<String> {
    let mut violations = Vec::new();
    if production.contains("PROGRESS_PREFIX") {
        violations.push(format!(
            "{relative}: uses PROGRESS_PREFIX outside {SINK}; keep formatting in the sink"
        ));
    }
    if production.contains("WROTE_PREFIX") {
        violations.push(format!(
            "{relative}: uses WROTE_PREFIX outside {SINK}; keep formatting in the sink"
        ));
    }
    for args in macro_calls(production, "format") {
        if contains_narration_literal(args) {
            violations.push(format!(
                "{relative}: format! builds a narration literal; route it through {SINK}"
            ));
        }
    }
    for args in macro_calls(production, "eprintln") {
        if contains_narration_literal(args) {
            violations.push(format!(
                "{relative}: eprintln! emits a narration literal; route it through {SINK}"
            ));
        }
    }
    for name in ["write", "writeln"] {
        for args in macro_calls(production, name) {
            if writes_stderr(args) && contains_narration_literal(args) {
                violations.push(format!(
                    "{relative}: {name}!(stderr) emits a narration literal; route it through {SINK}"
                ));
            }
        }
    }
    violations
}

#[test]
fn preformatted_progress_prefix_use_outside_the_sink_is_reported() {
    let snippet = r#"
        let line = format!("{PROGRESS_PREFIX}building_output [repo]");
        eprintln!("{line}");
    "#;
    let hits = production_violations("cli/commands/summary.rs", snippet);
    assert!(
        hits.iter().any(|row| row.contains("PROGRESS_PREFIX")),
        "preformatted PROGRESS_PREFIX must fail the scan: {hits:?}"
    );
    let formatted = r#"
        let line = format!("ripr progress: building_output [repo]");
        eprintln!("{line}");
    "#;
    let formatted_hits = production_violations("cli/commands/summary.rs", formatted);
    assert!(
        formatted_hits.iter().any(|row| row.contains("format!")),
        "preformatted narration literal must fail the scan: {formatted_hits:?}"
    );
    let clean = r#"
        eprintln!("ripr: attempt next command: ripr check");
    "#;
    assert!(
        production_violations("cli/commands/summary.rs", clean).is_empty(),
        "non-narration eprintln must not fail the scan"
    );
}

#[test]
fn narration_eprintln_sites_outside_the_progress_sink_are_explicit() -> Result<(), String> {
    let mut files = Vec::new();
    walk_rust_files(&crate_src(), &mut files)?;
    files.sort();
    let mut violations = Vec::new();
    for path in &files {
        let relative = relative_src(path)?;
        if skip_file(&relative) {
            continue;
        }
        let source = std::fs::read_to_string(path)
            .map_err(|error| format!("read {}: {error}", path.display()))?;
        let production = strip_cfg_test_items(&source);
        violations.extend(production_violations(&relative, &production));
    }
    if !violations.is_empty() {
        return Err(format!(
            "new stderr narration bypassed {SINK}:\n{}",
            violations.join("\n")
        ));
    }
    for row in EXEMPTIONS {
        let path = crate_src().join(row.relative);
        if !path.is_file() {
            return Err(format!(
                "exemption {} is missing ({})",
                row.relative, row.reason
            ));
        }
        if row.reason.trim().is_empty() {
            return Err(format!("exemption {} has no reason", row.relative));
        }
    }
    Ok(())
}
