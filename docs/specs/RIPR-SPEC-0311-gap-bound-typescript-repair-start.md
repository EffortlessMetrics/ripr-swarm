# RIPR-SPEC-0311: A complete TypeScript or Python packet offers a gap-bound repair start

Status: proposed

Owner: product / analysis

Created: 2026-10-22

Linked proposal:

- None yet

Linked ADRs:

- [ADR 0020](../adr/0020-repair-artifacts-carry-producer-identity.md)

Linked plan:

- None yet

Linked issues:

- #9103
- #10373 (Python gap-bound start, item 9)
- #10698 (commit step before a start a dirty tree would refuse, item 10)

Linked PRs:

- None yet

Support-tier impact:

- No tier change. TypeScript and Python stay `preview_advisory_only`.
- Claim boundaries remain governed by
  [support tiers](../status/SUPPORT_TIERS.md).

Policy impact:

- Register this spec in `.ripr/traceability.toml`.
- Additive `agent repair --gap-id/--base` flags and one additive, optional
  repair-attempt manifest member (`gap_binding`).

## Problem

A TypeScript finding with a complete repair packet had no start the repair
transaction accepted: `ripr check` printed "repair packet is not ready" and
`agent repair` required a Rust seam id. The verifier half (RIPR-SPEC-0310)
existed with no way to begin the attempt.

## Behavior

1. Gap-Bound Repair Start: for a TypeScript finding that is not static-limited,
   whose gap record passes the shared `validate_agent_gap_record_packet`, and
   when a base is known, `ripr check` prints
   `ripr agent repair --gap-id G --base B --phase before`.
2. `agent repair --gap-id G --base B --phase before` re-runs the diff-scoped
   check against `B`, looks the gap up, and runs the same shared validator.
   An unknown gap or a gap whose packet is not ready is refused ("the repair
   packet is not ready" or "names no current TypeScript gap") with no attempt
   and no workflow artifacts. The printed start line replays only `--base`
   and `--root`; a static-limited finding is refused here too, as `ripr check`
   would not offer it.
3. Flags are exclusive: `--gap-id` excludes `--seam-id`, requires `--base` and
   `--phase before`, accepts only a canonical TypeScript preview gap id or a
   canonical Python gap id (item 9), refuses other non-Rust gaps with their
   existing terminal reason, and refuses the Python repair-trust flags: a
   gap-bound start is the unbound flow for both languages, and the governed
   Python repair-trust binding (RIPR-SPEC-0176) stays seam-scoped.
4. Acceptance writes `before.check-findings.json` and `gap-repair-packet.json`
   (packet `seam_id` is the gap id) and a manifest with `seam_id` = gap id and
   `gap_binding.base`. The edit cage is derived from the packet's test
   file and is enforced by the gap-bound after phase (item 6). The
   persisted `next_command` is the ordinary `--attempt <id> --phase after`
   command. `gap_binding` records the `--base` spelling and `base_commit`, the
   full commit it resolved to; the before snapshot is produced against that
   commit. A start
   refused (including a dirty tracked non-test file, or a publication failure
   after the workflow artifacts were written) leaves no attempt manifest and
   restores the shared workflow artifacts to the bytes they had before the
   invocation: prior files survive byte-identical and only files the
   invocation created are removed.
5. Status and recovery text for a gap attempt name the gap start and never
   `--seam-id`.
6. `agent repair --phase after` for a gap-bound attempt writes
   `after.check-findings.json` through the same producer as the before
   snapshot, analyzed against `base_commit` (a relative `--base` such as
   `HEAD~1` names a different commit once the test is committed), and calls the
   shared verifier with the gap id. Closure is positive evidence only: the gap
   must be open before and `exposed` after; a gap that is absent from the after
   artifact, was never open, or still reads open is a typed refusal (exit 3,
   stdout `repair_after_refusal`) recorded against the attempt's after-phase
   retry budget, and the attempt stays `awaiting_edit`. A spent budget stops
   the phase with a restart that names `--gap-id`/`--base`. Only a closed gap
   reaches the edit cage, which is enforced: a violation or an incomparable
   cage ends the attempt terminally with the `repair_after_failure` envelope
   and no closure is reported. A compliant finish prints the
   `repair_after_result` envelope whose `verify.bound_gap` carries the closure.
   A compliant finish issues the gap-bound receipt (item 8). The shared
   `validate_agent_gap_record_packet` stays the only actionability authority.
7. Let-bound, imported and reach-barrier variants remain unrouted. Seam
   attempt manifests omit `gap_binding`, so their bytes and before commitment
   are unchanged.
8. Gap-Bound Receipt (C2c): a compliant gap-bound finish writes the ordinary
   `target/ripr/reports/agent-receipt.json` for the gap (`seam.seam_id` and
   `provenance.seam_id` are the gap id) from the verify document that closed
   it. The receipt carries one additive member, `bound_gap` (`gap_id`,
   `before_classification`, `after_classification`, `file`, `line`,
   `closed: true`), copied only from a verify document whose bound gap is the
   receipt's seam, and `repair_attempt.gap_binding` (`base`, `base_commit`).
   Seam receipts omit both members, so their bytes are unchanged. Issuance is
   fail-closed: a receipt for a gap refuses unless the verify document carries
   a closed `bound_gap` for that gap, and receipt validation recomputes the
   closure from the two snapshots, so an unbound pair, a `bound_gap` naming a
   gap that did not close, or an edited closure is refused
   (`[gap_binding]`/`[not_canonical]`) and no receipt is written. A refused,
   absent or cage-refused finish issues no receipt. The receipt reads the
   after snapshot as its analysis outcome (the same complete, diff-scoped
   producer document against the attempt's own base). The attempt retains the
   receipt and verify document as terminal evidence, and the stored-receipt
   reader refuses a gap attempt's receipt whose verify document lacks the
   closed `bound_gap` or whose `bound_gap` differs from it. `agent status`
   then reads the retained receipt as showing the gap closed (advisory plus
   improved static grip) and offers no `--seam-id` command. The receipt is
   static exposure evidence: it runs no test, reports no runtime mutation
   outcome and makes no sufficiency claim (`verification_not_run`,
   `runtime_mutation_execution: false`).
9. Python Gap-Bound Start (#10373): a Python finding that is not
   static-limited offers the same start when the gap record the gap ledger
   projects from that run's check JSON (`check_output_gap_records`, the
   projection `ripr reports gap-ledger --check-output` uses) is the single
   record for its canonical gap and passes `validate_agent_gap_record_packet`.
   `ripr check` prints `ripr agent repair --gap-id gap:python:... --base B
   --phase before`; the before phase re-runs the check, renders its check
   JSON and resolves the gap through the same lookup, so the printed start
   and the accepted start consult one admission. A forged or vanished Python
   gap is refused ("names no current Python gap") with no attempt. The after,
   verify and receipt phases are the language-neutral ones above. `ripr
   repair` and `agent repair --seam-id` given a Python gap still state the
   Rust-only seam reason and point at the gap-bound start; `agent status`
   after a pilot names the start check prints.
10. Commit Step Before A Dirty Start (#10698): the start refuses to begin
    while tracked files outside the packet's allowed edit surface differ from
    HEAD. When `ripr check` analyzed such a tree, it prints
    `git -C ROOT --literal-pathspecs commit -m "<message>" -- FILES` before
    the start, naming exactly the files the start's own premise check refuses,
    shell-quoted and spelled relative to ROOT, so the command works from any
    working directory and treats a name like `src/[ab].py` literally. The
    pilot-routing warnings in `agent status` name the same step. On a clean
    tree the start prints bare. When the packet, policy, HEAD or Git cannot
    establish the prerequisite, the start is withheld with a reason. A dirty
    worktree start binds the diff origin captured by the loader, so committing
    cannot move a symbolic base. A clean committed-history check can use its
    requested base; a committed-only check with dirty files outside the
    allowed edit surface withholds the start and asks for a fresh check after
    those files are committed or restored.
11. Preview Repair Guidance And Multi-Comparison Refusal (#13790): for a
    TypeScript/Python root, `ripr repair` and `ripr continue` say in short
    lines that the `ripr repair` seam flow is Rust only, that a preview gap
    starts with the `ripr agent repair --gap-id ...` command `ripr check`
    prints when one is ready, and otherwise how to add the test by hand. The
    root is spelled relative to the working directory (`.` for the directory
    itself, a relative child path, the absolute path when outside). When the
    shared validator has already refused a Python gap whose record is the
    no-repair-card decline and whose Missing check names a predicate holding
    several relational comparisons, the refusal appends that reason and one
    by-hand row per counted boundary (`x == N`). Rows are named only when every
    term is a plain relational `name OP literal` (or chain) and their number
    equals the producer's count; otherwise the shape alone is printed. This is
    wording only: no start command is built and readiness stays the shared
    validator's.
12. Preview Route On Seam Inspection Surfaces (#13791): when the selected
    scope's changed code is TypeScript or Python preview only (every changed
    file the shared language router routes is an enabled TypeScript or Python
    file), an empty `ripr agent brief` replaces its "under the current config"
    warning with one line that names the language, `ripr check --format
    agent` and the conditional `agent repair --gap-id` start, and says no
    config change adds a seam. `agent card --seam-id` and `agent packet
    --seam-id` given an id shaped like a TypeScript or Python preview finding
    or gap id say it is shaped like that id family, not a seam id, and name
    the same route; the packet refusal's derived remedy command and the
    card's typed `remedy_route` are that `ripr check --format agent` command.
    A `--diff` brief's route replays `--diff <path>`; a `--files` brief says
    the current diff need not match its selection. Classification reads every
    changed path, including files with only removed lines, and stays generic
    when the diff deletes a whole file. Exit codes (3) and the
    `seam_id_unknown` status are unchanged, a truly unknown or Rust-shaped
    stale id keeps its prior wording, and the start stays conditional on
    `ripr check` printing it, so no readiness claim is added.
13. Disabled-Python Disclosure On An Empty Brief (#13571): when the selected
    scope's changed code is Python only (every changed file the shared
    language router routes is a Python file) and Python is not enabled —
    the flat-layout repo with no root marker and no `ripr.toml` — an empty
    `ripr agent brief` replaces its "under the current config" warning with
    one line that names Python, says Python is not enabled in
    `ripr.toml [languages]` and ripr did not analyze the change, gives the
    enabling step (add `"python"` to `[languages] enabled` in `ripr.toml`),
    and only then names the `ripr check --format agent` route for the
    brief's own input. For a `--files` brief that route reads the current
    diff, so the line qualifies it as the current diff's findings (that
    diff need not match the `--files` selection) rather than presenting
    it as analyzing the selected files. It never says no config change
    adds a seam, because here a config change is exactly the fix.
    A Python-enabled scope keeps
    item 12's preview wording, a Rust-only or mixed-language scope keeps
    the generic wording, and no verdict, exit code, or diff-driven
    auto-enable behavior changes.

## Required Evidence

- Parser tests: exclusivity, bad prefix, Python accepted with `--base`,
  other non-Rust gaps rejected.
- Edit-cage tests with a TypeScript gap packet (test file accepted,
  production and non-test files refused).
- Manifest round-trip with and without `gap_binding`.
- Status and recovery text for a gap attempt.
- Installed journey: the start line is printed, the before phase is accepted,
  the manifest and snapshot are recorded, the eight variants stay unrouted
  with negative start assertions, and Python assertions are unchanged.
- Installed after-phase journeys: positive closure, not-closed refusal with
  retry, absent gap is not closure, cage violation, retry budget.
- Gap-bound receipt: the receipt names the closed gap, the attempt and the
  resolved base commit and is retained and read by status; an unbound pair, a
  forged `bound_gap` and an edited closure are refused with no receipt written;
  a not-closed, absent or cage-refused finish writes no receipt; the closure
  rule and receipt rendering have unit negatives.
- Python installed journey (#10373): check prints the start; seam routes
  point at it; a forged gap is refused with no attempt; the before phase
  replays to exactly one attempt; a committed boundary test closes the gap
  (`weakly_exposed -> exposed`) and writes the receipt; status after a pilot
  names the same start. A refused variant (tests removed, so the finding is
  static-limited) is offered no start, its `--gap-id` start is refused as not
  ready, and no attempt or workflow artifact is left (#10432). Unit negatives
  cover the subject predicate, every fail-closed lookup branch and a record
  the shared validator refuses.
- Preview guidance and multi-comparison refusal (#13790): the Python/TypeScript
  `ripr repair` text with a relative root and no "Rust gaps only" claim, a Rust
  root unchanged; the multi-comparison refusal names the reason and rows while
  `ripr check` prints no start for that gap and keeps the single-boundary
  gap's start; unit negatives for a non-no-card decline, a computed operand, a
  mixed equality term and a row count that disagrees with the producer.
- Preview route on seam inspection surfaces (#13791): CLI cases for a
  TypeScript and a Python preview scope (brief, card, packet) with negative
  controls for a truly unknown seam id and a Rust-shaped stale finding id;
  unit cases for the preview-only scope predicate (Rust, mixed, other
  language and disabled adapter scopes stay generic).
- Disabled-Python disclosure on an empty brief (#13571): a CLI case for a
  flat-layout Python scope (`lib/*.py` plus `t/test_*.py`, no root marker,
  no `ripr.toml`) naming Python, the not-analyzed disclosure and the
  enabling `[languages]` step before the `check` route, plus a `--files`
  case asserting the current-diff qualifier on that route, with negative
  controls for the same layout with Python enabled (item 12 wording) and
  for a Rust-only scope (generic wording); unit cases for the
  disabled-Python scope predicate (enabled Python, Rust, TypeScript, Perl,
  JavaScript, unrouted-only and empty scopes stay empty).

## Non-Goals

- Verification-run execution for gap attempts (a later slice); the receipt
  reports `verification_not_run`.
- Runtime mutation outcomes or sufficiency claims; results are static exposure
  evidence only.

## Acceptance Examples

1. On the installed TypeScript head fixture, `ripr check` prints
   `ripr agent repair --gap-id <gap> --base <base> --phase before`; running
   it is accepted, awaits the focused test edit, and records a manifest whose
   `seam_id` is the gap and whose `gap_binding.base` is the base.
2. The same start for a let-bound, imported or reach-barrier variant is
   refused with `the repair packet is not ready` and leaves no attempt.
3. `agent status` for the gap attempt never names `--seam-id`.
4. After a committed test that pins the equality boundary, the persisted
   `--phase after` command exits 0, prints `verify.bound_gap` with
   `closed: true`, and leaves the attempt `ready_to_finish`.
5. A committed test that leaves the boundary unpinned exits 3 with
   `agent verify gap binding refused` and the attempt stays `awaiting_edit`;
   reverting the production change (the gap vanishes) exits 3 with `absence is
   not closure evidence`; a closing test committed beside `src/extra.ts` ends
   the attempt terminally; the sixth refusal stops on the retry budget.
6. After the closing finish, `target/ripr/reports/agent-receipt.json` has
   `status: advisory`, `bound_gap.closed: true` for the gap, and the attempt id
   and `gap_binding`; `agent status --json` reports it `issued_for_attempt` and
   `shows_gap_closed`. `ripr agent receipt --seam-id <gap>` over a verify
   document produced without `--gap-id` is refused with "requires an agent
   verify document bound to that gap".

## Test Mapping

- `crates/ripr/tests/installed_python_typescript_journeys.rs`:
  `dirty_python_repair_start_keeps_original_diff_after_commit`,
  `dirty_typescript_start_uses_diverged_merge_base`,
  `dirty_python_check_withholds_start_when_git_inspection_fails`,
  `committed_python_dirty_file_requires_fresh_check`,
  `typescript_literal_threshold_packet_is_complete_and_controls_fail_closed`,
  `gap_bound_after_phase_closes_the_gap_on_positive_evidence`,
  `gap_bound_after_phase_refuses_a_gap_that_is_not_closed_and_stays_retryable`,
  `gap_bound_after_phase_does_not_treat_an_absent_gap_as_closed`,
  `gap_bound_after_phase_enforces_the_edit_cage`,
  `gap_bound_after_phase_stops_when_the_retry_budget_is_spent`,
  `gap_bound_after_phase_human_output_names_closure_and_replaces_a_stale_receipt`,
  `gap_bound_receipt_requires_the_verify_closure_and_rejects_forgery`,
  `typescript_status_warning_reports_unconfirmed_start_for_refused_variants`,
  `python_flat_layout_journey_runs_the_gap_bound_loop_from_the_printed_start`,
  `python_status_after_pilot_names_the_gap_bound_start`,
  `python_refused_variant_is_offered_no_gap_bound_start`.
- `crates/ripr/src/app/navigation.rs`:
  `commit_prerequisite_commits_literal_paths_from_a_decoy_directory`;
  `crates/ripr/src/app/task_first.rs`:
  `dirty_paths_outside_surface_mirror_the_start_premise_check`,
  `commit_hint_paths_are_relative_to_the_analyzed_root`;
  `crates/ripr/src/app/agent_status.rs`:
  `pilot_routing_messages_name_the_commit_before_a_start_on_a_dirty_tree`;
  `crates/ripr/src/output/human.rs`:
  `gap_bound_start_names_the_commit_first_on_a_dirty_tree`.
- `crates/ripr/src/app/gap_bound_repair.rs` unit tests
  `python_gap_subjects_need_a_printable_tail` and
  `a_python_lookup_fails_closed_without_one_validated_record`,
  `a_python_record_the_shared_validator_refuses_is_not_a_start`,
  `a_python_start_sees_every_finding_that_shares_its_gap`.
- `crates/ripr/src/cli/agent.rs`, `crates/ripr/src/app/repair_attempt/mod.rs`,
  `crates/ripr/src/app/agent_status.rs` and
  `crates/ripr/src/output/agent_seam_packets.rs` unit tests named in the
  traceability entry.
- #13571 (item 13): `crates/ripr/src/app/task_first.rs`
  (`disabled_python_scope_languages_requires_all_routed_python_and_disabled`)
  and `crates/ripr/tests/cli_smoke/agent_seam_preview_route.rs`
  (`python_disabled_flat_layout_brief_names_the_enable_step`), listed in
  `.ripr/traceability.d/13571-agent-brief-disabled-python.toml`.
- #13790 (item 11): `crates/ripr/tests/task_first_facade.rs`
  (`repair_with_changed_non_rust_code_says_rust_only`,
  `agent_repair_refusal_names_a_multi_comparison_predicate`,
  `agent_repair_refusal_for_an_unknown_python_gap_keeps_its_own_reason`) and
  unit tests in `app/gap_bound_repair.rs`, `output/human/triage.rs`,
  `output/gap_decision_ledger.rs` and `cli/commands/task_first.rs`, listed in
  `.ripr/traceability.d/13790-repair-preview-wording.toml`.

## Implementation Mapping

- `crates/ripr/src/app/gap_bound_repair.rs`: lookup, start spelling, manual
  verify route, and `default_check_gap_start`, the admission `ripr agent
  status` consults so its `pilot_routed_to_check_no_repair_target` warning
  names the start check prints, or reports that it could not confirm a start
  and directs the user to check the current route (#10001).
- `crates/ripr/src/cli/commands/agent.rs`: `run_gap_bound_before_phase`.
- `crates/ripr/src/app/repair_attempt/mod.rs`: `RepairAttemptGapBinding`,
  `begin_gap_bound_repair_attempt_with_identity`,
  `validate_gap_bound_receipt_closure`, `attempt_gap_binding_from`.
- `crates/ripr/src/app/agent_receipt.rs`: the canonical verify render
  recomputes a `bound_gap` closure from the snapshots;
  `crates/ripr/src/output/agent_receipt.rs`: `bound_gap_for_seam`.
- `crates/ripr/src/app/task_first.rs`, `app/navigation.rs`,
  `output/human/triage.rs`: the printed start line.

## Metrics

- `unit_test_pass_rate`: the tests above are required Rust tests.
