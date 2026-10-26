//! Exact semantic guards supplement the runtime-label coverage of specs 0150/0157.
//! The corpus's coarse verdict is deliberately not the limitation/relation oracle.

#![cfg(feature = "lang-rust")]

use ripr::domain::StaticLimitKind;
use ripr::{CheckInput, ExposureClass, Mode, ProbeFamily, check_workspace};
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

struct Scratch(PathBuf);

impl Scratch {
    fn create() -> Result<Self, String> {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ripr-spec-b-bindings-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let scratch = Self(root);
        std::fs::create_dir_all(&scratch.0).map_err(|error| error.to_string())?;
        Ok(scratch)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn read_json(path: &Path) -> Result<Value, String> {
    let text = std::fs::read_to_string(path).map_err(|error| error.to_string())?;
    serde_json::from_str(&text).map_err(|error| error.to_string())
}

fn materialize(corpus: &Path, case: &Value, root: &Path) -> Result<PathBuf, String> {
    let subject_id = case["subject_id"].as_str().ok_or("missing subject id")?;
    let subject = corpus.join("subjects").join(subject_id);
    let metadata = read_json(&subject.with_extension("json"))?;
    for file in metadata["retained_files"]
        .as_array()
        .ok_or("missing retained files")?
    {
        let relative = file["path"].as_str().ok_or("missing retained path")?;
        let source = subject.join(relative);
        let source = if source.is_file() {
            source
        } else {
            subject.join(format!("{relative}.txt"))
        };
        let destination = root.join(relative);
        let parent = destination.parent().ok_or("missing parent")?;
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        std::fs::copy(source, destination).map_err(|error| error.to_string())?;
    }
    let diff = corpus.join(case["diff"].as_str().ok_or("missing diff")?);
    let patch = std::fs::read_to_string(&diff).map_err(|error| error.to_string())?;
    let removed: Vec<_> = patch
        .lines()
        .filter(|line| !line.starts_with("---"))
        .filter_map(|line| line.strip_prefix('-'))
        .collect();
    let added: Vec<_> = patch
        .lines()
        .filter(|line| !line.starts_with("+++"))
        .filter_map(|line| line.strip_prefix('+'))
        .collect();
    assert_eq!(removed.len(), 1, "one changed binding is the stimulus");
    assert_eq!(added.len(), 1, "one changed binding is the stimulus");
    let file = case["anchor"]["file"].as_str().ok_or("missing anchor")?;
    let target = root.join(file);
    let source = std::fs::read_to_string(&target).map_err(|error| error.to_string())?;
    assert_eq!(source.matches(removed[0]).count(), 1);
    std::fs::write(target, source.replacen(removed[0], added[0], 1))
        .map_err(|error| error.to_string())?;
    Ok(diff)
}

#[test]
fn numbered_binding_cases_keep_exact_semantic_contracts() -> Result<(), String> {
    let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/rust-verdict-corpus");
    // Literal use positions follow the authored programs, independently of
    // expected/rows and its verdict/class/retarget projection.
    let cases: [(&str, &[usize], bool); 17] = [
        ("0150-comment-only-family", &[4], false),
        ("0150-destructured-binding", &[4], false),
        ("0150-len-utf8-map-or", &[5], true),
        ("0150-map-or-else", &[5], true),
        ("0150-member-lookalike", &[15], true),
        ("0150-missing-owner", &[7], false),
        ("0150-no-related-test", &[5], true),
        ("0150-rfind-map-or-equality", &[5], true),
        ("0150-shadowed-binding", &[4], false),
        ("0150-string-only-family", &[5], true),
        ("0157-closure-capture", &[4], false),
        ("0157-comment-and-string-mention", &[4], false),
        ("0157-macro-invocation", &[4], false),
        ("0157-reassigned-binding", &[4], false),
        ("0157-shadowed-binding", &[4], false),
        ("0157-sibling-function", &[4], false),
        ("0157-two-direct-uses", &[5, 8], true),
    ];
    for (name, lines, relates) in cases {
        let case = read_json(&corpus.join(format!("cases/spec-b-{name}.json")))?;
        let scratch = Scratch::create()?;
        let diff = materialize(&corpus, &case, &scratch.0)?;
        let result = check_workspace(CheckInput {
            root: scratch.0.clone(),
            diff_file: Some(diff),
            mode: Mode::Draft,
            include_unchanged_tests: true,
            ..CheckInput::default()
        })?;
        assert_eq!(
            result.findings.len(),
            lines.len(),
            "{name}: nonempty probes"
        );
        let mut ids = BTreeSet::new();
        for (finding, line) in result.findings.iter().zip(lines) {
            assert_eq!(finding.probe.location.line, *line, "{name}: use order");
            assert!(
                ids.insert(finding.probe.id.0.clone()),
                "{name}: distinct ids"
            );
            assert_ne!(
                finding.static_limit_kind,
                Some(StaticLimitKind::RustValuePropagationUnresolved),
                "{name}: refuse the specific value-propagation limitation"
            );
            assert_eq!(
                finding.probe.owner.is_none(),
                name == "0150-missing-owner",
                "{name}: exact function ownership"
            );
            let relations: Vec<_> = finding
                .evidence
                .iter()
                .filter(|evidence| evidence.starts_with("binding_predicate_relation:"))
                .collect();
            assert_eq!(relations.len(), usize::from(relates), "{name}: relation");
            if relates {
                assert_eq!(finding.probe.family, ProbeFamily::Predicate, "{name}");
                assert!(
                    finding.static_limit_kind.is_none(),
                    "{name}: disclosure only"
                );
            }
            if name == "0150-missing-owner" {
                assert_eq!(finding.class, ExposureClass::StaticUnknown, "{name}");
                assert!(finding.static_limit_kind.is_none(), "ownerless refusal");
                assert_eq!(finding.related_tests.len(), 1, "retained same-file test");
            }
            let causal = match name {
                "0150-rfind-map-or-equality" => Some((
                    "input.rfind(delim).map_or(0, |idx| idx)",
                    "input.rfind(delim).map_or(0, |at| at)",
                    ".rfind(",
                )),
                "0150-len-utf8-map-or" => Some((
                    "input.chars().next().map_or(0, |c| c.len_utf8())",
                    "input.chars().next().map_or(0, |ch| ch.len_utf8())",
                    ".chars(",
                )),
                "0157-two-direct-uses" => {
                    Some(("text.len()", "text.as_bytes().len()", "text.as_bytes("))
                }
                _ => None,
            };
            if let Some((before, after, operation)) = causal {
                let binding = if name == "0150-len-utf8-map-or" {
                    "width"
                } else {
                    "end"
                };
                assert_eq!(
                    relations[0].as_str(),
                    format!(
                        "binding_predicate_relation: changed binding `{binding}` initializer `{before}` -> `{after}` flows into predicate operand at line {line}"
                    ),
                    "{name}: exact causal relation"
                );
                assert_eq!(finding.probe.before.as_deref(), Some(before), "{name}");
                assert_eq!(finding.probe.after.as_deref(), Some(after), "{name}");
                assert!(
                    finding.evidence.iter().any(|evidence| {
                        evidence.starts_with("binding_predicate_value_unresolved:")
                            && evidence.ends_with(&format!("`{operation}`"))
                    }),
                    "{name}: earliest operation"
                );
                assert!(
                    !finding.evidence.iter().any(|evidence| {
                        evidence.contains("not mapped to a high-confidence probe family")
                    }),
                    "{name}: no generic syntax limitation after retarget"
                );
            }
        }
    }
    Ok(())
}
