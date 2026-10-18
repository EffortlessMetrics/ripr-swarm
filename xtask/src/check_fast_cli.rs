//! Execute the existing help/parser validator; do not rediscover parser owners.

pub(super) const GATE: &str = "cli-help-parity";
const TEST: &str = "cli::help::tests::every_parsed_flag_agrees_with_its_help_documentation";

pub(super) fn run(diagnostics: &mut Vec<crate::run::CommandDiagnostic>) -> Result<(), String> {
    let args = [
        "test", "--locked", "-p", "ripr", "--lib", TEST, "--", "--exact", "--format", "pretty",
        "--color", "never",
    ]
    .map(str::to_string);
    eprintln!("$ cargo {}", args.join(" "));
    let captured = crate::run::capture_output_without_timeout("cargo", &args, &[], GATE);
    diagnostics.push(crate::run::CommandDiagnostic::captured(
        "cargo", &args, &captured,
    ));
    let output = captured?;
    print!("{}", output.stdout);
    eprint!("{}", output.stderr);
    let status = output
        .status
        .ok_or("CLI help parity produced no native status")?;
    if output.timed_out || !status.success() {
        return Err(format!("CLI help parity failed with {status}"));
    }
    validate_execution(&output.stdout)
}

fn validate_execution(stdout: &str) -> Result<(), String> {
    // Cargo succeeds for an exact filter matching zero tests. Require the
    // named nonignored case and the one-case libtest result, not merely exit 0.
    let expected = format!("test {TEST} ... ok");
    if stdout
        .lines()
        .filter(|line| line.trim() == expected)
        .count()
        == 1
        && stdout
            .lines()
            .any(|line| line.starts_with("test result: ok. 1 passed; 0 failed; 0 ignored;"))
    {
        Ok(())
    } else {
        Err(format!(
            "CLI help parity execution not established: expected exactly one passing `{TEST}`; update the focused selector if the test moved"
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_passing_case_establishes_execution() -> Result<(), String> {
        validate_execution(&format!(
            "\nrunning 1 test\ntest {TEST} ... ok\n\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 11474 filtered out; finished in 0.35s\n"
        ))
    }

    #[test]
    fn zero_ignored_or_wrong_case_cannot_establish_execution() {
        for output in [
            "running 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored;",
            &format!("test {TEST} ... ok\n"),
            &format!(
                "test {TEST}_renamed ... ok\n\ntest result: ok. 1 passed; 0 failed; 0 ignored;"
            ),
            &format!("test {TEST} ... ignored\n\ntest result: ok. 0 passed; 0 failed; 1 ignored;"),
            &format!("test {TEST} ... ok\n\ntest result: ok. 2 passed; 0 failed; 0 ignored;"),
            &format!(
                "test {TEST} ... ok\ntest {TEST} ... ok\n\ntest result: ok. 1 passed; 0 failed; 0 ignored;"
            ),
        ] {
            assert!(validate_execution(output).is_err(), "accepted {output:?}");
        }
    }
}
