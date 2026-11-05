//! Compact publication reuses full evidence context without reusing full records.

use super::*;
use crate::analysis::cancellation::{AnalysisAbortKind, AnalysisCancellationToken};
use crate::analysis::facts::WorkspaceRootAuthority;
use crate::analysis::rust_index::{RaRustSyntaxAdapter, RustSyntaxAdapter};
use crate::analysis::seam_inventory::inventory_seams_from_index;

fn fixture() -> Result<(RustIndex, super::tests::AuthorityFixtureRoot, Vec<RepoSeam>), String> {
    let source = r#"
pub fn value(input: i32) -> i32 {
    if input > 0 { input + 1 } else { 0 }
}
pub fn record(out: &mut Vec<i32>) { out.push(value(2)); }
#[cfg(test)] mod tests {
    use super::*;
    #[test] fn value_pin() { assert_eq!(value(2), 3); }
    #[test] fn record_pin() {
        let mut out = Vec::new(); record(&mut out); assert_eq!(out, vec![3]);
    }
}
"#;
    let path = PathBuf::from("src/lib.rs");
    let facts = RaRustSyntaxAdapter.summarize_file(&path, source)?;
    assert!(!facts.used_lexical_fallback);
    let mut index = RustIndex::default();
    index.extend_tests(facts.tests.iter().cloned());
    index.extend_functions(facts.functions.iter().cloned());
    index.insert_file_only(path.clone(), facts);
    let root = super::tests::claim_index_fixture_root(12_506)?;
    std::fs::create_dir_all(root.join("src")).map_err(|error| error.to_string())?;
    std::fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"compact-windows\"\nversion = \"0.1.0\"\n",
    )
    .map_err(|error| error.to_string())?;
    std::fs::write(root.join(&path), source).map_err(|error| error.to_string())?;
    index.workspace_authority = Some(WorkspaceRootAuthority::from_sources(
        &root,
        std::iter::once((&path, source)),
    ));
    crate::analysis::facts::compose_fixture_source_roles(&mut index, &root);
    let mut seams = inventory_seams_from_index(&[path], &index);
    assert!(seams.len() > 2);
    // The API promises caller order, including repeated seam identities.
    seams.reverse();
    seams.push(seams[0].clone());
    Ok((index, root, seams))
}

fn json(evidence: &[TestGripEvidence]) -> Result<String, String> {
    serde_json::to_string(evidence).map_err(|error| error.to_string())
}

#[test]
fn compact_windows_fresh_and_full_context_match_canonical_compact() -> Result<(), String> {
    let (index, _root, seams) = fixture()?;
    let context = CompactGripContext::new(&index);
    let expected: Vec<_> = seams
        .iter()
        .map(|seam| compact_evidence_for_seam(seam, &context))
        .collect();
    let fresh = EvidencePass::new(&index)
        .compact_evidence_for(&seams)?
        .ok_or("fresh compact context declined")?;
    assert_eq!(json(&fresh)?, json(&expected)?);

    let reused = EvidencePass::new(&index);
    let full = reused.evidence_for(&seams);
    assert_eq!(full.len(), seams.len());
    assert!(full.iter().any(|record| !record.related_tests.is_empty()));
    let mut windows = Vec::new();
    for window in seams.chunks(2) {
        windows.extend(
            reused
                .compact_evidence_for(window)?
                .ok_or("reused context declined")?,
        );
        reused.clear_window_memos();
    }
    assert_eq!(json(&windows)?, json(&expected)?);
    assert_eq!(
        windows
            .iter()
            .map(|record| &record.seam_id)
            .collect::<Vec<_>>(),
        seams.iter().map(RepoSeam::id).collect::<Vec<_>>()
    );
    assert!(windows.iter().all(|record| {
        record.related_tests.is_empty()
            && record.observed_values.is_empty()
            && record.new_test_target.is_none()
    }));
    Ok(())
}

#[test]
fn compact_windows_refuse_both_shared_warm_panic_points() -> Result<(), String> {
    let (index, _root, seams) = fixture()?;
    for point in [1, 2] {
        let pass = EvidencePass::new(&index);
        pass.inject_warm_panic.store(point, Ordering::Release);
        let context = pass.context.as_ref().ok_or("missing context")?;
        pass.warm_shared_tables(context, None, &None);
        assert!(pass.warm_failed.load(Ordering::Acquire));
        assert!(pass.compact_evidence_for(&seams)?.is_none());
        let full = pass.evidence_for(&seams);
        assert!(
            full.iter()
                .all(|record| record.reach.summary == SEAM_EVIDENCE_INCOMPLETE)
        );
        pass.clear_window_memos();
        assert!(pass.compact_evidence_for(&seams)?.is_none());
    }
    Ok(())
}

#[test]
fn compact_windows_panic_permanently_poisons_context() -> Result<(), String> {
    let (index, _root, seams) = fixture()?;
    let pass = EvidencePass::new(&index);
    pass.inject_compact_panic.store(true, Ordering::Release);
    assert!(pass.compact_evidence_for(&seams)?.is_none());
    pass.inject_compact_panic.store(false, Ordering::Release);
    pass.clear_window_memos();
    assert!(pass.compact_evidence_for(&seams)?.is_none());
    let full = pass.evidence_for(&seams);
    assert!(
        full.iter()
            .all(|record| record.reach.summary == SEAM_EVIDENCE_INCOMPLETE)
    );
    assert!(!json(&full)?.contains("injected compact evidence panic"));
    Ok(())
}

#[test]
fn compact_windows_cancelled_and_absent_context_never_return_partial_records() -> Result<(), String>
{
    let (index, _root, seams) = fixture()?;
    let pass = EvidencePass::new(&index);
    let token = AnalysisCancellationToken::new();
    assert!(token.cancel(AnalysisAbortKind::Cancelled));
    for window in [&seams[..], &seams[..0]] {
        let error = cancellation::with_token(&token, || pass.compact_evidence_for(window))
            .err()
            .ok_or("cancelled compact window returned records")?;
        assert!(cancellation::is_cancellation_error(&error));
    }
    assert_eq!(token.observed_abort(), Some(AnalysisAbortKind::Cancelled));
    assert!(!pass.warm_failed.load(Ordering::Acquire));
    assert!(pass.compact_evidence_for(&seams)?.is_some());
    let mut absent = EvidencePass::new(&index);
    absent.context = None;
    assert!(absent.compact_evidence_for(&seams)?.is_none());
    Ok(())
}

#[test]
fn compact_windows_checkpoint_after_entry_and_before_empty_return() -> Result<(), String> {
    let (index, _root, seams) = fixture()?;
    let pass = EvidencePass::new(&index);
    for window in [&seams[..], &seams[..0]] {
        let started = Instant::now();
        let calls = Arc::new(AtomicUsize::new(0));
        let clock_calls = calls.clone();
        let token = AnalysisCancellationToken::with_budget(
            started,
            Duration::from_secs(1),
            Arc::new(move || {
                if clock_calls.fetch_add(1, Ordering::Relaxed) == 0 {
                    started
                } else {
                    started + Duration::from_secs(2)
                }
            }),
        );
        let error = cancellation::with_token(&token, || pass.compact_evidence_for(window))
            .err()
            .ok_or("expired compact window returned records")?;
        assert!(cancellation::is_cancellation_error(&error));
        assert!(calls.load(Ordering::Relaxed) >= 2);
        assert_eq!(
            token.observed_abort(),
            Some(AnalysisAbortKind::DeadlineExceeded)
        );
    }
    Ok(())
}
