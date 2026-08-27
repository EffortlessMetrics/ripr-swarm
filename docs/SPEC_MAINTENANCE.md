# Specification maintenance inventory

`cargo xtask specs maintenance --as-of YYYY-MM-DD` writes an advisory inventory
to `target/ripr/reports/spec-maintenance.json` and
`target/ripr/reports/spec-maintenance.md`. Add `--json` to also print the same
versioned JSON DTO to standard output.

The report covers every discoverable `docs/specs/RIPR-SPEC-NNNN.md` file. Other
Markdown files below `docs/specs/` are listed as omitted, with the reason they
are not part of the discoverable denominator. Required-spec read or UTF-8
errors are instrument failures and return a nonzero status.

Each row includes the spec ID and path, a SHA-256 content digest, observed
document status, objective reason codes, evidence references, a bounded next
route, and limitations. JSON and Markdown are rendered from the same
`SpecMaintenanceReportV1` value, and stable sorting makes fixed repository
bytes plus a fixed `--as-of` value reproducible.

Git history is optional. When it is unavailable, the report says so and keeps
repository-only findings. Age is an observation or ordering hint only; it
never changes spec validity, lifecycle, support posture, branch protection, or
merge eligibility. The report does not create review receipts, publish
digests, alter workflows, or infer that implementation or evidence exists.
