//! CLI exit-code contract guard (#12505): keep the ripr process exit
//! vocabulary closed and greppable from the source side.
//!
//! `docs/EXIT_CODES.md` documents exactly {0 completed, 2 could not
//! complete, 3 declined/refused, 130 interrupted}; the typed side is
//! pinned by `CommandError::exit_code` and the kani proofs in
//! `crates/ripr/src/cli/exit_kani.rs`, and the panic boundary maps to 2.
//! This guard pins the literal side: every `process::exit` call site in
//! `crates/ripr/src` must pass either a decimal integer literal inside
//! the closed set, or a non-literal argument covered by an allowlist row
//! (today only the startup boundary in `main.rs` and the test-only child
//! in `atomic_file.rs`). `exit(0)` test children in `git.rs` pass because
//! 0 is in the set. Occurrences inside string or char literals — analyzed
//! source fixtures, error prose — are not call sites and are ignored by
//! the Rust literal scanner below, so fixture text never needs allowlist
//! churn.
//!
//! Check name: `cargo xtask check-cli-exit-contract`. Joins
//! `check-policy-all` so a future site cannot reintroduce an undocumented
//! code unnoticed.

use std::collections::BTreeMap;
use std::path::Path;

use crate::{FixKind, PolicyReportSpec, collect_files, finish_policy_report, read_text_lossy};

/// The documented ripr exit codes (`docs/EXIT_CODES.md`). Ordered; a
/// literal outside this set is an undocumented exit.
const CLOSED_EXIT_CODES: [i64; 4] = [0, 2, 3, 130];

/// Pattern spelling for a non-literal exit argument in the allowlist.
const NON_LITERAL_PATTERN: &str = "process::exit(<non-literal>)";

/// The only production tree whose `process::exit` sites terminate the ripr
/// CLI process. `crates/ripr/tests` and fixture crates are analyzed-source
/// surfaces, not the CLI boundary.
fn cli_exit_source_roots() -> [&'static Path; 1] {
    [Path::new("crates/ripr/src")]
}

/// Allowlist: `path|pattern|max_count|owner|reason`, shared format with the
/// other count policies, including per-PR fragment files in the `.d/`
/// directory.
const ALLOWLIST_PATH: &str = "policy/cli-exit-contract-allowlist.txt";

pub(crate) fn check_cli_exit_contract() -> Result<(), String> {
    check_cli_exit_contract_impl()
}

/// One `process::exit` call site found in real (non-string-literal) code.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CliExitFinding {
    pub(crate) line: usize,
    /// The decimal literal the site passes, when it passes one.
    pub(crate) literal: Option<i64>,
}

pub(crate) fn check_cli_exit_contract_impl() -> Result<(), String> {
    let allowlist = crate::read_count_policy_allowlist(ALLOWLIST_PATH)?;
    let mut counts = BTreeMap::<(String, String), usize>::new();
    let mut first_line = BTreeMap::<(String, String), usize>::new();

    for root in cli_exit_source_roots() {
        if !root.exists() {
            continue;
        }
        for path in collect_files(root)? {
            let normalized = crate::normalize_path(&path);
            if !normalized.ends_with(".rs") {
                continue;
            }
            let text = read_text_lossy(&path)?;
            for finding in cli_exit_findings(&text) {
                let documented = finding
                    .literal
                    .is_some_and(|code| CLOSED_EXIT_CODES.contains(&code));
                if documented {
                    continue;
                }
                let pattern = match finding.literal {
                    Some(code) => format!("process::exit({code})"),
                    None => NON_LITERAL_PATTERN.to_string(),
                };
                let key = (normalized.clone(), pattern);
                counts.entry(key.clone()).or_insert(0);
                first_line.entry(key.clone()).or_insert(finding.line);
                if let Some(count) = counts.get_mut(&key) {
                    *count += 1;
                }
            }
        }
    }

    let mut violations = Vec::new();
    for (key, count) in &counts {
        let (path, pattern) = key;
        let allowed = allowlist.get(key).copied().unwrap_or(0);
        let at = first_line
            .get(key)
            .map(|line| format!(" (first at {path}:{line})"))
            .unwrap_or_default();
        if *count > allowed {
            violations.push(format!(
                "{path} contains `{pattern}` {count} time(s), allowed {allowed}{at}\n  the \
                 documented ripr exit set is {CLOSED_EXIT_CODES:?} (docs/EXIT_CODES.md); map \
                 production control flow through `CommandError::exit_code`\n  to allowlist, add \
                 a file under policy/cli-exit-contract-allowlist.d/ (named for this PR) \
                 holding:\n  {path}|{pattern}|{count}|owner|reason"
            ));
        } else if *count < allowed {
            // Reverse-direction check (#2413 pattern): a stale bound lets new
            // undocumented sites slip through without an allowlist update.
            violations.push(format!(
                "{path} allowlist entry for `{pattern}` is stale: max_count={allowed} but \
                 actual={count}; tighten to {count}"
            ));
        }
    }
    for (key, allowed) in &allowlist {
        if !counts.contains_key(key) && *allowed > 0 {
            violations.push(format!(
                "{} allowlist entry for `{}` is orphaned: max_count={allowed} but the site is \
                 gone; remove the entry",
                key.0, key.1
            ));
        }
    }

    finish_policy_report(
        PolicyReportSpec {
            report_file: "cli-exit-contract.md",
            check: "check-cli-exit-contract",
            why_it_matters: "The ripr process exit vocabulary is a documented closed set; an undocumented code makes every harness retry/repair decision about a failed run unreliable.",
            fix_kind: FixKind::AuthorDecisionRequired,
            recommended_fixes: &[
                "Map production exit decisions through `CommandError::exit_code` (2/3/130) or the startup panic boundary (2).",
                "Use a documented code from the closed set {0, 2, 3, 130} at the new site.",
                "Allowlist only a test-only child that must signal a fixture-specific code to its own test, with owner and reason.",
            ],
            rerun_command: "cargo xtask check-cli-exit-contract",
            exception_template: None,
        },
        &violations,
    )
}

/// Find every `process::exit(` call site in `text` that is real code —
/// string/char literal contents and comments are blanked first, so
/// analyzed-source fixture strings never produce findings. Only decimal
/// integer literals are classified as literals; hex, suffixed-with-suffix
/// arithmetic, or identifier arguments all report as non-literal so the
/// pinned surface cannot smuggle an undocumented code through.
pub(crate) fn cli_exit_findings(text: &str) -> Vec<CliExitFinding> {
    let stripped = blank_string_and_char_literals(text);
    let mut findings = Vec::new();
    let mut search_from = 0usize;
    while let Some(offset) = stripped[search_from..].find("process::exit(") {
        let call_start = search_from + offset;
        let arg_start = call_start + "process::exit(".len();
        let line = stripped[..call_start].matches('\n').count() + 1;
        // Balanced-paren scan for the call's closing paren. The stripped
        // text has no live string literals, so a paren inside one cannot
        // unbalance the count.
        let mut depth = 1usize;
        let mut arg_end = None;
        for (index, character) in stripped[arg_start..].char_indices() {
            match character {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        arg_end = Some(arg_start + index);
                        break;
                    }
                }
                _ => {}
            }
        }
        if let Some(arg_end) = arg_end {
            let argument = stripped[arg_start..arg_end].trim();
            let literal = parse_decimal_literal(argument);
            findings.push(CliExitFinding { line, literal });
        }
        search_from = arg_start;
    }
    findings
}

/// A decimal integer literal with optional `_` separators and an optional
/// primitive integer suffix. Everything else (identifiers, calls, hex,
/// negative expressions) returns `None` so it needs an allowlist row.
fn parse_decimal_literal(argument: &str) -> Option<i64> {
    let body = argument
        .strip_suffix("i32")
        .or_else(|| argument.strip_suffix("u32"))
        .or_else(|| argument.strip_suffix("i64"))
        .or_else(|| argument.strip_suffix("u64"))
        .or_else(|| argument.strip_suffix("usize"))
        .or_else(|| argument.strip_suffix("isize"))
        .unwrap_or(argument);
    if body.is_empty() || !body.chars().all(|c| c.is_ascii_digit() || c == '_') {
        return None;
    }
    body.replace('_', "").parse::<i64>().ok()
}

/// Blank the contents of string literals, raw string literals, char
/// literals, and comments while preserving byte offsets and newlines, so
/// a scanner over the result sees only real code tokens. Char-literal
/// detection uses a bounded lookahead so a lifetime apostrophe (`&'a`)
/// never starts a runaway blanking span.
pub(crate) fn blank_string_and_char_literals(text: &str) -> String {
    let characters: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let mut index = 0usize;
    while index < characters.len() {
        let rest = &characters[index..];
        match rest[0] {
            '/' if rest.len() > 1 && rest[1] == '/' => {
                // Line comment: blank to end of line (keep the newline).
                while index < characters.len() && characters[index] != '\n' {
                    out.push(' ');
                    index += 1;
                }
            }
            '/' if rest.len() > 1 && rest[1] == '*' => {
                // Block comment, nesting per the Rust reference.
                let mut depth = 1usize;
                out.push_str("  ");
                index += 2;
                while index < characters.len() && depth > 0 {
                    if characters[index] == '/'
                        && index + 1 < characters.len()
                        && characters[index + 1] == '*'
                    {
                        depth += 1;
                        out.push_str("  ");
                        index += 2;
                    } else if characters[index] == '*'
                        && index + 1 < characters.len()
                        && characters[index + 1] == '/'
                    {
                        depth -= 1;
                        out.push_str("  ");
                        index += 2;
                    } else {
                        out.push(if characters[index] == '\n' { '\n' } else { ' ' });
                        index += 1;
                    }
                }
            }
            'r' | 'b' if starts_raw_string(rest) => {
                // Raw (byte) string: r"..." r#"..."# br#"..."# etc.
                let mut cursor = index;
                while characters[cursor] == 'b' || characters[cursor] == 'r' {
                    cursor += 1;
                }
                let mut hashes = 0usize;
                while characters[cursor] == '#' {
                    hashes += 1;
                    cursor += 1;
                }
                // cursor now at the opening quote.
                out.push_str(&" ".repeat(cursor - index));
                out.push('"');
                cursor += 1;
                loop {
                    if characters[cursor] == '"' {
                        let mut closing = 0usize;
                        while closing < hashes
                            && cursor + 1 + closing < characters.len()
                            && characters[cursor + 1 + closing] == '#'
                        {
                            closing += 1;
                        }
                        if closing == hashes {
                            out.push('"');
                            out.push_str(&"#".repeat(hashes));
                            cursor += 1 + hashes;
                            break;
                        }
                    }
                    out.push(if characters[cursor] == '\n' {
                        '\n'
                    } else {
                        ' '
                    });
                    cursor += 1;
                }
                index = cursor;
            }
            '"' => {
                // Ordinary string literal: keep the quotes, blank contents.
                out.push('"');
                index += 1;
                while index < characters.len() {
                    if characters[index] == '\\' && index + 1 < characters.len() {
                        out.push_str("  ");
                        index += 2;
                    } else if characters[index] == '"' {
                        out.push('"');
                        index += 1;
                        break;
                    } else {
                        out.push(if characters[index] == '\n' { '\n' } else { ' ' });
                        index += 1;
                    }
                }
            }
            '\'' if is_char_literal(rest) => {
                // Char literal with a closing quote within a short span.
                out.push('\'');
                index += 1;
                let mut span = 0usize;
                while index < characters.len() && span < 16 {
                    if characters[index] == '\\' && index + 1 < characters.len() {
                        out.push_str("  ");
                        index += 2;
                        span += 2;
                    } else if characters[index] == '\'' {
                        out.push('\'');
                        index += 1;
                        break;
                    } else {
                        out.push(if characters[index] == '\n' { '\n' } else { ' ' });
                        index += 1;
                        span += 1;
                    }
                }
            }
            other => {
                out.push(other);
                index += 1;
            }
        }
    }
    out
}

/// Whether `rest` starts an `r`/`br` raw string literal: an optional `b`,
/// an `r`, one or more `#`, then a quote. A bare identifier like `run` or
/// `return` must not match.
fn starts_raw_string(rest: &[char]) -> bool {
    let mut cursor = 0usize;
    if rest[cursor] == 'b' {
        cursor += 1;
    }
    if rest.get(cursor) != Some(&'r') {
        return false;
    }
    cursor += 1;
    let mut hashes = 0usize;
    while rest.get(cursor) == Some(&'#') {
        hashes += 1;
        cursor += 1;
    }
    hashes > 0 && rest.get(cursor) == Some(&'"')
}

/// Distinguish a char literal (`'x'`, `'\n'`, `'\u{1F}'`) from a lifetime
/// (`'a`, `&'static`). A char literal has its closing quote within a few
/// characters; a lifetime is followed by an identifier and no immediate
/// quote.
fn is_char_literal(rest: &[char]) -> bool {
    if rest.len() < 3 {
        return false;
    }
    if rest[1] == '\\' {
        // Escape open: find the closing quote within the bounded span.
        return rest[2..].iter().take(8).any(|&c| c == '\'');
    }
    rest[2] == '\''
}

#[cfg(test)]
mod tests {
    use super::{
        ALLOWLIST_PATH, NON_LITERAL_PATTERN, blank_string_and_char_literals,
        check_cli_exit_contract_impl, cli_exit_findings,
    };
    use crate::tests::{with_temp_cwd, write};

    #[test]
    fn literal_scanner_blanks_strings_chars_and_comments_but_keeps_code() {
        let stripped = blank_string_and_char_literals(concat!(
            "let prose = \"std::process::exit(1);\"; // process::exit(7) remark\n",
            "/* process::exit(9) */ let raw = r#\"process::exit(4)\"#;\n",
            "let owner: &'static str = \"x\";\n",
            "std::process::exit(3);\n",
            "let lifetime_free = 'x';\n",
        ));
        assert!(
            !stripped.contains("process::exit(1)"),
            "string-literal fixture must be blanked: {stripped}"
        );
        assert!(
            !stripped.contains("process::exit(7)") && !stripped.contains("process::exit(9)"),
            "comment text must be blanked: {stripped}"
        );
        assert!(
            !stripped.contains("process::exit(4)"),
            "raw-string fixture must be blanked: {stripped}"
        );
        assert!(
            stripped.contains("process::exit(3);"),
            "a real call site must survive stripping: {stripped}"
        );
        assert!(
            stripped.contains("&'static str"),
            "a lifetime must not be blanked as a char literal: {stripped}"
        );
        assert_eq!(stripped.matches('\n').count(), 5, "newlines preserved");
    }

    #[test]
    fn findings_classify_documented_and_undocumented_literals() {
        let findings = cli_exit_findings(concat!(
            "fn a() { std::process::exit(2); }\n",
            "fn b() { std::process::exit(130); }\n",
            "fn c() { process::exit(1); }\n",
            "fn d() { process::exit(86); }\n",
            "fn e() { process::exit(code); }\n",
            "fn f() { process::exit(err.exit_code()); }\n",
            "fn g() { let s = \"process::exit(1)\"; }\n",
        ));
        let literals: Vec<Option<i64>> = findings.iter().map(|f| f.literal).collect();
        assert_eq!(
            literals,
            vec![Some(2), Some(130), Some(1), Some(86), None, None],
            "string-literal line must not produce a finding; identifier and call \
             arguments report non-literal"
        );
        assert_eq!(findings[2].line, 3, "line numbers follow the source");
    }

    #[test]
    fn check_fails_on_undocumented_exit_literal_and_passes_the_closed_set() -> Result<(), String> {
        with_temp_cwd("cli-exit-contract-literal", |root| -> Result<(), String> {
            std::fs::create_dir_all(root.join("crates/ripr/src/cli"))
                .map_err(|err| err.to_string())?;
            std::fs::create_dir_all(root.join("policy")).map_err(|err| err.to_string())?;
            std::fs::write(root.join(ALLOWLIST_PATH), "# empty allowlist\n")
                .map_err(|err| err.to_string())?;
            write(
                &root.join("crates/ripr/src/cli/startup_boundary.rs"),
                concat!(
                    "fn boundary(code: i32) {\n",
                    "    std::process::exit(2);\n",
                    "    std::process::exit(130);\n",
                    "    let fixture = \"process::exit(1)\";\n",
                    "}\n",
                ),
            );
            check_cli_exit_contract_impl().map_err(|error| {
                format!("closed-set and string-literal sites must pass: {error}")
            })?;

            write(
                &root.join("crates/ripr/src/cli/startup_boundary.rs"),
                concat!(
                    "fn boundary(code: i32) {\n",
                    "    std::process::exit(2);\n",
                    "    std::process::exit(1);\n",
                    "}\n",
                ),
            );
            let error = check_cli_exit_contract_impl()
                .err()
                .ok_or("an undocumented exit literal must fail the guard")?;
            assert!(
                error.contains("process::exit(1)"),
                "the violation must name the undocumented code: {error}"
            );
            assert!(
                error.contains("docs/EXIT_CODES.md"),
                "the violation must point at the documented contract: {error}"
            );
            Ok(())
        })
    }

    #[test]
    fn check_requires_allowlist_for_non_literal_exits_and_rejects_stale_bounds()
    -> Result<(), String> {
        with_temp_cwd(
            "cli-exit-contract-non-literal",
            |root| -> Result<(), String> {
                std::fs::create_dir_all(root.join("crates/ripr/src"))
                    .map_err(|err| err.to_string())?;
                std::fs::create_dir_all(root.join("policy")).map_err(|err| err.to_string())?;
                std::fs::write(root.join(ALLOWLIST_PATH), "# empty allowlist\n")
                    .map_err(|err| err.to_string())?;
                write(
                    &root.join("crates/ripr/src/boundary.rs"),
                    "fn boundary(err: E) { process::exit(err.exit_code()); }\n",
                );
                let error = check_cli_exit_contract_impl()
                    .err()
                    .ok_or("a non-literal exit must fail without an allowlist row")?;
                assert!(
                    error.contains(NON_LITERAL_PATTERN),
                    "the violation must name the non-literal pattern: {error}"
                );

                // The pinned boundary row admits it.
                let allowlist = format!(
                    "# pinned startup boundary\n{}\n",
                    format_args!(
                        "crates/ripr/src/boundary.rs|{NON_LITERAL_PATTERN}|1|cli-exit-boundary|\
                     pinned typed mapping"
                    )
                );
                std::fs::write(root.join(ALLOWLIST_PATH), allowlist)
                    .map_err(|err| err.to_string())?;
                check_cli_exit_contract_impl().map_err(|error| {
                    format!("allowlisted non-literal boundary must pass: {error}")
                })?;

                // A stale bound (allowed 2, actual 1) must fail the reverse
                // direction so invisible slack cannot accumulate (#2413).
                let stale = format!(
                    "# pinned startup boundary\n{}\n",
                    format_args!(
                        "crates/ripr/src/boundary.rs|{NON_LITERAL_PATTERN}|2|cli-exit-boundary|\
                     pinned typed mapping"
                    )
                );
                std::fs::write(root.join(ALLOWLIST_PATH), stale).map_err(|err| err.to_string())?;
                let error = check_cli_exit_contract_impl()
                    .err()
                    .ok_or("a stale allowlist bound must fail")?;
                assert!(
                    error.contains("stale"),
                    "the violation must name the stale bound: {error}"
                );
                Ok(())
            },
        )
    }
}
