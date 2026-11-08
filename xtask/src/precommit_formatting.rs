//! Formatting belongs to local precommit or the explicit hosted preflight.
//! A ci-policy pass does not establish the separate formatting obligation.

pub(crate) fn run(
    ci_policy: bool,
    formatting: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    if ci_policy { Ok(()) } else { formatting() }
}

#[cfg(test)]
mod tests {
    use super::run;

    #[test]
    fn local_precommit_executes_formatting_once() {
        let mut calls = 0;
        let result = run(false, || {
            calls += 1;
            Ok(())
        });
        assert_eq!(result, Ok(()));
        assert_eq!(calls, 1);
    }

    #[test]
    fn ci_policy_does_not_execute_a_second_formatter() {
        let mut calls = 0;
        let result = run(true, || {
            calls += 1;
            Err("duplicate formatter executed".to_string())
        });
        assert_eq!(result, Ok(()));
        assert_eq!(calls, 0);
    }

    #[test]
    fn local_formatting_failure_is_not_suppressed() {
        let result = run(false, || Err("seeded formatting defect".to_string()));
        assert_eq!(result, Err("seeded formatting defect".to_string()));
    }
}
