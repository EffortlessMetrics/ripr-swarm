# check-fast report

Status: failed

Original failure:
```text
CLI help parity failed with exit status: 101
```

## Child command diagnostics

Diagnostic observations of existing child calls in this invocation only. Missing rows/values are unavailable evidence, never zero compilation or completed work. Elapsed scopes differ; the captured runner polls exit and excludes pipe draining; enforced timeout duration includes termination/reaping and Unix process-group confirmation where applicable. Cargo progress counts recognize stderr lines only, not compiler-unit identities, completed artifacts, reuse proof or redundant compilation. These diagnostics waive no gate. Capture scope: fmt and workspace Clippy inherit streams; selected CLI help parity captures streams and reports recognized Cargo stderr progress.

- program="cargo"; argv=["fmt", "--check"]; outcome=pass; status=exit status: 0; exit_code=0; elapsed_ms=15595; elapsed_scope=inherited_status_call_including_launch; cargo_progress=unavailable
- program="cargo"; argv=["test", "--locked", "-p", "ripr", "--lib", "cli::help::tests::every_parsed_flag_agrees_with_its_help_documentation", "--", "--exact", "--format", "pretty", "--color", "never"]; outcome=failed; status=exit status: 101; exit_code=101; elapsed_ms=127716; elapsed_scope=runner_start_to_observed_exit_excluding_pipe_drain; cargo_progress=recognized_stderr_lines(compiling=49, checking=unavailable, fresh=unavailable)
