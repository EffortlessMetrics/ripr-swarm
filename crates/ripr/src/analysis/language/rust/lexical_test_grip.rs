//! Related-test honesty for unchanged Rust files indexed on lexical fallback.
//!
//! #4775: when an unchanged test file fails the reference parser, it is
//! indexed lexically. Compact `#[test] fn ...` registrations then drop out of
//! related-test discovery, so a changed production owner can read
//! `no_static_path` while the run stays complete.
//!
//! This module is the single owner that decides whether such a file
//! contributed, or would have contributed, related-test evidence to a
//! classified finding. Unrelated parser-refused files in the same crate do
//! not make the run partial (TypeScript #4261 analog). Changed files stay
//! out: they are the #4722 producer-failure lane, not this claim.

use super::mask_rust_comments_and_strings;
use super::owner_name_from_id;
use crate::analysis::classify::body_contains_owner_call;
use crate::analysis::facts::{FileFacts, RustIndex};
use crate::analysis::rust_index;
use crate::analysis::workspace;
use crate::analysis_outcome::{
    AnalysisLimitation, AnalysisLimitationKind, AnalysisRecovery, AnalysisRecoveryKind,
    AnalysisStage,
};
use crate::domain::Finding;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Taxonomy name JSON consumers can key on. Distinct from #4722's
/// changed-file `producer_failure` limitation.
pub(crate) const LIMITATION_NAME: &str = "rust_lexical_test_index_partial";

const RECOVERY: &str = "An unchanged test file was indexed by lexical fallback after the reference parser refused it, so related-test evidence for a classified owner was weaker or missing. Check whether tests in the named file already exercise the change before trusting a no-path or weak result; rewrite nightly-only syntax or move those tests to a parser-valid file.";

const SAMPLE_FILES: usize = 3;

pub(super) fn limitation_for_consulted_unchanged_lexical_tests(
    index: &RustIndex,
    findings: &[Finding],
    changed_paths: &[PathBuf],
) -> Result<Option<AnalysisLimitation>, String> {
    let gaps = consulted_gaps(index, findings, changed_paths);
    summary_limitation(&gaps)
}

fn consulted_gaps(
    index: &RustIndex,
    findings: &[Finding],
    changed_paths: &[PathBuf],
) -> Vec<PathBuf> {
    if findings.is_empty() {
        return Vec::new();
    }
    let changed = changed_paths
        .iter()
        .map(|path| workspace::normalize_path(path))
        .collect::<BTreeSet<_>>();
    let owner_names = findings
        .iter()
        .filter_map(|finding| {
            owner_name_from_id(&finding.probe.owner, &finding.probe.location.file)
        })
        .collect::<BTreeSet<_>>();
    let related_files = findings
        .iter()
        .flat_map(|finding| finding.related_tests.iter())
        .map(|test| workspace::normalize_path(&test.file))
        .collect::<BTreeSet<_>>();

    let mut gaps = BTreeSet::new();
    for (path, facts) in &index.files {
        let normalized = workspace::normalize_path(path);
        if !facts.used_lexical_fallback || changed.contains(&normalized) {
            continue;
        }
        if !is_test_evidence_file(path, facts) {
            continue;
        }
        let contributed = related_files.contains(&normalized);
        let lost_owner_call = owner_names.iter().any(|owner| {
            body_contains_owner_call(&mask_rust_comments_and_strings(&facts.source), owner)
        });
        if contributed || lost_owner_call {
            gaps.insert(path.clone());
        }
    }
    gaps.into_iter().collect()
}

fn is_test_evidence_file(path: &Path, facts: &FileFacts) -> bool {
    rust_index::is_test_file(path)
        || !facts.tests.is_empty()
        || source_registers_executable_test(&facts.source)
}

fn source_registers_executable_test(source: &str) -> bool {
    source.lines().any(|line| {
        let trimmed = line.trim_start();
        trimmed.starts_with("#[test]")
            || trimmed.starts_with("#[tokio::test")
            || trimmed.starts_with("#[async_std::test")
            || trimmed.starts_with("#[rstest")
    })
}

fn summary_limitation(gaps: &[PathBuf]) -> Result<Option<AnalysisLimitation>, String> {
    let recovery =
        || AnalysisRecovery::new(AnalysisRecoveryKind::InspectFailure, RECOVERY.to_string());
    match gaps {
        [] => Ok(None),
        [path] => Ok(Some(
            AnalysisLimitation::new(
                AnalysisLimitationKind::LanguageScopeUnsupported,
                AnalysisStage::LanguageAdapter,
                recovery()?,
            )
            .with_path(workspace::normalize_path(path))?
            .with_affected_items(1)?
            .with_detail(format!(
                "{LIMITATION_NAME}: unchanged lexical-fallback test file {} contributed or lost related-test evidence for a classified owner",
                workspace::normalize_path(path)
            ))?,
        )),
        _ => {
            let files = gaps
                .iter()
                .map(|path| workspace::normalize_path(path))
                .collect::<BTreeSet<_>>();
            let samples = files.iter().take(SAMPLE_FILES).cloned().collect::<Vec<_>>();
            let remainder = files.len().saturating_sub(samples.len());
            let head = format!(
                "{LIMITATION_NAME}: {} unchanged test file(s) were indexed by lexical fallback while related-test evidence was consulted",
                files.len()
            );
            let mut detail = format!("{head}; e.g. {}", samples.join(", "));
            if remainder > 0 {
                detail.push_str(&format!(" (+{remainder} more)"));
            }
            if detail.chars().count()
                > crate::analysis_outcome::MAX_ANALYSIS_LIMITATION_DETAIL_CHARS
            {
                detail = head;
            }
            Ok(Some(
                AnalysisLimitation::new(
                    AnalysisLimitationKind::LanguageScopeUnsupported,
                    AnalysisStage::LanguageAdapter,
                    recovery()?,
                )
                .with_affected_items(u64::try_from(files.len()).unwrap_or(u64::MAX))?
                .with_detail(detail)?,
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::diff;
    use crate::analysis::facts::TestFact;
    use crate::analysis::language::{LanguageAdapter, LanguageId};
    use crate::analysis::syntax::{RaRustSyntaxAdapter, RustSyntaxAdapter};
    use crate::analysis::{AnalysisMode, AnalysisOptions};
    use crate::config::OraclePolicy;
    use crate::domain::{
        ActivationEvidence, Confidence, DeltaKind, ExposureClass, Probe, ProbeFamily, ProbeId,
        RelatedTest, RevealEvidence, RiprEvidence, SourceCurrentness, SourceLocation,
        StageEvidence, StageState, SymbolId,
    };
    use std::fs;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    const PARSER_REFUSAL: &str = "\nfn refuse_reference_parser(\n";

    fn temp_root(name: &str) -> Result<PathBuf, String> {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let root = std::env::temp_dir().join(format!("ripr-lexical-test-grip-{name}-{stamp}"));
        fs::create_dir_all(&root).map_err(|err| format!("create temp root failed: {err}"))?;
        Ok(root)
    }

    fn write(path: &Path, text: &str) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|err| format!("create parent failed: {err}"))?;
        }
        fs::write(path, text).map_err(|err| format!("write {} failed: {err}", path.display()))
    }

    fn analysis_options(root: PathBuf) -> AnalysisOptions {
        AnalysisOptions {
            root,
            base: None,
            diff_file: None,
            mode: AnalysisMode::Draft,
            resolved_subject_identity: None,
            include_unchanged_tests: true,
            resolve_tsconfig_paths: false,
            perl_facts_path: None,
            git_timeout: None,
            git_candidate: None,
            production_like_targets: Default::default(),
            test_harnesses: Vec::new(),
        }
    }

    fn price_lib() -> &'static str {
        "pub fn price(total: u32, d: u32) -> u32 {\n    if total >= 100 { total - d } else { total }\n}\n"
    }

    fn price_diff() -> &'static str {
        "diff --git a/src/lib.rs b/src/lib.rs\n\
         new file mode 100644\n\
         --- /dev/null\n\
         +++ b/src/lib.rs\n\
         @@ -0,0 +1,3 @@\n\
         +pub fn price(total: u32, d: u32) -> u32 {\n\
         +    if total >= 100 { total - d } else { total }\n\
         +}\n"
    }

    fn compact_owner_test(owner: &str) -> String {
        format!(
            "use demo::{owner};\n#[test] fn p() {{ assert_eq!({owner}(200, 10), 190); }}\n{PARSER_REFUSAL}"
        )
    }

    fn formatted_owner_test(owner: &str) -> String {
        format!(
            "use demo::{owner};\n#[test]\nfn p() {{\n    assert_eq!({owner}(200, 10), 190);\n}}\n{PARSER_REFUSAL}"
        )
    }

    fn formatted_parser_ok_owner_test(owner: &str) -> String {
        format!(
            "use demo::{owner};\n#[test]\nfn p() {{\n    assert_eq!({owner}(200, 10), 190);\n}}\n"
        )
    }

    fn write_demo_crate(root: &Path, test_files: &[(&str, &str)]) -> Result<(), String> {
        write(
            &root.join("Cargo.toml"),
            "[package]\nname='demo'\nversion='0.1.0'\nedition='2024'\n",
        )?;
        write(&root.join("src/lib.rs"), price_lib())?;
        for (path, source) in test_files {
            write(&root.join(path), source)?;
        }
        Ok(())
    }

    fn analyze_price_diff(
        root: PathBuf,
    ) -> Result<crate::analysis::language::LanguageDiffResult, String> {
        let changed_files = diff::parse_unified_diff(price_diff());
        super::super::RustAdapter.analyze_diff(
            &analysis_options(root),
            &OraclePolicy::default(),
            &changed_files,
        )
    }

    fn limitation_details(result: &crate::analysis::language::LanguageDiffResult) -> Vec<String> {
        result
            .limitations
            .iter()
            .filter_map(|limitation| limitation.bounded_detail.clone())
            .collect()
    }

    fn names_this_limitation(result: &crate::analysis::language::LanguageDiffResult) -> bool {
        limitation_details(result)
            .iter()
            .any(|detail| detail.contains(LIMITATION_NAME))
    }

    fn fallback_file(path: &str, source: &str, test_names: &[&str]) -> FileFacts {
        FileFacts {
            path: PathBuf::from(path),
            functions: Vec::new(),
            tests: test_names
                .iter()
                .map(|name| TestFact {
                    name: (*name).to_string(),
                    file: PathBuf::from(path),
                    start_line: 1,
                    end_line: 2,
                    body: source.to_string(),
                    calls: Vec::new(),
                    assertions: Vec::new(),
                    literals: Vec::new(),
                    attrs: Vec::new(),
                    nested_fn_names: Vec::new(),
                    let_bindings: Vec::new(),
                })
                .collect(),
            calls: Vec::new(),
            returns: Vec::new(),
            literals: Vec::new(),
            probe_shapes: Vec::new(),
            used_lexical_fallback: true,
            module_declarations: Vec::new(),
            role_provenance: Default::default(),
            source: source.to_string(),
        }
    }

    fn price_finding(related_file: Option<&str>) -> Finding {
        let stage = |state| StageEvidence::new(state, Confidence::Low, "stage");
        Finding {
            id: "probe:src_lib.rs:predicate:price".to_string(),
            canonical_gap: None,
            probe: Probe {
                id: ProbeId("probe:src_lib.rs:predicate:price".to_string()),
                location: SourceLocation::new("src/lib.rs", 2, 1),
                owner: Some(SymbolId("src/lib.rs::price".to_string())),
                family: ProbeFamily::Predicate,
                delta: DeltaKind::Control,
                before: None,
                after: Some("if total >= 100 {".to_string()),
                expression: "if total >= 100 {".to_string(),
                expected_sinks: Vec::new(),
                required_oracles: Vec::new(),
            },
            class: ExposureClass::NoStaticPath,
            ripr: RiprEvidence {
                reach: stage(StageState::No),
                infect: stage(StageState::Unknown),
                propagate: stage(StageState::Yes),
                reveal: RevealEvidence {
                    observe: stage(StageState::No),
                    discriminate: stage(StageState::No),
                },
            },
            confidence: 0.4,
            evidence: Vec::new(),
            missing: Vec::new(),
            flow_sinks: Vec::new(),
            activation: ActivationEvidence::default(),
            stop_reasons: Vec::new(),
            related_tests: related_file
                .map(|file| RelatedTest {
                    name: "p".to_string(),
                    file: PathBuf::from(file),
                    line: 2,
                    oracle: None,
                    oracle_kind: crate::domain::OracleKind::Unknown,
                    oracle_strength: crate::domain::OracleStrength::None,
                    relation_reason: None,
                    relation_confidence: None,
                })
                .into_iter()
                .collect(),
            recommended_next_step: None,
            language: Some(LanguageId::Rust),
            language_status: None,
            owner_kind: None,
            static_limit_kind: None,
            changed_sink: None,
            observed_sink: None,
            oracle_alignment: None,
            alignment_reason: None,
            source_currentness: SourceCurrentness::CandidateCurrent,
        }
    }

    fn require_named_limitation(
        gaps: Vec<PathBuf>,
        findings: &[Finding],
        changed: &[PathBuf],
    ) -> Result<AnalysisLimitation, String> {
        let mut index = RustIndex::default();
        for path in &gaps {
            let source = format!("#[test] fn p() {{ price(200, 10); }}\n{PARSER_REFUSAL}");
            index.files.insert(
                path.clone(),
                fallback_file(&path.to_string_lossy(), &source, &[]),
            );
        }
        limitation_for_consulted_unchanged_lexical_tests(&index, findings, changed)?
            .ok_or_else(|| "expected a rust_lexical_test_index_partial limitation".to_string())
    }

    #[test]
    fn detector_names_a_dropped_owner_call_in_an_unchanged_fallback_test_file() -> Result<(), String>
    {
        let limitation = require_named_limitation(
            vec![PathBuf::from("tests/price.rs")],
            &[price_finding(None)],
            &[PathBuf::from("src/lib.rs")],
        )?;
        assert_eq!(
            limitation.kind,
            AnalysisLimitationKind::LanguageScopeUnsupported
        );
        assert_eq!(limitation.path.as_deref(), Some("tests/price.rs"));
        let detail = limitation.bounded_detail.as_deref().unwrap_or_default();
        assert!(
            detail.starts_with(LIMITATION_NAME),
            "detail must carry the taxonomy name, got {detail}"
        );
        Ok(())
    }

    #[test]
    fn detector_names_a_fallback_file_that_already_contributed_related_tests() -> Result<(), String>
    {
        let mut index = RustIndex::default();
        index.files.insert(
            PathBuf::from("tests/price.rs"),
            fallback_file(
                "tests/price.rs",
                "#[test]\nfn p() {\n    helper();\n}\n",
                &["p"],
            ),
        );
        let limitation = limitation_for_consulted_unchanged_lexical_tests(
            &index,
            &[price_finding(Some("tests/price.rs"))],
            &[PathBuf::from("src/lib.rs")],
        )?
        .ok_or_else(|| "contributed fallback evidence must be limited".to_string())?;
        assert_eq!(limitation.path.as_deref(), Some("tests/price.rs"));
        Ok(())
    }

    #[test]
    fn detector_ignores_an_unrelated_fallback_file_that_does_not_name_the_owner()
    -> Result<(), String> {
        let mut index = RustIndex::default();
        index.files.insert(
            PathBuf::from("tests/nightly.rs"),
            fallback_file(
                "tests/nightly.rs",
                "#[test] fn p() { other(1); }\nfn refuse(\n",
                &[],
            ),
        );
        let limitation = limitation_for_consulted_unchanged_lexical_tests(
            &index,
            &[price_finding(None)],
            &[PathBuf::from("src/lib.rs")],
        )?;
        assert!(
            limitation.is_none(),
            "an unused nightly-syntax file must not make the run partial, got {limitation:?}"
        );
        Ok(())
    }

    #[test]
    fn detector_ignores_owner_mentions_inside_comments_and_strings() -> Result<(), String> {
        let mut index = RustIndex::default();
        index.files.insert(
            PathBuf::from("tests/nightly.rs"),
            fallback_file(
                "tests/nightly.rs",
                "#[test] fn p() { other(1); }\n// price(200, 10)\nlet _ = \"price(\";\nfn refuse(\n",
                &[],
            ),
        );
        let limitation = limitation_for_consulted_unchanged_lexical_tests(
            &index,
            &[price_finding(None)],
            &[PathBuf::from("src/lib.rs")],
        )?;
        assert!(
            limitation.is_none(),
            "comment/string mentions must not count as lost related-test evidence, got {limitation:?}"
        );
        Ok(())
    }

    #[test]
    fn detector_skips_changed_files_so_it_does_not_absorb_producer_failure() -> Result<(), String> {
        let mut index = RustIndex::default();
        index.files.insert(
            PathBuf::from("tests/price.rs"),
            fallback_file(
                "tests/price.rs",
                "#[test] fn p() { price(200, 10); }\nfn refuse(\n",
                &[],
            ),
        );
        let limitation = limitation_for_consulted_unchanged_lexical_tests(
            &index,
            &[price_finding(None)],
            &[PathBuf::from("tests/price.rs")],
        )?;
        assert!(
            limitation.is_none(),
            "changed lexical-fallback files belong to #4722, got {limitation:?}"
        );
        Ok(())
    }

    #[test]
    fn detector_stays_quiet_when_no_owner_was_classified() -> Result<(), String> {
        let mut index = RustIndex::default();
        index.files.insert(
            PathBuf::from("tests/price.rs"),
            fallback_file(
                "tests/price.rs",
                "#[test] fn p() { price(200, 10); }\nfn refuse(\n",
                &[],
            ),
        );
        let limitation = limitation_for_consulted_unchanged_lexical_tests(
            &index,
            &[],
            &[PathBuf::from("src/lib.rs")],
        )?;
        assert!(limitation.is_none());
        Ok(())
    }

    #[test]
    fn detector_collapses_several_consulted_files_into_one_summary() -> Result<(), String> {
        let limitation = require_named_limitation(
            vec![
                PathBuf::from("tests/a.rs"),
                PathBuf::from("tests/b.rs"),
                PathBuf::from("tests/c.rs"),
                PathBuf::from("tests/d.rs"),
                PathBuf::from("tests/e.rs"),
            ],
            &[price_finding(None)],
            &[PathBuf::from("src/lib.rs")],
        )?;
        assert!(limitation.path.is_none());
        assert_eq!(limitation.affected_items, Some(5));
        let detail = limitation.bounded_detail.as_deref().unwrap_or_default();
        assert!(
            detail.starts_with(
                "rust_lexical_test_index_partial: 5 unchanged test file(s) were indexed by lexical fallback while related-test evidence was consulted; e.g. "
            ) && detail.ends_with("(+2 more)"),
            "{detail}"
        );
        Ok(())
    }

    #[test]
    fn compact_unchanged_unparseable_test_file_is_a_partial_adapter_result() -> Result<(), String> {
        let source = compact_owner_test("price");
        RaRustSyntaxAdapter
            .summarize_file(Path::new("tests/price.rs"), &source)
            .err()
            .ok_or_else(|| {
                "fixture precondition: the reference parser must refuse the compact test file"
                    .to_string()
            })?;

        let root = temp_root("compact-dropout")?;
        write_demo_crate(&root, &[("tests/price.rs", source.as_str())])?;
        let result = analyze_price_diff(root.clone())?;
        assert!(
            names_this_limitation(&result),
            "dropped tests in an unchanged lexical-fallback file must be a typed limitation, got {:?}",
            result.limitations
        );
        assert!(
            result.findings.iter().any(|finding| {
                finding.class == ExposureClass::NoStaticPath && finding.related_tests.is_empty()
            }),
            "the compact lexical scan must actually drop the discriminating test: {:?}",
            result.findings
        );
        let _ = fs::remove_dir_all(root);
        Ok(())
    }

    #[test]
    fn formatted_unchanged_unparseable_test_file_is_still_limited_once_it_relates()
    -> Result<(), String> {
        let source = formatted_owner_test("price");
        RaRustSyntaxAdapter
            .summarize_file(Path::new("tests/price.rs"), &source)
            .err()
            .ok_or_else(|| {
                "fixture precondition: the reference parser must refuse the formatted test file"
                    .to_string()
            })?;

        let root = temp_root("formatted-contributed")?;
        write_demo_crate(&root, &[("tests/price.rs", source.as_str())])?;
        let result = analyze_price_diff(root.clone())?;
        assert!(
            names_this_limitation(&result),
            "lexical-fallback related-test evidence must still mark the run limited, got {:?}",
            result.limitations
        );
        assert!(
            result.findings.iter().any(|finding| finding
                .related_tests
                .iter()
                .any(|test| { workspace::normalize_path(&test.file) == "tests/price.rs" })),
            "the formatted lexical scan must still extract the test: {:?}",
            result.findings
        );
        let _ = fs::remove_dir_all(root);
        Ok(())
    }

    #[test]
    fn unrelated_unchanged_unparseable_test_file_does_not_make_the_crate_partial()
    -> Result<(), String> {
        let nightly = compact_owner_test("other");
        let ok = formatted_parser_ok_owner_test("price");
        RaRustSyntaxAdapter
            .summarize_file(Path::new("tests/nightly.rs"), &nightly)
            .err()
            .ok_or_else(|| {
                "fixture precondition: nightly.rs must fail the reference parser".to_string()
            })?;
        RaRustSyntaxAdapter
            .summarize_file(Path::new("tests/ok.rs"), &ok)
            .map_err(|err| format!("fixture precondition: ok.rs must parse: {err}"))?;

        let root = temp_root("unrelated-nightly")?;
        write_demo_crate(
            &root,
            &[
                ("tests/nightly.rs", nightly.as_str()),
                ("tests/ok.rs", ok.as_str()),
            ],
        )?;
        let result = analyze_price_diff(root.clone())?;
        assert!(
            !names_this_limitation(&result),
            "an unused nightly-syntax file must not make a crate with a parser-backed discriminator partial, got {:?}",
            result.limitations
        );
        assert!(
            result
                .findings
                .iter()
                .any(|finding| !finding.related_tests.is_empty()),
            "the parser-backed test must still relate: {:?}",
            result.findings
        );
        let _ = fs::remove_dir_all(root);
        Ok(())
    }

    #[test]
    fn parser_backed_unchanged_test_file_stays_complete() -> Result<(), String> {
        let source = formatted_parser_ok_owner_test("price");
        RaRustSyntaxAdapter
            .summarize_file(Path::new("tests/price.rs"), &source)
            .map_err(|err| format!("fixture precondition: parser-ok file must parse: {err}"))?;

        let root = temp_root("parser-ok")?;
        write_demo_crate(&root, &[("tests/price.rs", source.as_str())])?;
        let result = analyze_price_diff(root.clone())?;
        assert!(
            !names_this_limitation(&result),
            "parser-backed related tests must not be reported as lexical-fallback gaps, got {:?}",
            result.limitations
        );
        assert!(
            result
                .findings
                .iter()
                .any(|finding| !finding.related_tests.is_empty()),
            "parser-backed tests must relate: {:?}",
            result.findings
        );
        let _ = fs::remove_dir_all(root);
        Ok(())
    }

    #[test]
    fn changed_unparseable_test_file_is_not_this_limitation() -> Result<(), String> {
        let source = compact_owner_test("price");
        let root = temp_root("changed-test-file")?;
        write_demo_crate(&root, &[("tests/price.rs", source.as_str())])?;
        let changed_files = diff::parse_unified_diff(
            "diff --git a/tests/price.rs b/tests/price.rs\n\
             new file mode 100644\n\
             --- /dev/null\n\
             +++ b/tests/price.rs\n\
             @@ -0,0 +1,3 @@\n\
             +use demo::price;\n\
             +#[test] fn p() { assert_eq!(price(200, 10), 190); }\n\
             +fn refuse_reference_parser(\n",
        );
        let result = super::super::RustAdapter.analyze_diff(
            &analysis_options(root.clone()),
            &OraclePolicy::default(),
            &changed_files,
        )?;
        assert!(
            !names_this_limitation(&result),
            "a changed test file is #4722, not #4775, got {:?}",
            result.limitations
        );
        let _ = fs::remove_dir_all(root);
        Ok(())
    }
}
