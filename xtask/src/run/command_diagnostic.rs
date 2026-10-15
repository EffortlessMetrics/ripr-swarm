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
    pub(crate) exit_code: Option<i32>,
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
                exit_code: output.status.and_then(|status| status.code()),
                elapsed_ms: Some(output.duration.as_millis()),
                elapsed_scope: if output.timed_out {
                    #[cfg(unix)]
                    {
                        "runner_start_through_timeout_termination_and_group_confirmation_excluding_pipe_drain"
                    }
                    #[cfg(not(unix))]
                    {
                        "runner_start_through_timeout_termination_excluding_pipe_drain"
                    }
                } else {
                    "runner_start_to_observed_exit_excluding_pipe_drain"
                },
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
                exit_code: None,
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
            exit_code: result.as_ref().ok().and_then(|status| status.code()),
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
            "program={:?}; argv={:?}; outcome={}; status={}; exit_code={}; elapsed_ms={}; elapsed_scope={}; cargo_progress={cargo}",
            self.program,
            self.argv,
            self.outcome,
            self.status.as_deref().unwrap_or("unavailable"),
            self.exit_code
                .map_or_else(|| "unavailable".to_string(), |code| code.to_string()),
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
                .filter(|line| {
                    crate::windows_advisory::strip_ansi(line)
                        .trim_start()
                        .starts_with(prefix)
                })
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

pub(crate) const DISCLOSURE: &str = "Diagnostic observations of existing child calls in this invocation only. Missing rows/values are unavailable evidence, never zero compilation or completed work. Elapsed scopes differ; the captured runner polls exit and excludes pipe draining; enforced timeout duration includes termination/reaping and Unix process-group confirmation where applicable. Cargo progress counts recognize stderr lines only, not compiler-unit identities, completed artifacts, reuse proof or redundant compilation. These diagnostics waive no gate.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colored_cargo_progress_matches_verified_cargo_capture() -> Result<(), String> {
        // Cargo 1.95.0, CARGO_TERM_COLOR=always, build/check/build -v:
        // reset is between the keyword and its separating space.
        let stderr = concat!(
            "\u{1b}[1m\u{1b}[92m   Compiling\u{1b}[0m diagnostic-color-probe v0.1.0\r\n",
            "\u{1b}[1m\u{1b}[92m    Checking\u{1b}[0m diagnostic-color-probe v0.1.0\n",
            "\u{1b}[1m\u{1b}[92m       Fresh\u{1b}[0m diagnostic-color-probe v0.1.0\n",
            " Compiling plain v1\nnot Fresh false\nCheckingish false\n",
        );
        let progress = CargoProgress::from_stderr(stderr).ok_or("colored progress lost")?;
        assert_eq!(progress.compiling_lines, Some(2));
        assert_eq!(progress.checking_lines, Some(1));
        assert_eq!(progress.fresh_lines, Some(1));
        Ok(())
    }

    #[test]
    fn captured_scope_distinguishes_timeout_cleanup_and_unavailable_capture() {
        let args = vec!["test".to_string()];
        let row = |timed_out| {
            CommandDiagnostic::captured(
                "cargo",
                &args,
                &Ok(TimedOutput {
                    status: None,
                    stdout: String::new(),
                    stderr: String::new(),
                    duration: Duration::from_millis(125),
                    timed_out,
                }),
            )
        };
        let normal = row(false);
        let timeout = row(true);
        assert_eq!(normal.elapsed_ms, timeout.elapsed_ms);
        assert_eq!(
            normal.elapsed_scope,
            "runner_start_to_observed_exit_excluding_pipe_drain"
        );
        assert_ne!(normal.elapsed_scope, timeout.elapsed_scope);
        assert!(timeout.elapsed_scope.contains("timeout_termination"));
        assert!(timeout.elapsed_scope.ends_with("excluding_pipe_drain"));
        #[cfg(not(unix))]
        assert!(!timeout.elapsed_scope.contains("group_confirmation"));
        let unavailable = CommandDiagnostic::captured(
            "cargo",
            &args,
            &Err("group confirmation failed".to_string()),
        );
        assert_eq!(unavailable.outcome, "capture_unavailable");
        assert_eq!(unavailable.elapsed_scope, "unavailable");
        assert_eq!(unavailable.elapsed_ms, None);
    }

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

    fn native_exit(code: i32) -> ExitStatus {
        #[cfg(windows)]
        {
            use std::os::windows::process::ExitStatusExt;
            ExitStatus::from_raw(code.cast_unsigned())
        }
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            ExitStatus::from_raw(code << 8)
        }
    }

    #[test]
    fn numeric_native_exits_survive_json_and_rendering() -> Result<(), String> {
        let args = vec!["argument with spaces".to_string()];
        for code in [0, 7] {
            let output = TimedOutput {
                status: Some(native_exit(code)),
                stdout: "success-looking text is not exit evidence".to_string(),
                stderr: String::new(),
                duration: Duration::ZERO,
                timed_out: false,
            };
            for row in [
                CommandDiagnostic::captured("tool", &args, &Ok(output)),
                CommandDiagnostic::inherited(
                    "tool",
                    &["argument with spaces"],
                    &Ok(native_exit(code)),
                    Duration::ZERO,
                ),
            ] {
                let json = serde_json::to_value(&row).map_err(|error| error.to_string())?;
                assert_eq!(json.get("exit_code"), Some(&serde_json::json!(code)));
                assert_eq!(json["argv"], serde_json::json!(args));
                assert_eq!(json["outcome"], if code == 0 { "pass" } else { "failed" });
                assert!(row.render().contains(&format!("; exit_code={code};")));
            }
        }
        Ok(())
    }

    #[test]
    fn missing_native_exit_is_explicit_and_timeout_keeps_its_outcome() -> Result<(), String> {
        let missing_status = TimedOutput {
            status: None,
            stdout: String::new(),
            stderr: String::new(),
            duration: Duration::ZERO,
            timed_out: false,
        };
        let missing = [
            CommandDiagnostic::captured("tool", &[], &Ok(missing_status)),
            CommandDiagnostic::captured("tool", &[], &Err("capture failed".to_string())),
            CommandDiagnostic::inherited(
                "tool",
                &[],
                &Err(std::io::Error::other("status failed")),
                Duration::ZERO,
            ),
        ];
        for row in missing {
            let json = serde_json::to_value(&row).map_err(|error| error.to_string())?;
            assert_eq!(json.get("exit_code"), Some(&serde_json::Value::Null));
            assert!(row.render().contains("; exit_code=unavailable;"));
            assert_ne!(row.outcome, "pass");
        }
        let timed_out = CommandDiagnostic::captured(
            "tool",
            &[],
            &Ok(TimedOutput {
                status: Some(native_exit(0)),
                stdout: String::new(),
                stderr: String::new(),
                duration: Duration::ZERO,
                timed_out: true,
            }),
        );
        let json = serde_json::to_value(&timed_out).map_err(|error| error.to_string())?;
        assert_eq!(json.get("exit_code"), Some(&serde_json::json!(0)));
        assert_eq!(json["outcome"], "timed_out");
        Ok(())
    }

    #[cfg(unix)]
    #[test]
    fn signaled_status_has_no_numeric_exit() -> Result<(), String> {
        use std::os::unix::process::ExitStatusExt;
        let row =
            CommandDiagnostic::inherited("tool", &[], &Ok(ExitStatus::from_raw(9)), Duration::ZERO);
        let json = serde_json::to_value(&row).map_err(|error| error.to_string())?;
        assert_eq!(json.get("exit_code"), Some(&serde_json::Value::Null));
        assert_eq!(json["outcome"], "failed");
        assert!(json["status"].is_string());
        Ok(())
    }
}
