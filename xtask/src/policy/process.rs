/// The process gate also pins numeric `process::exit` literals in the CLI
/// control paths (#12505), so the exit-vocabulary guard runs wherever
/// `check-process-policy` is enforced (precommit and the hosted lanes).
pub(crate) fn check_process_policy() -> Result<(), String> {
    crate::check_process_policy_impl()?;
    crate::cli_exit_contract::check_cli_exit_contract_impl()
}
