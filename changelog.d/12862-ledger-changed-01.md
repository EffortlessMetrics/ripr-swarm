<!-- section: Changed -->
- Default `ripr check` human output is now a bounded triage view instead of
  a full evidence section for every finding. `--format human-full` (alias
  `text-full`) prints every finding body, and `--suppression-policy` accepts
  it. A repo-scoped format such as `repo-exposure-json` or `repo-sarif`
  combined with `--base` or `--diff` now warns on stderr that those flags do
  not bound it and names `--format json` or
  `--format repo-exposure-summary-json` (RIPR-SPEC-0122, #1489).
- Python: a test that imports the owner function through a relative import
  of the owner's own module (`from .handler import normalize` beside
  `handler.py`) now carries the owner's module identity, so its assertions
  can be credited. A relative import of a sibling module, or one that climbs
  above the package root, stays uncredited, and owner import matching now
  compares the whole dotted module path instead of the file stem, so a
  same-named module in another package is no longer taken for the owner
  (#1690).
- The workflow from `ripr init --ci github` now documents its repository
  variables inline: where to set them, the accepted `RIPR_GATE_MODE` values
  (empty for advisory, `visible-only`, `acknowledgeable`, `baseline-check`,
  `calibrated-gate`) and the `RIPR_COMMENT_MODE` values (`off`, `plan`,
  `inline`) (#2055, #2010).
- LSP: when an optional analysis component fails during a refresh (the
  seam inventory, the gap decision ledger or the causal projection), the
  server now records a typed component outcome on the snapshot, reports the
  run as `limited` instead of `full`, lists the outcomes in a new
  `components` array on `ripr/analysisStatus`, and sends one
  `window/logMessage` warning per distinct failure naming the component and
  its recovery route. Before, these failures were written only to the
  server's stderr. Diff-scoped findings are still published (#2256, #1997).
- `ripr --help` now prints a one-screen, task-led overview (setup, the gap,
  fix and verify loop, and one route per task) instead of the full command
  catalog. `ripr help --all` and `ripr --help --all` print the full
  reference, which now also lists `pr-summary`, `annotations`,
  `pr-evidence` and `impacted-evidence`. `ripr help <command>` is unchanged
  (#2789).
- Badge JSON schema `0.8` adds nullable `analysis_complete` and a typed
  `analysis_outcome` to diff-scoped badges. A diff-scoped badge with zero
  findings from an incomplete analysis now reads warn/yellow and names the
  outcome kind instead of pass/green; repo-scoped badges emit both fields as
  `null`. Diff-scoped SARIF runs carry run-level `properties` `run_status`
  (`complete` or `incomplete`), `analysis_complete` and `analysis_outcome`,
  so an empty `results` array no longer reads as a clean analysis (#2880).
- The workflow from `ripr init --ci github` now saves `ripr check --format
  json` output to `target/ripr/pr/check.json` and passes it to
  `ripr review-comments` with `--check-output` (inside its
  `ripr reports ci-packet` step). When that check does not finish, the step
  says so and review-comments fails closed on the named artifact instead
  of recomputing its own result (#2884).
- `ripr agent review-summary` now reads the producer's typed analysis
  outcome (`target/ripr/workflow/analysis-outcome.json`, written by the
  generated agent workflow), reports it as an `analysis_outcome` JSON field
  and an `Analysis outcome:` Markdown line, and reads `ready` only when that
  outcome is present and complete. A missing or incomplete outcome makes the
  summary `incomplete` (#2890).
- Agent seam packets built from gap records now carry `command_specs` only
  when the gap record itself holds typed command specs, and a spec that
  fails validation or has the wrong verify/receipt role is refused. The
  packet renderer no longer rebuilds typed routes by parsing the display
  command strings (#2892).
- `ripr agent receipt` JSON now carries the producer's typed analysis
  outcome from `target/ripr/workflow/analysis-outcome.json` as
  `analysis_outcome_status` (`complete`, `incomplete`, `missing` or
  `invalid`), `analysis_outcome_error` and `analysis_outcome`. The receipt
  `status` is `advisory` only when that outcome is complete; it reads
  `incomplete` when the outcome is incomplete or missing and `invalid` when
  the artifact cannot be validated, instead of always `advisory` (#2895).
- Assertion guidance is now typed: review-comment cards carry an
  `assertion_guidance` object and agent packets' `assertion_shape` carries
  `state` (`concrete`, `requires_observer_setup`, `fix_site_only`,
  `verification_only`, `unresolved` or `stale`), `basis`,
  `observer_kind`, `reason` and `recovery` beside `kind` and `example`.
  Only a `concrete` state carries an example; the other states carry
  `example: null` with a reason (and, for `unresolved` or `stale`, a
  recovery) instead of a fill-in assertion (#2908, #2658).
- `ripr agent verify` and `ripr agent receipt` now refuse a before/after
  artifact pair whose producer version, analysis mode or analysis profile
  differ, or whose snapshot identity does not track the repository head
  (identical across distinct heads, or different for the same head), and
  the refusal names which field differs; the receipt refusal is
  `incomparable_analysis_inputs` (#2915).
- `repo-exposure-json` artifacts now record `snapshot_identity` as
  `snapshot:input:<fingerprint>;revision:<head sha>`, and `ripr agent
  verify` and the other artifact consumers refuse an artifact whose snapshot
  identity does not match its declared input identity and repository head,
  so snapshots of the same inputs at different commits no longer share an
  identity (#2920, #3001).
- `ripr agent verify` JSON moves to `schema_version` `0.3`: the pair-level
  `artifact_currentness` now names each side
  (`historical_before_current_after`, `current_before_historical_after`,
  `dirty_before`, `dirty_after`, `dirty_both`) instead of labeling every
  mixed pair, including a clean historical before and current after, as
  `dirty_worktree`. `ripr agent receipt` refuses a `0.2` verify document
  (#3053, #3027).
- `ripr first-action` JSON for a selected gap now carries the gap ledger's
  own typed verify `command_specs` entry when the ledger record is
  `candidate_current` and its display matches the selected verify command.
  Otherwise the typed verify spec is omitted, and a missing, ambiguous or
  mismatched ledger record adds a bounded warning instead of a typed route
  (#3510).
