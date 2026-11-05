//! Comparison against the accepted base for honesty-capture digest requirements.
//! This guards a single change, not authenticity or coordinated multi-change edits.
use super::EVIDENCE_PROMOTION_HONESTY_CORPUS;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

const REPORT_PREFIX: &str = "fixtures/evidence-promotion-honesty-corpus/reports/";

fn sha(text: &str) -> bool {
    matches!(text.len(), 40 | 64) && text.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Qualification's base comes from the already verified admission output, and
/// must be the tested integration commit's first parent. Legacy PR/queue
/// runs instead require the event base to equal that first parent; this does
/// not claim an admission receipt.
fn choose_base(
    profile: &str,
    event: &str,
    admitted: &str,
    event_base: &str,
    first_parent: &str,
    local: &str,
) -> Result<String, String> {
    match profile {
        "qualification" if sha(admitted) && admitted == first_parent => Ok(admitted.into()),
        "qualification" => Err(
            "honesty digest: admitted base missing, malformed, or not HEAD's first parent".into(),
        ),
        "legacy"
            if matches!(event, "pull_request" | "merge_group")
                && sha(event_base)
                && event_base == first_parent =>
        {
            Ok(event_base.into())
        }
        "legacy" if matches!(event, "pull_request" | "merge_group") => Err(
            "honesty digest: legacy event base missing, malformed, or not HEAD's first parent"
                .into(),
        ),
        "" | "legacy" if sha(local) => Ok(local.into()),
        "" | "legacy" => Err("honesty digest: no valid local merge base".into()),
        _ => Err("honesty digest: unknown qualification profile".into()),
    }
}

fn entries(corpus: &Value) -> Result<BTreeSet<String>, String> {
    let rows = corpus
        .get("report_digest_required")
        .and_then(Value::as_array)
        .ok_or("honesty digest: base required list is missing or malformed")?;
    let mut paths = BTreeSet::new();
    for row in rows {
        let path = row
            .as_str()
            .filter(|s| {
                s.starts_with(REPORT_PREFIX)
                    && !s[REPORT_PREFIX.len()..].contains('/')
                    && s.ends_with(".json")
            })
            .ok_or("honesty digest: malformed required path")?;
        if !paths.insert(path.to_string()) {
            return Err("honesty digest: duplicate required path".into());
        }
    }
    Ok(paths)
}

fn cases(corpus: &Value) -> Result<BTreeMap<String, String>, String> {
    let rows = corpus
        .get("cases")
        .and_then(Value::as_array)
        .ok_or("honesty digest: base cases missing or malformed")?;
    let mut result = BTreeMap::new();
    for case in rows {
        let id = case
            .get("id")
            .and_then(Value::as_str)
            .filter(|v| !v.is_empty())
            .ok_or("honesty digest: case without id")?;
        let report = case
            .get("source_report")
            .and_then(Value::as_str)
            .unwrap_or("");
        if result.insert(id.to_string(), report.to_string()).is_some() {
            return Err("honesty digest: duplicate case id".into());
        }
    }
    Ok(result)
}

/// A required report may leave the list only when its original path is gone.
fn delta_problems(
    base: &Value,
    current: &Value,
    exists: impl Fn(&str) -> Result<bool, String>,
) -> Result<Vec<String>, String> {
    let old = entries(base)?;
    let new = entries(current)?;
    let mut violations = Vec::new();
    for path in old.difference(&new) {
        if exists(path)? {
            violations.push(format!(
                "{path}: required digest entry removed while report still exists (#12856)"
            ));
        }
    }
    Ok(violations)
}

/// Read the base manifest and direct sidecar cases from the pinned Git tree,
/// using the same duplicate-key rejection and ID uniqueness as the live loader.
fn base_corpus(base: &str) -> Result<Value, String> {
    let object = format!("{base}:{EVIDENCE_PROMOTION_HONESTY_CORPUS}");
    let raw = crate::run_output_bytes("git", &["show", &object])
        .map_err(|e| format!("honesty digest: cannot read base corpus: {e}"))?;
    let text = std::str::from_utf8(&raw)
        .map_err(|e| format!("honesty digest: non-UTF-8 base corpus: {e}"))?;
    let mut corpus = super::parse_json_rejecting_duplicate_keys(text)
        .map_err(|e| format!("honesty digest: malformed base corpus: {e}"))?;
    let dir = "fixtures/evidence-promotion-honesty-corpus/cases/";
    let listing =
        crate::run_output_bytes("git", &["ls-tree", "-z", "--name-only", base, "--", dir])
            .map_err(|e| format!("honesty digest: cannot list base cases: {e}"))?;
    if !listing.is_empty() && !listing.ends_with(&[0]) {
        return Err("honesty digest: truncated base case listing".into());
    }
    let rows = corpus
        .get_mut("cases")
        .and_then(Value::as_array_mut)
        .ok_or("honesty digest: base cases missing or malformed")?;
    for bytes in listing.split(|b| *b == 0).filter(|s| !s.is_empty()) {
        let path = std::str::from_utf8(bytes)
            .map_err(|e| format!("honesty digest: non-UTF-8 base case path: {e}"))?;
        let Some(name) = path.strip_prefix(dir) else {
            return Err("honesty digest: unexpected base case path".into());
        };
        if name.contains('/') || !name.ends_with(".json") {
            continue;
        }
        let object = format!("{base}:{path}");
        let raw = crate::run_output_bytes("git", &["show", &object])
            .map_err(|e| format!("honesty digest: cannot read base case {path}: {e}"))?;
        let text = std::str::from_utf8(&raw)
            .map_err(|e| format!("honesty digest: non-UTF-8 base case {path}: {e}"))?;
        let value = super::parse_json_rejecting_duplicate_keys(text)
            .map_err(|e| format!("honesty digest: malformed base case {path}: {e}"))?;
        rows.push(value);
    }
    cases(&corpus)?;
    Ok(corpus)
}

pub(super) fn check(corpus: &Value, violations: &mut Vec<String>) -> Result<(), String> {
    let profile = std::env::var("RIPR_HONESTY_PROFILE").unwrap_or_default();
    let event = std::env::var("GITHUB_EVENT_NAME").unwrap_or_default();
    let admitted = std::env::var("RIPR_HONESTY_ADMITTED_BASE").unwrap_or_default();
    let event_base = std::env::var("RIPR_HONESTY_EVENT_BASE").unwrap_or_default();
    let first = if profile == "qualification"
        || (profile == "legacy" && matches!(event.as_str(), "pull_request" | "merge_group"))
    {
        crate::run_output("git", &["rev-parse", "HEAD^1"])?
            .trim()
            .to_owned()
    } else {
        String::new()
    };
    let local = if profile.is_empty()
        || (profile == "legacy" && !matches!(event.as_str(), "pull_request" | "merge_group"))
    {
        crate::run_output("git", &["merge-base", "origin/main", "HEAD"])
            .map(|s| s.trim().to_owned())
            .unwrap_or_default()
    } else {
        String::new()
    };
    let base = choose_base(&profile, &event, &admitted, &event_base, &first, &local)?;
    let base_corpus = base_corpus(&base)?;
    let found = delta_problems(
        &base_corpus,
        corpus,
        |path| match std::fs::symlink_metadata(path) {
            Ok(_) => Ok(true),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(e) => Err(format!("honesty digest: cannot inspect {path}: {e}")),
        },
    )?;
    violations.extend(found);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    const OLD: &str = "fixtures/evidence-promotion-honesty-corpus/reports/old.json";
    const PROJ: &str = "fixtures/evidence-promotion-honesty-corpus/reports/projection.json";
    const MOVED: &str = "fixtures/evidence-promotion-honesty-corpus/reports/moved.json";
    fn corpus(required: &[&str], pairs: &[(&str, &str)]) -> Value {
        json!({"report_digest_required": required, "cases": pairs.iter().map(|(id, path)| json!({"id": id, "source_report": path})).collect::<Vec<_>>()})
    }
    #[test]
    fn base_selection_is_fail_closed() {
        let a = "a".repeat(40);
        let b = "b".repeat(40);
        assert_eq!(
            choose_base("qualification", "pull_request", &a, "", &a, "").ok(),
            Some(a.clone())
        );
        assert_eq!(
            choose_base("qualification", "pull_request", "", "", &a, &a)
                .err()
                .as_deref(),
            Some("honesty digest: admitted base missing, malformed, or not HEAD's first parent")
        );
        assert_eq!(
            choose_base("qualification", "pull_request", &a, "", &b, &a)
                .err()
                .as_deref(),
            Some("honesty digest: admitted base missing, malformed, or not HEAD's first parent")
        );
        assert_eq!(
            choose_base("legacy", "pull_request", "", "", &a, &a)
                .err()
                .as_deref(),
            Some(
                "honesty digest: legacy event base missing, malformed, or not HEAD's first parent"
            )
        );
        assert_eq!(
            choose_base("legacy", "merge_group", "", "", &a, &a)
                .err()
                .as_deref(),
            Some(
                "honesty digest: legacy event base missing, malformed, or not HEAD's first parent"
            )
        );
        assert_eq!(choose_base("", "", "", "", "", &a).ok(), Some(a.clone()));
        assert_eq!(
            choose_base("", "", "", "", "", "").err().as_deref(),
            Some("honesty digest: no valid local merge base")
        );
        assert_eq!(
            choose_base("legacy", "pull_request", "", &a, &a, "").ok(),
            Some(a.clone())
        );
        assert_eq!(
            choose_base("legacy", "merge_group", "", &a, &b, "")
                .err()
                .as_deref(),
            Some(
                "honesty digest: legacy event base missing, malformed, or not HEAD's first parent"
            )
        );
        assert_eq!(
            entries(&json!({})).err().as_deref(),
            Some("honesty digest: base required list is missing or malformed")
        );
        assert_eq!(
            cases(&json!({})).err().as_deref(),
            Some("honesty digest: base cases missing or malformed")
        );
    }
    #[test]
    fn removed_requirement_requires_original_path_deletion() -> Result<(), String> {
        let base = corpus(&[OLD], &[("capture", OLD), ("projection", PROJ)]);
        let retained = corpus(&[OLD], &[("capture", OLD), ("projection", PROJ)]);
        assert!(delta_problems(&base, &retained, |_| Ok(true))?.is_empty());
        let unlisted = corpus(&[], &[("capture", OLD), ("projection", PROJ)]);
        let found = delta_problems(&base, &unlisted, |_| Ok(true))?;
        assert!(
            found
                .iter()
                .any(|error| error.contains(OLD) && error.contains("still exists"))
        );
        let deletion = corpus(&[], &[("projection", PROJ)]);
        assert!(delta_problems(&base, &deletion, |_| Ok(false))?.is_empty());
        assert_eq!(
            delta_problems(&base, &deletion, |_| Err("unreadable".into()))
                .err()
                .as_deref(),
            Some("unreadable")
        );
        Ok(())
    }

    /// Nonclaim: the path delta cannot distinguish a removed report from the
    /// same report relocated and stripped. Real full-gate evidence is retained
    /// separately; this test does not approve deletion of corpus evidence.
    #[test]
    fn relocation_is_not_detected_nonclaim() -> Result<(), String> {
        let base = corpus(&[OLD], &[("capture", OLD)]);
        let relocated = corpus(&[], &[("capture", MOVED)]);
        assert!(delta_problems(&base, &relocated, |_| Ok(false))?.is_empty());
        Ok(())
    }

    #[test]
    fn base_corpus_reads_pinned_git_sidecars_and_rejects_bad_cases() -> Result<(), String> {
        crate::tests::with_temp_cwd("honesty-base-git", |root| {
            use std::fs;
            let dir = root.join("fixtures/evidence-promotion-honesty-corpus");
            fs::create_dir_all(dir.join("cases/nested")).map_err(|e| e.to_string())?;
            let write = |path: &str, contents: &str| {
                fs::write(dir.join(path), contents).map_err(|e| e.to_string())
            };
            write(
                "corpus.json",
                r#"{"report_digest_required":[],"cases":[{"id":"manifest"}]}"#,
            )?;
            write("cases/a.json", r#"{"id":"sidecar"}"#)?;
            write("cases/ignored.txt", "not json")?;
            write("cases/nested/ignored.json", "not json")?;
            crate::run("git", &["init", "--quiet"])?;
            let commit = || -> Result<String, String> {
                crate::run("git", &["add", "-A"])?;
                crate::run(
                    "git",
                    &[
                        "-c",
                        "user.name=Fixture",
                        "-c",
                        "user.email=fixture@example.invalid",
                        "commit",
                        "--quiet",
                        "-m",
                        "fixture",
                    ],
                )?;
                Ok(crate::run_output("git", &["rev-parse", "HEAD"])?
                    .trim()
                    .to_owned())
            };
            let valid = commit()?;
            // Working-tree changes must not alter the pinned base evidence.
            write("cases/a.json", "{")?;
            let loaded = base_corpus(&valid)?;
            assert_eq!(
                loaded["cases"]
                    .as_array()
                    .ok_or("missing cases")?
                    .iter()
                    .map(|row| row["id"].as_str().unwrap_or(""))
                    .collect::<Vec<_>>(),
                ["manifest", "sidecar"]
            );
            let malformed = commit()?;
            assert!(
                base_corpus(&malformed)
                    .err()
                    .ok_or("malformed base unexpectedly accepted")?
                    .contains("malformed base case")
            );
            write("cases/a.json", r#"{"id":"one","id":"two"}"#)?;
            let duplicate_key = commit()?;
            assert!(
                base_corpus(&duplicate_key)
                    .err()
                    .ok_or("duplicate_key base unexpectedly accepted")?
                    .contains("malformed base case")
            );
            write("cases/a.json", r#"{"id":"manifest"}"#)?;
            let duplicate_id = commit()?;
            assert!(
                base_corpus(&duplicate_id)
                    .err()
                    .ok_or("duplicate_id base unexpectedly accepted")?
                    .contains("duplicate case id")
            );
            write("cases/a.json", r#"{"id":"sidecar"}"#)?;
            write(
                "corpus.json",
                r#"{"cases":[],"cases":[],"report_digest_required":[]}"#,
            )?;
            let duplicate_manifest_key = commit()?;
            assert!(
                base_corpus(&duplicate_manifest_key)
                    .err()
                    .ok_or("duplicate_manifest_key base unexpectedly accepted")?
                    .contains("malformed base corpus")
            );
            Ok(())
        })
    }
}
