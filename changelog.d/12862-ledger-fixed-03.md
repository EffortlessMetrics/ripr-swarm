<!-- section: Fixed -->
- The published `schemas/ripr/check.schema.json` now admits
  `findings[].source_currentness` and the top-level `test_harnesses` object
  that `ripr check --format json` emits. Before, check JSON with any finding
  failed validation against the published schema (#3894).
- `ripr pilot`'s terminal screen now leads the picked gap line with its
  seam id and, for a repair-eligible gap, prints a paste-ready
  `ripr agent repair --root ... --seam-id ... --phase before` command.
  Before, the id `ripr agent repair` needs appeared only in
  `pilot-summary.md` and `agent-seam-packets.json` (#3898, #3896).
- `ripr first-pr --root .` no longer prints paths with an interior `/./`
  (`/repo/./target/ripr/reports/start-here.md`) in its `Start here:`,
  `Artifacts:` and `Wrote` lines. A leading `./` and any `..` are kept
  (#3910).
- `ripr help --all` now matches what the parsers accept: it names the
  default outputs `reports ts-limitations` and `reports ts-false-actionable`
  write, all four `gate evaluate --mode` values, the `--repo-exposure` and
  `--check-output` inputs of `reports gap-ledger` and the gap-ledger
  selectors of `agent packet` and `rerun`, shows `--stdio` as optional for
  `ripr mcp`, and lists `agent repair --phase verify` only with its
  `--verify-authorized` and `--verify-authority` flags (#3914, #3915).
- `schemas/ripr/gate-decision.schema.json` now defines the optional
  `baseline_match_kind` field that `ripr gate evaluate` writes on a decision
  matched only through a legacy fallback baseline entry. Before, such a gate
  decision failed validation against the published schema (#3916, #3912).
- `ripr doctor` now gives one first command. Its recommended first command
  is `ripr check` with no hard-coded `--base origin/main`, so the advice
  works in a repository whose default branch is not `main`, and with no
  start-here packet the safe next action is that same command instead of
  `ripr first-pr`, which only composes a packet from existing evidence.
  Once a packet exists, the safe next action opens it and names the
  `ripr first-pr` refresh (#3946).
- TypeScript and JavaScript preview: a changed predicate (for example
  `total > 50` to `total >= 50`) no longer reads `exposed` just because a
  strong family-matching assertion exists. It reads `exposed` only when such
  an assertion calls the owner at the changed boundary (for example an
  argument carries the literal operand); otherwise it reads `weakly_exposed`
  and names the boundary as the missing check. Blank, comment-only and
  punctuation-only added lines (`}`, `});`) no longer produce probes
  (#3953).
- Python: a changed comparison (`qty > item.on_hand` to
  `qty >= item.on_hand`) reads `exposed` only when a strong related test
  calls the owner with arguments that make both sides of the comparison
  equal. A test that calls it only away from the boundary now reads
  `weakly_exposed`, and for simple operands the finding names the boundary
  as the missing discriminator with the observed values. When no strong
  call passes a literal argument the oracle reading stands and an `exposed`
  finding carries a `boundary_activation_unresolved` limitation (#3955).
- `ripr pilot` now writes `repo-exposure.json` with the same producer
  `artifact` identity that `ripr check --format repo-exposure-json` writes,
  so it can serve as the `--before` snapshot of `ripr agent verify`. When the
  pilot seam budget truncated the seams, the identity is left out and verify
  refuses the file instead of comparing two different seam populations
  (#3956).
- `ripr agent repair`: running `cargo test` or `cargo build` between the
  before and after phases of a Rust repair no longer makes the edit cage
  report a violation for Cargo output under `target/`, and an unchanged,
  untracked file present before the attempt (such as `Cargo.lock`) no
  longer makes the receipt refuse as outside the trusted edit surface. A
  refused after phase names each refused path and the recovery route
  (#3957).
- `ripr help --all` now shows the artifact inputs of `ripr first-action`
  and `ripr pr-review front-panel` as a required one-of group, and each
  command's `--help` says at least one input is needed. The help used to
  list every input as optional with a path beside it, although the command
  exits 2 when given none (#3970, #3969).
- TypeScript: a same-stem test file, a `describe` title or a test title no
  longer relates a test to an owner the test never references. An owner
  whose only links were such names now reads `no_static_path` instead of
  `weakly_exposed` with related tests that call only a sibling function. A
  test that references the owner without a recognized call shape still
  relates as `weakly_exposed` (`ambiguous_related_test`) (#3977).
- `ripr agent status` now reports a finished repair attempt's outcome, not
  only its state. An attempt reads `finished` only when the receipt issued
  for it is `advisory` with movement `improved`, `gap_open` when the receipt
  shows the gap still open (the seam is offered a new attempt), and
  `unconfirmed` otherwise, with a `repair_receipt_unconfirmed` warning. Each
  attempt row carries a `receipt` object and `last_after_refusal`, the
  repair-attempt manifest records the most recent refused after phase, and
  an attempt whose test was committed on top of its prepared head stays
  resumable (#3982).
- `ripr agent repair --phase after` no longer refuses an attempt because
  the first `cargo test` created an untracked `Cargo.lock`; only a
  Git-tracked lockfile counts as an analysis input. Committing the focused
  test before the after phase is now accepted when `HEAD` only moved
  forward from the prepared head, and the receipt records `before_head` and
  `after_head`. A rewritten history, or a changed tracked lockfile,
  manifest or `ripr.toml`, refuses before the attempt finishes and names
  the restore route. `ripr agent receipt` can be rerun after the after
  phase (#3986).
- `ripr agent receipt --out PATH` with a relative path now writes under the
  resolved `--root`, so a printed receipt command pasted from another
  directory writes inside the selected workspace instead of the current
  directory; an absolute `--out` is unchanged (#3993, #3967).
