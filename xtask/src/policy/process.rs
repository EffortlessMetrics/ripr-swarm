/// The process gate also pins numeric `process::exit` literals in the CLI
/// control paths (#12505), so the exit-vocabulary guard runs wherever
/// `check-process-policy` is enforced (precommit and the hosted lanes).
pub(crate) fn check_process_policy() -> Result<(), String> {
    crate::check_process_policy_impl()?;
    crate::cli_exit_contract::check_cli_exit_contract_impl()
}

#[cfg(test)]
mod tests {
    use crate::tests::temp_dir;
    use crate::{count_policy_fragments, parse_count_policy_allowlist};
    use std::fs;

    const LIST: &str = "policy/process_allowlist.txt";

    /// Each required field is checked on its own: a blank path, pattern,
    /// owner or reason refuses the row with its line, and so does a count
    /// that is not a number.
    #[test]
    fn count_allowlist_refuses_each_blank_field_and_a_bad_count_by_line() -> Result<(), String> {
        let cases = [
            (
                " |Marker|1|owner|reason",
                "2 allowlist entries require path, pattern, owner, and reason",
            ),
            (
                "a.rs| |1|owner|reason",
                "2 allowlist entries require path, pattern, owner, and reason",
            ),
            (
                "a.rs|Marker|1| |reason",
                "2 allowlist entries require path, pattern, owner, and reason",
            ),
            (
                "a.rs|Marker|1|owner| ",
                "2 allowlist entries require path, pattern, owner, and reason",
            ),
            ("a.rs|Marker|many|owner|reason", "2 invalid max_count: "),
        ];
        for (row, expected) in cases {
            let text = format!("# header\n{row}\n");
            match parse_count_policy_allowlist(LIST, &text) {
                Ok(rows) => return Err(format!("{row:?} was accepted: {rows:?}")),
                Err(error) => assert!(
                    error.starts_with(&format!("{LIST}:{expected}")),
                    "{row:?}: {error}"
                ),
            }
        }
        Ok(())
    }

    /// Only an absent fragment directory is optional. A fragment path that
    /// exists but cannot be listed as a directory is a read error, not an
    /// empty set of rows.
    #[test]
    fn count_policy_fragments_treat_only_an_absent_directory_as_empty() -> Result<(), String> {
        let root = temp_dir("count-policy-fragments-absent");
        let list = root.join("process_allowlist.txt");
        let list = list.to_string_lossy();
        assert_eq!(count_policy_fragments(&list)?, Vec::new());

        let fragments = root.join("process_allowlist.d");
        fs::write(&fragments, "not a directory\n").map_err(|error| error.to_string())?;
        let error = match count_policy_fragments(&list) {
            Ok(rows) => {
                return Err(format!(
                    "a file in place of the directory was read as {rows:?}"
                ));
            }
            Err(error) => error,
        };
        assert!(
            error.contains("process_allowlist.d: cannot read: "),
            "{error}"
        );
        Ok(())
    }
}
