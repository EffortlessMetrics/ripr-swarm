use super::*;
use crate::analysis::classify::{
    BooleanComparisonReceipt, OwnerPinSyntax, boolean_comparison_receipt,
};

const SOURCE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/rust-verdict-corpus/subjects/authored-mined-wire/src/lib.rs.txt"
));

#[test]
fn same_asserted_helper_receipt_owns_repository_return_evidence() -> Result<(), String> {
    let path = PathBuf::from("src/lib.rs");
    let index = index_from_files(&[(path.clone(), SOURCE)])?;
    let seams = inventory_seams_from_index(&[path], &index);
    let seam = seams
        .iter()
        .find(|seam| {
            seam.kind() == SeamKind::ReturnValue
                && seam.expression().starts_with("u32::from_be_bytes")
        })
        .ok_or("exact u32 return seam")?;
    let evidence = evidence_for_seam(seam, &index);
    assert_eq!(evidence.activate.state, StageState::Yes, "{evidence:?}");
    assert_eq!(evidence.propagate.state, StageState::Yes, "{evidence:?}");
    assert_eq!(evidence.discriminate.state, StageState::Yes, "{evidence:?}");
    assert_eq!(
        crate::analysis::seam_classification::classify_seam(seam, &evidence),
        SeamGripClass::StronglyGripped,
        "{evidence:?}"
    );
    assert!(
        evidence
            .propagate
            .summary
            .contains("asserted_u32_helper_comparison"),
        "{evidence:?}"
    );
    let inputs: Vec<_> = evidence
        .observed_values
        .iter()
        .filter(|fact| fact.value.starts_with("bytes = "))
        .collect();
    assert_eq!(inputs.len(), 4, "{inputs:?}");
    let compact_context = CompactGripContext::new(&index);
    let compact = compact_evidence_for_seam(seam, &compact_context);
    assert_eq!(compact.reach.state, evidence.reach.state);
    assert_eq!(compact.activate.state, evidence.activate.state);
    assert_eq!(compact.propagate.state, evidence.propagate.state);
    assert_eq!(compact.discriminate.state, evidence.discriminate.state);
    assert!(
        compact.observed_values.is_empty(),
        "compact output deliberately omits input payloads"
    );
    Ok(())
}

#[test]
fn comparison_receipt_requires_live_source_owner_and_exact_asserted_identity() -> Result<(), String>
{
    let index = index_from_files(&[(PathBuf::from("src/lib.rs"), SOURCE)])?;
    let functions = index.functions();
    let owner = functions
        .iter()
        .find(|function| function.name == "read_u32")
        .ok_or("owner")?;
    let tests = index.tests();
    let test = tests
        .iter()
        .find(|test| test.name == "u32_reads_what_was_written")
        .ok_or("test")?;
    let syntax = OwnerPinSyntax::default();
    let receipt =
        boolean_comparison_receipt(owner, test, &index, &syntax).ok_or("source receipt")?;
    let assertion = receipt.oracle();
    assert!(receipt.matches_source(
        &owner.file,
        26,
        "u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])"
    ));
    assert!(receipt.admits(test, &assertion));
    assert!(
        test.assertions.is_empty(),
        "source receipt must not mutate raw parser facts"
    );
    let mut wrong_assertion = assertion.clone();
    wrong_assertion.line += 1;
    assert!(!receipt.admits(test, &wrong_assertion));
    assert!(
        boolean_comparison_receipt(&owner.clone(), test, &index, &syntax).is_none(),
        "copied owner cannot nominate live source authority"
    );
    let mut forged = serde_json::to_value(&receipt).map_err(|error| error.to_string())?;
    forged["samples"] = serde_json::json!([7]);
    let forged: BooleanComparisonReceipt =
        serde_json::from_value(forged).map_err(|error| error.to_string())?;
    assert!(!forged.matches_source(
        &owner.file,
        26,
        "u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])"
    ));
    Ok(())
}
