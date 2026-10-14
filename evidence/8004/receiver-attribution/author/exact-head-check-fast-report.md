# check-fast report

Status: failed

Original failure:
```text
cargo clippy --workspace --all-targets -- -D warnings failed with exit status: 101
```

## Child command diagnostics

Diagnostic observations of existing child calls in this invocation only. Missing rows/values are unavailable evidence, never zero compilation or completed work. Elapsed scopes differ; the captured runner polls exit and excludes pipe draining; enforced timeout duration includes termination/reaping and Unix process-group confirmation where applicable. Cargo progress counts recognize stderr lines only, not compiler-unit identities, completed artifacts, reuse proof or redundant compilation. These diagnostics waive no gate. Capture scope: fmt and workspace Clippy only; streams remain inherited, so Cargo progress is unavailable.

- program="cargo"; argv=["fmt", "--check"]; outcome=pass; status=exit status: 0; elapsed_ms=13972; elapsed_scope=inherited_status_call_including_launch; cargo_progress=unavailable
- program="cargo"; argv=["clippy", "--workspace", "--all-targets", "--", "-D", "warnings"]; outcome=failed; status=exit status: 101; elapsed_ms=188448; elapsed_scope=inherited_status_call_including_launch; cargo_progress=unavailable
