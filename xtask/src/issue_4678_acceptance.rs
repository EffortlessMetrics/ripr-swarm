//! Acceptance witnesses for GitHub #4678: issue lifecycle and burn-down bound
//! to portfolio-selected work identity (#1706 delivery slice PR C).
//!
//! Most of the slice landed with PR #8325 (RIPR-SPEC-0274..0277 plus the
//! RIPR-SPEC-0218 lifecycle counting law). These tests pin the issue's
//! acceptance predicates to committed artifacts without inventing authority.

#[cfg(test)]
mod tests {
    use std::fs;

    use crate::issue_lifecycle_attempt::{
        IssueLifecycleDispositionV1, assess_issue_lifecycle_attempt,
    };
    use crate::issue_lifecycle_real_rows::load_real_rows_corpus;
    use crate::work_burndown_identity::{
        DEFAULT_WORK_BURNDOWN_CORPUS_DIR, load_work_burndown_corpus, run_work_burndown_check,
    };
    use crate::work_portfolio::workspace_path;
    use crate::work_selection_identity::{
        DEFAULT_WORK_SELECTION_CORPUS_DIR, WORK_IDENTITY_FRESH_ROOT_DOMAINS,
        load_work_identity_fresh_root_receipt, load_work_selection_corpus,
        run_work_selection_check, verify_work_identity_fresh_root_receipt,
    };
    use crate::work_wave_identity::{
        DEFAULT_WORK_WAVE_CORPUS_DIR, load_work_wave_corpus, run_work_wave_check,
    };

    /// Acceptance: burn-down and lifecycle consumers embed `SelectedWorkIdentityV1`
    /// and fail closed on wrong repository/snapshot/basis (PR #8325 / 0274).
    #[test]
    fn issue_4678_burndown_committed_corpus_binds_selected_work() -> Result<(), String> {
        let body = fs::read_to_string(
            workspace_path(DEFAULT_WORK_BURNDOWN_CORPUS_DIR).join("corpus.json"),
        )
        .map_err(|error| format!("read burn-down corpus: {error}"))?;
        let corpus = load_work_burndown_corpus(&body)?;
        let view = run_work_burndown_check(DEFAULT_WORK_BURNDOWN_CORPUS_DIR, &corpus)?;
        if view.counts.failed != 0 {
            return Err(format!(
                "burn-down check observed {} failing cases",
                view.counts.failed
            ));
        }
        Ok(())
    }

    /// Acceptance: verified/closed requires acceptance evidence; merge alone is
    /// not completed closeout (RIPR-SPEC-0218 counting law).
    #[test]
    fn issue_4678_lifecycle_merge_without_closeout_stays_pending() -> Result<(), String> {
        let corpus = load_real_rows_corpus()?;
        let row_b = corpus
            .rows
            .iter()
            .find(|row| row.lifecycle_id == "lifecycle-real-8325-1706-pending-closeout")
            .ok_or_else(|| "missing real row B".to_string())?;
        let assessed = assess_issue_lifecycle_attempt(row_b);
        if assessed.disposition != Some(IssueLifecycleDispositionV1::MergedPendingCloseout) {
            return Err(format!(
                "row B must stay merged_pending_closeout, got {:?}",
                assessed.disposition
            ));
        }
        Ok(())
    }

    /// Acceptance: cross-repository / wrong-basis selection fails closed
    /// (shared identity law engine, RIPR-SPEC-0244 scenario-09).
    #[test]
    fn issue_4678_selection_wrong_repository_fails_closed() -> Result<(), String> {
        let body = fs::read_to_string(
            workspace_path(DEFAULT_WORK_SELECTION_CORPUS_DIR).join("corpus.json"),
        )
        .map_err(|error| format!("read selection corpus: {error}"))?;
        let corpus = load_work_selection_corpus(&body)?;
        let view = run_work_selection_check(DEFAULT_WORK_SELECTION_CORPUS_DIR, &corpus)?;
        let scenario = view
            .results
            .iter()
            .find(|scenario| scenario.scenario_id == "scenario-09-wrong-identities-fail-visibly")
            .ok_or_else(|| "missing wrong-identity scenario".to_string())?;
        let failing = scenario
            .cases
            .iter()
            .filter(|case| !case.violations.is_empty())
            .count();
        if failing == 0 {
            return Err("wrong-identity scenario must fail at least one case".to_string());
        }
        Ok(())
    }

    /// Acceptance: several campaigns can progress independently when resources
    /// are disjoint (RIPR-SPEC-0271 wave corpus).
    #[test]
    fn issue_4678_disjoint_cross_campaign_waves_admitted() -> Result<(), String> {
        let body =
            fs::read_to_string(workspace_path(DEFAULT_WORK_WAVE_CORPUS_DIR).join("corpus.json"))
                .map_err(|error| format!("read wave corpus: {error}"))?;
        let corpus = load_work_wave_corpus(&body)?;
        let view = run_work_wave_check(DEFAULT_WORK_WAVE_CORPUS_DIR, &corpus)?;
        let scenario = view
            .results
            .iter()
            .find(|scenario| scenario.scenario_id == "wave-01-disjoint-cross-campaign-waves-pass")
            .ok_or_else(|| "missing disjoint wave scenario".to_string())?;
        let failing = scenario
            .cases
            .iter()
            .filter(|case| case.status == "failed")
            .count();
        if failing != 0 {
            return Err(format!("disjoint wave scenario failed {failing} cases"));
        }
        Ok(())
    }

    /// Acceptance: fresh-root reconstruction includes burn-down among the ten
    /// PR B consumers without chat or active.toml authority (#8325).
    #[test]
    fn issue_4678_fresh_root_receipt_covers_burndown_domain() -> Result<(), String> {
        let receipt = load_work_identity_fresh_root_receipt()?;
        verify_work_identity_fresh_root_receipt(&receipt)?;
        let has_burndown = receipt
            .domains
            .iter()
            .any(|domain| domain.domain == "burndown");
        if !has_burndown {
            return Err("fresh-root receipt must record the burndown domain".to_string());
        }
        if WORK_IDENTITY_FRESH_ROOT_DOMAINS.len() != 10 {
            return Err("expected ten fresh-root domains".to_string());
        }
        Ok(())
    }

    /// Issue proof block names `portfolio_burndown`; alias the burn-down suite.
    #[test]
    fn portfolio_burndown_committed_corpus_matches_work_burndown_law() -> Result<(), String> {
        issue_4678_burndown_committed_corpus_binds_selected_work()
    }
}
