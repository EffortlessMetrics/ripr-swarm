<!-- section: Internal / CI -->
- The Windows advisory workspace test runs set `CARGO_LOG=cargo::core::compiler::fingerprint=info`, so the retained log records Cargo's per-unit fingerprint reasons when the policy `covered_by` enumeration rebuilds a unit on the shared target directory (#12668). This is diagnostic only; the reasons are evidence toward the cause, not the fix.
