//! Follow-through from #9574 to #9575: exercise the retained stage entrypoint.

use super::*;
use crate::analysis::cancellation::{self, AnalysisAbortKind, AnalysisCancellationToken};
use std::collections::BTreeSet;
use std::fmt::Write;

fn thousand_seam_fixture() -> Result<(FixtureIndex, Vec<RepoSeam>), String> {
    let mut source = String::new();
    for ordinal in 0..1_000 {
        writeln!(source, "pub fn value_{ordinal}() -> i32 {{ {ordinal} }}")
            .map_err(|error| error.to_string())?;
    }
    let path = PathBuf::from("src/values.rs");
    let index = index_from_files(&[(path.clone(), &source)])?;
    let facts = index.files().get(&path).ok_or("missing parsed source")?;
    assert!(!facts.used_lexical_fallback);
    let seams = inventory_seams_from_index(&[path], &index);
    assert_eq!(seams.len(), 1_000, "one real return seam per parsed owner");
    assert_eq!(
        seams
            .iter()
            .map(|seam| seam.id().as_str())
            .collect::<BTreeSet<_>>()
            .len(),
        1_000,
        "threshold subjects must have distinct identities"
    );
    Ok((index, seams))
}

#[test]
fn evidence_entrypoint_dispatches_warm_at_1000_but_not_999() -> Result<(), String> {
    let (index, seams) = thousand_seam_fixture()?;
    let pass = EvidencePass::new(&index);
    let context = pass.context.as_ref().ok_or("fixture context must build")?;
    assert_eq!(context.workspace_scan_passes(), 0);

    let cold_small = pass.evidence_for(&seams[..999]);
    assert_eq!(cold_small.len(), 999);
    assert!(!pass.shared_tables_warmed.is_completed());
    assert_eq!(context.workspace_scan_passes(), 0);

    let large = pass.evidence_for(&seams);
    assert_eq!(large.len(), 1_000);
    assert!(pass.shared_tables_warmed.is_completed());
    let paid = context.workspace_scan_passes();
    // Retain the ambiguity-pass plus completion allowance from #9574.
    assert!((1..=2).contains(&paid), "expected warm scan, got {paid}");

    let warm_small = pass.evidence_for(&seams[..999]);
    assert_eq!(context.workspace_scan_passes(), paid);
    assert_eq!(
        serde_json::to_string(&cold_small).map_err(|error| error.to_string())?,
        serde_json::to_string(&warm_small).map_err(|error| error.to_string())?,
        "warming must preserve the evidence for the smaller stage"
    );
    eprintln!(
        "#9575 entrypoint: 999 seams => 0 warm scans; 1000 => {paid}; repeated stage => {paid}"
    );
    Ok(())
}

#[test]
fn evidence_entrypoint_cancelled_1000_pays_zero_warm_scans() -> Result<(), String> {
    let (index, seams) = thousand_seam_fixture()?;
    // Build outside cancellation so this cannot pass by losing the context.
    let pass = EvidencePass::new(&index);
    let context = pass.context.as_ref().ok_or("fixture context must build")?;
    assert_eq!(context.workspace_scan_passes(), 0);
    assert!(!pass.shared_tables_warmed.is_completed());
    let token = AnalysisCancellationToken::new();
    assert!(token.cancel(AnalysisAbortKind::Cancelled));
    let evidence = cancellation::with_token(&token, || pass.evidence_for(&seams));
    assert!(
        pass.shared_tables_warmed.is_completed(),
        "large-stage warm dispatch must run"
    );
    assert!(evidence.is_empty());
    assert_eq!(context.workspace_scan_passes(), 0);
    assert_eq!(token.observed_abort(), Some(AnalysisAbortKind::Cancelled));
    eprintln!(
        "#9575 entrypoint: already-cancelled 1000 seams => warm dispatch reached, 0 scans, 0 evidence, typed Cancelled observed"
    );
    Ok(())
}
