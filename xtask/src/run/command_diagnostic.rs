use std::process::ExitStatus;
use std::time::Duration;

use serde::Serialize;

use super::TimedOutput;

/// Observations of one existing child call, never compilation acceptance.
/// No streams, environment values, compiler-unit identity or cache decision.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct CommandDiagnostic {
    pub(crate) program: String,
    pub(crate) argv: Vec<String>,
    pub(crate) outcome: &'static str,
    pub(crate) status: Option<String>,
    pub(crate) elapsed_ms: Option<u128>,
    pub(crate) elapsed_scope: &'static str,
    pub(crate) cargo_progress: Option<CargoProgress>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CargoProgress {
    compiling_lines: Option<usize>,
    checking_lines: Option<usize>,
    fresh_lines: Option<usize>,
}

impl CommandDiagnostic {
    pub(crate) fn captured(
        program: &str,
        argv: &[String],
        result: &Result<TimedOutput, String>,
    ) -> Self {
        match result {
            Ok(output) => Self {
                program: program.to_string(),
                argv: argv.to_vec(),
                outcome: if output.timed_out {
                    "timed_out"
                } else {
                    match output.status {
                        Some(status) if status.success() => "pass",
                        Some(_) => "failed",
                        None => "status_unavailable",
                    }
                },
                status: output.status.map(|status| status.to_string()),
                elapsed_ms: Some(output.duration.as_millis()),
                elapsed_scope: "runner_start_to_observed_exit_excluding_pipe_drain",
                cargo_progress: (program == "cargo")
                    .then(|| CargoProgress::from_stderr(&output.stderr))
                    .flatten(),
            },
            Err(_) => Self {
                program: program.to_string(),
                argv: argv.to_vec(),
                // Capture can fail after launch, polling or pipe draining.
                outcome: "capture_unavailable",
                status: None,
                elapsed_ms: None,
                elapsed_scope: "unavailable",
                cargo_progress: None,
            },
        }
    }

    pub(crate) fn inherited(
        program: &str,
        argv: &[&str],
        result: &Result<ExitStatus, std::io::Error>,
        elapsed: Duration,
    ) -> Self {
        Self {
            program: program.to_string(),
            argv: argv.iter().map(|arg| (*arg).to_string()).collect(),
            outcome: match result {
                Ok(status) if status.success() => "pass",
                Ok(_) => "failed",
                Err(_) => "status_call_unavailable",
            },
            status: match result {
                Ok(status) => Some(status.to_string()),
                Err(_) => None,
            },
            elapsed_ms: Some(elapsed.as_millis()),
            elapsed_scope: "inherited_status_call_including_launch",
            // Existing inherited streams stay inherited, never inferred fresh.
            cargo_progress: None,
        }
    }

    pub(crate) fn render(&self) -> String {
        let cargo = self.cargo_progress.as_ref().map_or_else(
            || "unavailable".to_string(),
            |progress| {
                format!(
                    "recognized_stderr_lines(compiling={}, checking={}, fresh={})",
                    optional_number(progress.compiling_lines),
                    optional_number(progress.checking_lines),
                    optional_number(progress.fresh_lines),
                )
            },
        );
        format!(
            "program={:?}; argv={:?}; outcome={}; status={}; elapsed_ms={}; elapsed_scope={}; cargo_progress={cargo}",
            self.program,
            self.argv,
            self.outcome,
            self.status.as_deref().unwrap_or("unavailable"),
            self.elapsed_ms
                .map_or_else(|| "unavailable".to_string(), |ms| ms.to_string()),
            self.elapsed_scope,
        )
    }
}

fn optional_number(value: Option<usize>) -> String {
    value.map_or_else(|| "unavailable".to_string(), |count| count.to_string())
}

impl CargoProgress {
    fn from_stderr(stderr: &str) -> Option<Self> {
        let count = |prefix| {
            let count = stderr
                .lines()
                .filter(|line| line.trim_start().starts_with(prefix))
                .count();
            (count > 0).then_some(count)
        };
        let progress = Self {
            compiling_lines: count("Compiling "),
            checking_lines: count("Checking "),
            fresh_lines: count("Fresh "),
        };
        (progress.compiling_lines.is_some()
            || progress.checking_lines.is_some()
            || progress.fresh_lines.is_some())
        .then_some(progress)
    }
}

pub(crate) const DISCLOSURE: &str = "Diagnostic observations of existing child calls in this invocation only. Missing rows/values are unavailable evidence, never zero compilation or completed work. Elapsed scopes differ; the captured runner polls exit and excludes pipe draining. Cargo progress counts recognize stderr lines only, not compiler-unit identities, completed artifacts, reuse proof or redundant compilation. These diagnostics waive no gate.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unavailable_capture_and_zero_duration_do_not_claim_compilation() -> Result<(), String> {
        let args = vec!["clippy".to_string(), "argument with spaces".to_string()];
        let missing = CommandDiagnostic::captured(
            "cargo",
            &args,
            &Err("capture error after launch".to_string()),
        );
        let output = TimedOutput {
            status: None,
            stdout: "Finished successfully".to_string(),
            stderr: String::new(),
            duration: Duration::ZERO,
            timed_out: false,
        };
        let zero = CommandDiagnostic::captured("cargo", &args, &Ok(output));
        assert_eq!(missing.outcome, "capture_unavailable");
        assert_eq!(missing.elapsed_ms, None);
        assert_eq!(missing.status, None);
        assert_eq!(zero.outcome, "status_unavailable");
        assert_eq!(zero.elapsed_ms, Some(0));
        assert!(zero.cargo_progress.is_none());
        let json = serde_json::to_value(zero).map_err(|error| error.to_string())?;
        assert_eq!(json["argv"], serde_json::json!(args));
        assert!(json.get("compilation_complete").is_none());
        Ok(())
    }

    #[test]
    fn cargo_progress_is_output_observation_and_absence_is_unavailable() -> Result<(), String> {
        assert!(CargoProgress::from_stderr("Finished dev profile in 0.32s").is_none());
        let progress =
            CargoProgress::from_stderr("    Compiling one v1\n Checking two v2\nnot Fresh three\n")
                .ok_or("recognized Cargo progress was lost")?;
        assert_eq!(progress.compiling_lines, Some(1));
        assert_eq!(progress.checking_lines, Some(1));
        assert_eq!(progress.fresh_lines, None);
        let progress = CargoProgress::from_stderr(" Fresh one v1\nCompiling two v2\n")
            .ok_or("mixed Cargo progress was lost")?;
        assert_eq!(progress.fresh_lines, Some(1));
        assert_eq!(progress.compiling_lines, Some(1));
        Ok(())
    }
}
