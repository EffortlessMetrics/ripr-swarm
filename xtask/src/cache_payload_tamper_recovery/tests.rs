use super::{
    CORPUS_SCHEMA, ExactCandidateExecution, FamilyDisposition, FamilyRow, REQUIRED_CONTROL_IDS,
    REQUIRED_FAMILY_IDS, REQUIRED_TAMPER_OPERATION_IDS, load_committed_corpus, load_corpus,
    validate_corpus,
};

#[test]
fn committed_corpus_passes_offline_validation() -> Result<(), String> {
    let corpus = load_committed_corpus()?;
    validate_corpus(&corpus)
}

#[test]
fn missing_family_is_rejected() -> Result<(), String> {
    let mut corpus = load_committed_corpus()?;
    corpus
        .families
        .retain(|row| row.id != REQUIRED_FAMILY_IDS[0]);
    let error = match validate_corpus(&corpus) {
        Err(error) => error,
        Ok(()) => return Err("missing family must fail".to_string()),
    };
    if !error.contains(REQUIRED_FAMILY_IDS[0]) {
        return Err(format!("unexpected error: {error}"));
    }
    Ok(())
}

#[test]
fn exact_candidate_pass_on_family_is_rejected_in_this_slice() -> Result<(), String> {
    let mut corpus = load_committed_corpus()?;
    corpus.families[0].exact_candidate_execution = ExactCandidateExecution::Passed;
    let error = match validate_corpus(&corpus) {
        Err(error) => error,
        Ok(()) => return Err("passed exact candidate must fail in this corpus slice".to_string()),
    };
    if !error.contains("exact_candidate_execution") {
        return Err(format!("unexpected error: {error}"));
    }
    Ok(())
}

#[test]
fn required_denominators_are_stable() -> Result<(), String> {
    if REQUIRED_FAMILY_IDS.len() != 6 {
        return Err("issue #4515 family denominator changed unexpectedly".to_string());
    }
    if REQUIRED_TAMPER_OPERATION_IDS.len() != 11 {
        return Err("tamper operation denominator changed unexpectedly".to_string());
    }
    if REQUIRED_CONTROL_IDS.len() != 9 {
        return Err("required control denominator changed unexpectedly".to_string());
    }
    if CORPUS_SCHEMA != "cache_payload_tamper_recovery_corpus.v1" {
        return Err("corpus schema token changed unexpectedly".to_string());
    }
    Ok(())
}

#[test]
fn malformed_json_is_rejected() -> Result<(), String> {
    let error = match load_corpus("{") {
        Err(error) => error,
        Ok(_) => return Err("malformed corpus must fail to parse".to_string()),
    };
    if !error.contains("corpus JSON") {
        return Err(format!("unexpected error: {error}"));
    }
    Ok(())
}

#[test]
fn extra_family_with_passed_execution_is_rejected() -> Result<(), String> {
    let mut corpus = load_committed_corpus()?;
    corpus.families.push(FamilyRow {
        id: "unexpected_family".to_string(),
        disposition: FamilyDisposition::IncludedAndExercised,
        library_producer_test: None,
        library_consumer_test: None,
        exact_candidate_execution: ExactCandidateExecution::Passed,
    });
    let error = match validate_corpus(&corpus) {
        Err(error) => error,
        Ok(()) => return Err("extra passed family must fail".to_string()),
    };
    if !error.contains("unexpected_family") {
        return Err(format!("unexpected error: {error}"));
    }
    Ok(())
}

#[test]
fn stale_test_symbol_in_existing_file_is_rejected() -> Result<(), String> {
    let mut corpus = load_committed_corpus()?;
    corpus.families[0].library_producer_test = Some(
        "crates/ripr/src/analysis/facts/build.rs::tests::this_test_name_does_not_exist".to_string(),
    );
    let error = match validate_corpus(&corpus) {
        Err(error) => error,
        Ok(()) => return Err("stale test symbol must fail".to_string()),
    };
    if !error.contains("this_test_name_does_not_exist") {
        return Err(format!("unexpected error: {error}"));
    }
    Ok(())
}
