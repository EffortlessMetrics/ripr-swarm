//! Compatibility facade for the producer-owned analysis-outcome contract.
//! Definitions and validation live in `ripr-contract`; existing crate-private
//! consumer paths stay here (#12726 sub-slice 2c).

#[cfg(test)]
pub(crate) use ripr_contract::analysis_outcome::ANALYSIS_OUTCOME_CLAIM_BOUNDARY;
pub(crate) use ripr_contract::analysis_outcome::{
    AnalysisIdentity, AnalysisLimitation, AnalysisLimitationKind, AnalysisOutcome,
    AnalysisOutcomeCounts, AnalysisOutcomeKind, AnalysisRecovery, AnalysisRecoveryKind,
    AnalysisStage, CheckFindingsInputs, EmptyScopeDiffAuthority, GitCandidateSubjectIdentity,
    MAX_ANALYSIS_LIMITATION_DETAIL_CHARS, PYTHON_PARSE_FAILURE_RECOVERY, RUST_NOT_UTF8_RECOVERY,
    RUST_PARSE_FAILURE_RECOVERY, TYPESCRIPT_PARSE_FAILURE_RECOVERY,
    normalize_portable_analysis_path,
};
