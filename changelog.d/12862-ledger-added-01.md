<!-- section: Added -->
- `ripr check --json`: a finding with a static limitation now carries an
  additive optional `static_limitation` object with `kind`,
  `last_established_edge`, `first_unresolved_edge`, `analyzer_route` and
  `non_claim`, taken from the same evidence lines the human output prints.
  It is absent when any detail is missing; `static_limit_kind` stays the
  discriminator (#1369).
- `ripr first-pr` now selects a repairable preview TypeScript gap from the
  gap decision ledger on a TypeScript project root without a `Cargo.toml`,
  as it already did for preview Python gaps: preflight reports a
  `typescript_project` check, `start-here` labels the gap as preview
  TypeScript with its missing discriminator and verify command, and the
  refresh route regenerates the ledger from saved check output (#1501).
- LSP: a failed, cancelled or superseded refresh no longer replaces the last
  completed analysis snapshot. Its diagnostics and inspection context are kept
  and marked stale, repair actions are withheld until a later successful
  refresh, and the server reports attempt health (`state`, `run_status`,
  `failure`, `retry_command`) through the `ripr/analysisStatus` notification
  and the `ripr.collectWorkspaceStatus` result. VS Code shows the failure,
  says the last-known-good evidence is retained, and names `ripr.refresh` to
  retry (#1596).
- LSP: finding diagnostics now carry a `fix_instruction` summary in their
  `data` next to the discriminator witness, and the finding hover adds a
  **Fix instruction** line naming its state: fix site ready, static
  limitation, stale, inspect only or unavailable (#1870, #1753).
- `ripr doctor --json` now includes a `languages` array naming the
  languages the effective configuration enables, so CI and scripts can read
  it instead of parsing the human `Enabled languages:` line (#2182, #2072).
- LSP: the `ripr/analysisStatus` and receipt status payloads now carry a
  bounded `client_features` object projecting the client capabilities
  negotiated at `initialize`, and the VS Code extension advertises its
  `riprEditor` experimental block. An absent, malformed or unknown
  `riprEditor` or `riprAgent` block reads as unsupported without breaking
  the standard session (#2268, #1987).
- `ripr check --worktree` now accepts `--write-artifact PATH`, so
  `ripr explain --from` and `ripr context --from` can reuse a worktree run.
  The base-to-working-tree diff is re-resolved at reuse time, and an
  artifact whose working tree has changed since it was written is refused
  (#2280, #2251).
- `ripr agent verify-execute --packet PATH --result-json PATH --authorize
  --json` runs the direct `ripr agent verify` route carried by a
  producer-emitted agent packet. It runs only when the packet's typed verify
  command matches its own verification commands and the route recomputed
  from provenance-validated `--before` and `--after` repo-exposure artifacts
  matches the packet. The child is the current ripr binary under a bounded
  timeout, output caps and a reduced environment; the typed JSON result goes
  to stdout and to a new `--result-json` file. It records process evidence
  only and issues no receipt (#2396, #2332).
- Library: the `ripr::config` module is now public, so library consumers
  can name and construct `RiprConfig` and read its configuration types such
  as `OraclePolicy` and `SeverityConfig`, instead of only running with the
  default configuration (#2514, #2458).
- Library API: `check_workspace_with_config`,
  `check_workspace_repo_with_config`, `check_workspace_worktree_with_config`,
  `collect_context_with_config` and `explain_finding_with_config` are now
  public, so an embedder can pass its own `RiprConfig` to check, context and
  explain instead of only the default configuration.
  `check_workspace_with_config` is re-exported at the crate root; the others
  are reachable through `ripr::app` (#2524, #2458).
- Commands that read a live Git diff now print a stderr warning when the
  repository is mid-rebase, mid-merge or mid-cherry-pick
  (`ripr: git repository is mid-<operation>; ... Results may be
  distorted.`), because the working tree may hold conflict markers and a
  rebase may leave `HEAD` at a replay commit (#2670).
- The library crate root now re-exports `explain_finding_with_input` and
  `render_check`, and the published package includes runnable library
  examples (`check_diff`, `explain`, `render_json`) that can be run with
  `cargo run --example <name>` (#2709).
- `ripr check --format agent-seam-packets-json`: `write_targeted_test`
  packets carry an advisory `suggested_test_command` with
  `suggested_test_command_status`
  `runnable_after_the_suggested_test_exists`, and an envelope with an
  actionable targeted-test packet carries a `next` block of before/after
  snapshot, `verify_after_edit` and (for one seam) `receipt_after_verify`
  commands (#2866).
- `ripr agent packet` and agent seam packets now carry
  `analysis_outcome_status` (`complete`, `incomplete`, `missing`, `invalid`
  or `not_applicable`), an optional `analysis_outcome_error`, and the
  producer's typed `analysis_outcome` envelope, so a packet consumer cannot
  read a finished seam budget as a complete diff analysis. The packet
  `schema_version` moved past `0.3` for these fields (#2897).
- VS Code: the extension adds the `ripr.includeUnchangedTests`,
  `ripr.seamDiagnostics` and `ripr.diagnosticProfile` (`actionable` or
  `full`) settings and forwards them to the language server (#2905).
