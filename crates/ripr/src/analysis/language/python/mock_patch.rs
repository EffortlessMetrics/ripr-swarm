//! Which patch/mock/monkeypatch sites provably leave the owner untouched.
//!
//! `test_has_mocked_module` is a syntax-only check: any `patch(...)`,
//! `patch.object(...)`, `monkeypatch.setattr(...)` or similar in a related
//! test limits the finding, whatever it patches. This module narrows that
//! limit, fail closed, for one shape only: a string-literal dotted target
//! (`patch("os.getcwd")`, `monkeypatch.setattr("a.b.c", ...)`) none of whose
//! dotted segments names anything in the owner's file or module.
//!
//! The decision is by name and never by runtime resolution:
//!
//! - every mock site in the test must be a plain `patch(`, `mock.patch(`,
//!   `mocker.patch(` or `monkeypatch.setattr(` call whose first argument is a
//!   single-line, non-prefixed, non-concatenated string literal;
//! - `patch.object`, `patch.multiple`, `monkeypatch.setitem`/`delattr`, a
//!   bare `@patch` decorator, an object first argument, an f-string, a
//!   computed target or a decorator whose arguments cannot be read keep the
//!   limit;
//! - a target segment that is any identifier token of the owner's source
//!   file (its body, imports, callees, class path) or a name of the owner's
//!   module (file stem, re-exporting package) keeps the limit;
//! - without the owner's source the limit is kept.
//!
//! Residual, documented rather than hidden: a target the owner reaches only
//! through another file's code (a callee in a different module that itself
//! uses the patched name) is not visible to a name check. The narrowing is a
//! cost-bounded static judgment, not a proof that no substitution reaches
//! the owner.

use super::related_tests::is_python_identifier_char;
use super::{PythonOwner, PythonTest};
use std::collections::BTreeSet;

/// Call spellings that substitute names at runtime. The first group has a
/// readable string target; the second never does and always keeps the limit.
const LITERAL_TARGET_FORMS: [&str; 2] = ["patch(", "monkeypatch.setattr("];
const OPAQUE_FORMS: [&str; 5] = [
    "patch.object(",
    "patch.multiple(",
    "patch.dict(",
    "monkeypatch.setitem(",
    "monkeypatch.delattr(",
];

/// Whether every mock site of `test` provably targets a name the owner does
/// not use. False whenever any site is ambiguous or the owner source is
/// missing.
pub(super) fn mock_sites_are_disjoint_from_owner(
    test: &PythonTest,
    owner: &PythonOwner,
    owner_source: Option<&str>,
) -> bool {
    let Some(owner_source) = owner_source else {
        return false;
    };
    // `body_text` starts at `def`; the decorators are separate source text.
    let combined: String = test
        .decorator_texts
        .iter()
        .map(|decorator| format!("@{decorator}\n"))
        .chain(std::iter::once(test.body_text.clone()))
        .collect();
    let body = combined.as_str();
    if OPAQUE_FORMS
        .iter()
        .any(|form| form_sites(body, form).next().is_some())
    {
        return false;
    }
    let mut targets: Vec<String> = Vec::new();
    let mut decorator_sites = 0_usize;
    for form in LITERAL_TARGET_FORMS {
        for site in form_sites(body, form) {
            let Some(target) = literal_target(&body[site + form.len()..]) else {
                return false;
            };
            if line_prefix(body, site).trim_start().starts_with('@') {
                decorator_sites += 1;
            }
            targets.push(target);
        }
    }
    // A decorator whose arguments are not in the text (a bare `@patch`) has
    // no readable site; it must not be silently dropped.
    let patch_decorators = test
        .decorators
        .iter()
        .filter(|name| is_patch_decorator_name(name))
        .count();
    if patch_decorators > decorator_sites {
        return false;
    }
    if targets.is_empty() {
        return false;
    }
    let used = owner_names(owner, owner_source);
    targets
        .iter()
        .all(|target| target.split('.').all(|segment| !used.contains(segment)))
}

fn is_patch_decorator_name(name: &str) -> bool {
    ["patch", "patch.object", "patch.multiple", "patch.dict"]
        .iter()
        .any(|form| name == *form || name.ends_with(&format!(".{form}")))
}

/// Byte offsets of `form` at an identifier boundary (`mock.patch(` and
/// `patch(` but not `dispatch(`).
fn form_sites<'a>(body: &'a str, form: &'a str) -> impl Iterator<Item = usize> + 'a {
    body.match_indices(form).filter_map(move |(idx, _)| {
        let boundary = !body[..idx]
            .chars()
            .next_back()
            .is_some_and(is_python_identifier_char);
        boundary.then_some(idx)
    })
}

fn line_prefix(body: &str, idx: usize) -> &str {
    let start = body[..idx].rfind('\n').map_or(0, |at| at + 1);
    // Drop a dotted qualifier such as `mock.` so `@mock.patch(` reads as a
    // decorator site.
    let prefix = &body[start..idx];
    prefix.trim_end_matches(|ch: char| is_python_identifier_char(ch) || ch == '.')
}

/// The dotted string target at the start of a call's arguments, or None when
/// the first argument is anything other than one plain `"a.b"` literal
/// followed by `,` or `)`. A leading `target=` keyword is accepted.
fn literal_target(args: &str) -> Option<String> {
    let mut rest = args.trim_start();
    if let Some(after) = rest.strip_prefix("target") {
        rest = after.trim_start().strip_prefix('=')?.trim_start();
    }
    let quote = rest.chars().next().filter(|ch| matches!(ch, '"' | '\''))?;
    let inner = &rest[1..];
    if inner.starts_with(quote) {
        // Empty string or the opening of a triple-quoted literal.
        return None;
    }
    let end = inner.find(quote)?;
    let literal = &inner[..end];
    if literal.contains(['\\', '\n']) {
        return None;
    }
    let after = inner[end + 1..].trim_start();
    if !(after.starts_with(',') || after.starts_with(')')) {
        return None;
    }
    let segments: Vec<&str> = literal.split('.').collect();
    let dotted = segments.len() >= 2
        && segments.iter().all(|segment| {
            segment
                .chars()
                .next()
                .is_some_and(|ch| ch == '_' || ch.is_ascii_alphabetic())
                && segment.chars().all(is_python_identifier_char)
        });
    dotted.then(|| literal.to_string())
}

/// Every name the owner can be reached or influenced through by name: all
/// identifier tokens of its source file plus the owner's own metadata.
fn owner_names(owner: &PythonOwner, owner_source: &str) -> BTreeSet<String> {
    let mut names: BTreeSet<String> = BTreeSet::new();
    let mut token = String::new();
    for ch in owner_source.chars().chain(std::iter::once(' ')) {
        if is_python_identifier_char(ch) {
            token.push(ch);
        } else if !token.is_empty() {
            names.insert(std::mem::take(&mut token));
        }
    }
    let mut add_dotted = |text: &str| {
        for segment in text.split(['.', '/']) {
            if !segment.is_empty() {
                names.insert(segment.to_string());
            }
        }
    };
    add_dotted(&owner.name);
    add_dotted(&owner.qualified_name);
    add_dotted(&owner.class_path);
    for module in &owner.reexport_modules {
        add_dotted(module);
    }
    for callee in owner.module_callees.iter().chain(&owner.same_class_callees) {
        add_dotted(callee);
    }
    for import in &owner.imports {
        add_dotted(&import.imported);
        add_dotted(&import.alias);
        add_dotted(&import.source_module);
    }
    if let Some(stem) = owner.file.file_stem().and_then(|stem| stem.to_str()) {
        if stem == "__init__" {
            if let Some(package) = owner
                .file
                .parent()
                .and_then(|dir| dir.file_name())
                .and_then(|name| name.to_str())
            {
                names.insert(package.to_string());
            }
        } else {
            names.insert(stem.to_string());
        }
    }
    names
}
