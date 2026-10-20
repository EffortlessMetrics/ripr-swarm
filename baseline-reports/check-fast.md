# check-fast report

Status: pass

Selector: passed
Base: origin/main

Ran:
- fmt --check
- check-static-language
- check-command-catalog
- check-generated
- check-generated-clean
- check-lint-policy

Skipped:
- check-support-tiers (not applicable)
- cli-help-parity
- check-no-panic-family
- check-allow-attributes
- check-file-policy
- clippy
- check-fixture-contracts

## Child command diagnostics

Diagnostic observations of existing child calls in this invocation only. Missing rows/values are unavailable evidence, never zero compilation or completed work. Elapsed scopes differ; the captured runner polls exit and excludes pipe draining; enforced timeout duration includes termination/reaping and Unix process-group confirmation where applicable. Cargo progress counts recognize stderr lines only, not compiler-unit identities, completed artifacts, reuse proof or redundant compilation. These diagnostics waive no gate. Capture scope: fmt and workspace Clippy inherit streams; selected CLI help parity captures streams and reports recognized Cargo stderr progress.

- program="cargo"; argv=["fmt", "--check"]; outcome=pass; status=exit status: 0; exit_code=0; elapsed_ms=21005; elapsed_scope=inherited_status_call_including_launch; cargo_progress=unavailable
