<!-- section: Internal / CI -->
- The nightly Windows advisory lane now runs the real policy facade in isolation and admits `target/ripr/reports/file-policy.md` through `cargo xtask windows-advisory-file-policy-evidence`, binding the retained report to the checkout head, facade log/status, subject sidecar, and existing packet oracle instead of relying on artifact upload alone (#5044).
