//! Ratchet against new inline test modules in the published crates (#15193).
//!
//! Development tests stay out of the published `ripr` and `ripr-contract`
//! source. Cargo packages whole files, so a `#[cfg(test)] mod tests { .. }`
//! body inside a production file ships even though no ordinary build compiles
//! it. The layout that keeps tests out is an owner-local out-of-line module:
//! `#[cfg(test)] mod tests;` resolving to `<stem>/tests.rs` (or
//! `#[path = "<name>_tests.rs"]`), which both manifests exclude and which keeps
//! the module path and private access unchanged.
//!
//! Existing inline modules are counted per file in
//! `policy/inline-test-modules.txt`. A new inline module, or one more in a
//! listed file, fails; a baseline above the real count fails too, so the
//! ceiling only goes down as the migration moves modules out of line.
//!
//! A module counts when it has a body and an outer `cfg` attribute whose
//! predicate names `test` (`test`, `all(test, ..)`, `any(test, ..)`), and no
//! enclosing module already counted. Files that do not parse are refused, not
//! skipped: an unparsed file is not evidence that it has no inline tests.

use std::collections::BTreeMap;

use ra_ap_syntax::ast::{self, HasAttrs};
use ra_ap_syntax::{AstNode, Edition, SourceFile, SyntaxKind};

const BASELINE_PATH: &str = "policy/inline-test-modules.txt";
const SOURCE_ROOTS: [&str; 2] = ["crates/ripr/src/", "crates/ripr-contract/src/"];

fn is_scanned_path(path: &str) -> bool {
    SOURCE_ROOTS.iter().any(|root| path.starts_with(root)) && path.ends_with(".rs")
}

/// True when the attribute is `cfg(..)` and its predicate names `test` as an
/// identifier outside any `not(..)` (`test`, `all(test, ..)`, `any(test, ..)`).
/// `feature = "test"` is a string, and `not(test)` gates production code, so
/// neither counts.
fn is_test_cfg(attr: &ast::Attr) -> bool {
    let Some(ast::Meta::CfgMeta(cfg)) = attr.meta() else {
        return false;
    };
    let tokens: Vec<_> = cfg
        .syntax()
        .descendants_with_tokens()
        .filter_map(|element| element.into_token())
        .filter(|token| !token.kind().is_trivia())
        .collect();
    // Names of the open predicate calls (`all`, `any`, `not`, and the outer
    // `cfg`) enclosing the current token.
    let mut open: Vec<String> = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        match token.kind() {
            SyntaxKind::L_PAREN => {
                let name = index
                    .checked_sub(1)
                    .and_then(|before| tokens.get(before))
                    .filter(|before| before.kind() == SyntaxKind::IDENT)
                    .map_or_else(String::new, |before| before.text().to_string());
                open.push(name);
            }
            SyntaxKind::R_PAREN => {
                open.pop();
            }
            SyntaxKind::IDENT if token.text() == "test" => {
                let followed_by_call = tokens
                    .get(index + 1)
                    .is_some_and(|next| next.kind() == SyntaxKind::L_PAREN);
                if !followed_by_call && !open.iter().any(|name| name == "not") {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

/// Number of outermost inline test modules in `source`, or an error when the
/// file does not parse cleanly.
pub(crate) fn inline_test_module_count(source: &str) -> Result<usize, String> {
    let parse = SourceFile::parse(source, Edition::CURRENT);
    if !parse.errors().is_empty() {
        return Err(format!(
            "does not parse ({} syntax error(s)); fix the file before the inline-test ratchet can count it",
            parse.errors().len()
        ));
    }
    let tree = parse.tree();
    let counted = tree
        .syntax()
        .descendants()
        .filter_map(ast::Module::cast)
        .filter(|module| module.item_list().is_some() && module.attrs().any(|a| is_test_cfg(&a)))
        .filter(|module| {
            !module
                .syntax()
                .ancestors()
                .skip(1)
                .filter_map(ast::Module::cast)
                .any(|outer| outer.attrs().any(|a| is_test_cfg(&a)))
        })
        .count();
    Ok(counted)
}

/// Compares measured per-file counts with the baseline ceilings.
pub(crate) fn ratchet_violations(
    counts: &BTreeMap<String, usize>,
    baseline: &BTreeMap<String, usize>,
) -> Vec<String> {
    let mut violations = Vec::new();
    for (path, count) in counts {
        let allowed = baseline.get(path).copied().unwrap_or(0);
        if *count > allowed {
            violations.push(format!(
                "{path} has {count} inline test module(s), baseline allows {allowed}\n  reason: #15193: declare `#[cfg(test)] mod tests;` and put the body in the file Rust resolves for it (`<stem>/tests.rs`, or `#[path = \"<name>_tests.rs\"]`), so the tests stay out of the published crate; do not raise the baseline"
            ));
        }
    }
    for (path, allowed) in baseline {
        let count = counts.get(path).copied().unwrap_or(0);
        if count < *allowed {
            violations.push(format!(
                "{BASELINE_PATH} lists {allowed} for {path} but only {count} remain\n  reason: lower or remove the entry so the ratchet only tightens"
            ));
        }
    }
    violations
}

fn read_baseline() -> Result<BTreeMap<String, usize>, String> {
    let mut baseline = BTreeMap::new();
    for record in crate::read_pipe_records(BASELINE_PATH, 2)? {
        let count = record[1]
            .parse::<usize>()
            .map_err(|err| format!("{BASELINE_PATH}: bad count `{}`: {err}", record[1]))?;
        if count == 0 {
            return Err(format!(
                "{BASELINE_PATH}: `{}` lists 0; remove the entry instead",
                record[0]
            ));
        }
        if baseline.insert(record[0].clone(), count).is_some() {
            return Err(format!("{BASELINE_PATH}: duplicate entry `{}`", record[0]));
        }
    }
    Ok(baseline)
}

/// Violations for the tracked file set, for `check-architecture`.
pub(crate) fn violations(files: &[String]) -> Result<Vec<String>, String> {
    let baseline = read_baseline()?;
    let mut counts = BTreeMap::new();
    let mut violations = Vec::new();
    let mut scanned = 0usize;
    for file in files.iter().filter(|file| is_scanned_path(file)) {
        scanned += 1;
        let text = crate::read_text_lossy(std::path::Path::new(file))?;
        match inline_test_module_count(&text) {
            Ok(0) => {}
            Ok(count) => {
                counts.insert(file.clone(), count);
            }
            Err(err) => violations.push(format!("{file} {err}")),
        }
    }
    if scanned == 0 {
        return Err(
            "inline-test-module ratchet scanned zero files; zero subjects is not a pass"
                .to_string(),
        );
    }
    violations.extend(ratchet_violations(&counts, &baseline));
    Ok(violations)
}

#[cfg(test)]
#[path = "inline_test_modules_tests.rs"]
mod tests;
