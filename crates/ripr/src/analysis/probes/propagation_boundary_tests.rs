//! #1715: a changed closing delimiter belongs to a complete propagation expression.
use super::diff::probes_for_file;
use crate::analysis::diff::{ChangedFile, ChangedLine};
use crate::analysis::rust_index::RustIndex;
use crate::analysis::syntax::{LexicalRustSyntaxAdapter, RaRustSyntaxAdapter, RustSyntaxAdapter};
use crate::domain::{Probe, ProbeFamily};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const SOURCE: &str = "pub fn finish(value: Option<()>) -> Result<(), &'static str> {\n    let _ = value.ok_or_else(|| {\n        \"missing\"\n    })?;\n    Ok(())\n}\n";
const SUBJECT: &str = "value.ok_or_else(|| {\n        \"missing\"\n    })?";

fn probes(
    source: &str,
    line: usize,
    claimed: Option<&str>,
    fallback: bool,
) -> Result<Vec<Probe>, String> {
    let path = PathBuf::from("src/lib.rs");
    let mut facts = if fallback {
        LexicalRustSyntaxAdapter.summarize_file(&path, source)?
    } else {
        RaRustSyntaxAdapter.summarize_file(&path, source)?
    };
    facts.used_lexical_fallback |= fallback;
    let text = claimed
        .map(str::to_string)
        .or_else(|| source.lines().nth(line.checked_sub(1)?).map(str::to_string))
        .ok_or("fixture line missing")?;
    let index = RustIndex::from_owned(crate::analysis::facts::OwnedRustIndex {
        files: BTreeMap::from([(path.clone(), facts)]),
        ..Default::default()
    });
    let changed = ChangedFile {
        path,
        added_lines: vec![ChangedLine {
            line,
            new_side_line: line,
            text,
        }],
        removed_lines: Vec::new(),
    };
    Ok(probes_for_file(Path::new("."), &changed, &index))
}

#[test]
fn closing_question_mark_names_the_complete_current_expression() -> Result<(), String> {
    let found = probes(SOURCE, 4, None, false)?;
    let errors: Vec<_> = found
        .iter()
        .filter(|p| p.family == ProbeFamily::ErrorPath)
        .collect();
    assert_eq!(errors.len(), 1, "{found:?}");
    assert_eq!(errors[0].expression, SUBJECT);
    assert_eq!(errors[0].after.as_deref(), Some(SUBJECT));
    assert_eq!(errors[0].location.line, 2);
    assert_eq!(errors[0].location.column, 13);
    assert!(errors[0].owner.is_some());
    Ok(())
}

#[test]
fn closing_question_mark_identity_survives_crlf() -> Result<(), String> {
    let lf = probes(SOURCE, 4, None, false)?;
    let crlf = probes(&SOURCE.replace('\n', "\r\n"), 4, None, false)?;
    let error = |rows: Vec<Probe>| {
        rows.into_iter()
            .find(|p| p.family == ProbeFamily::ErrorPath)
            .ok_or("missing error")
    };
    let first = error(lf)?;
    let second = error(crlf)?;
    assert_eq!(first.id, second.id);
    assert_eq!(first.location, second.location);
    assert_eq!(first.expression, second.expression.replace("\r\n", "\n"));
    Ok(())
}

#[test]
fn stale_fallback_ignored_and_sibling_lines_borrow_no_complete_subject() -> Result<(), String> {
    for (source, line, claimed, fallback) in [
        (SOURCE.to_string(), 4, Some(")?;"), false),
        (SOURCE.to_string(), 4, None, true),
        (SOURCE.replace("})?;", "});"), 4, None, false),
        (SOURCE.replace("})?;", "})?; other()?;"), 4, None, false),
        (SOURCE.replace("})?;", "})?; }"), 4, None, true),
    ] {
        let found = probes(&source, line, claimed, fallback)?;
        assert!(
            !found.is_empty(),
            "refusal must preserve changed-source evidence"
        );
        assert!(found.iter().all(|p| p.expression != SUBJECT), "{found:?}");
    }
    Ok(())
}

#[test]
fn nested_return_targets_borrow_no_enclosing_function_subject() -> Result<(), String> {
    for wrapper in ["||", "async", "try", "const"] {
        let source = format!(
            "fn finish(value: Option<()>) {{\n    let _ = {wrapper} {{\n        let _ = value.ok_or_else(|| {{\n            \"missing\"\n        }})?;\n        Ok(())\n    }};\n}}\n"
        );
        let found = probes(&source, 5, None, false)?;
        assert!(!found.is_empty(), "{wrapper}: evidence disappeared");
        assert!(
            found
                .iter()
                .all(|p| !p.expression.starts_with("value.ok_or_else")),
            "{wrapper}: {found:?}"
        );
    }
    Ok(())
}

#[test]
fn an_exact_sibling_error_does_not_confirm_the_changed_question_mark() -> Result<(), String> {
    let path = PathBuf::from("src/lib.rs");
    let source = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../fixtures/propagated_error_closing_early_path/input/src/lib.rs"),
    )
    .map_err(|error| error.to_string())?;
    let facts = RaRustSyntaxAdapter.summarize_file(&path, &source)?;
    let mut index = RustIndex::default();
    index.insert_file(path.clone(), facts, true);
    assert_eq!(index.tests().len(), 1, "the actual oracle must be indexed");
    let changed = ChangedFile {
        path,
        added_lines: vec![ChangedLine {
            line: 8,
            new_side_line: 8,
            text: "    )?;".to_string(),
        }],
        removed_lines: Vec::new(),
    };
    let seeded = super::diff::probes_for_file_with_relations(Path::new("."), &changed, &index, &[]);
    let error = seeded
        .iter()
        .find(|s| s.probe.family == ProbeFamily::ErrorPath)
        .ok_or("no error subject")?;
    assert_eq!(
        error.probe.expression,
        "value.ok_or(\n        Failure::Missing\n    )?"
    );
    let origin = error
        .propagation_origin
        .as_ref()
        .ok_or("missing original mutation site")?;
    assert_eq!(origin.new_side_line, 8);
    assert_eq!(origin.text.trim(), ")?;");
    let candidates = crate::analysis::classify::RelatedTestCandidateIndex::new(&index);
    let without_origin = crate::analysis::classifier::classify_probe_with_candidate_index(
        &error.probe,
        &index,
        true,
        None,
        &candidates,
        None,
    );
    assert_eq!(
        without_origin.ripr.reveal.discriminate.state,
        crate::domain::StageState::Yes,
        "removing the origin must expose the token-credit trap: {without_origin:?}"
    );
    let finding = crate::analysis::classifier::classify_probe_with_candidate_index(
        &error.probe,
        &index,
        true,
        None,
        &candidates,
        Some(origin),
    );
    assert_ne!(finding.class, crate::domain::ExposureClass::Exposed);
    assert_eq!(
        finding.ripr.reveal.discriminate.state,
        crate::domain::StageState::Weak
    );
    assert!(
        finding
            .ripr
            .reveal
            .discriminate
            .summary
            .contains("propagation_boundary_activation_unestablished")
    );
    assert!(
        finding
            .related_tests
            .iter()
            .any(|row| row.miss == Some(crate::domain::RelatedTestMiss::ObservationUnconfirmed))
    );
    Ok(())
}
