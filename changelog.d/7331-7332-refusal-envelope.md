<!-- section: Fixed -->
- `ripr diff --json`, `ripr rerun --json`, `ripr context`, `ripr agent brief
  --json`, and `ripr agent packet --json` now emit the shared typed refusal
  envelope on stdout for post-argv-parse failures instead of exit 2 with empty
  stdout, keyed with each command's own `schema_version` and carrying the new
  `check_artifact_unusable`, `gap_ledger_unusable`, and `seam_id_unknown`
  identities where they apply. The exit stays 2, the human diagnostic stays on
  stderr, and argv usage errors stay prose-only (#7331).
- `ripr check --format sarif|github|badge-json|repo-exposure-json|
  agent-seam-packets-json` now emits a per-format refusal document on stdout
  for post-argv-parse failures (a schema-valid SARIF run with a
  tool-configuration notification, one GitHub error annotation, the badge
  degraded-state convention, or the format's refused envelope with zeroed
  arrays) instead of zero bytes. Machine formats without an honest refusal
  shape yet stay prose-only as a named limitation (#7332).
