run_id: inserted-guard-vanilla-r01
scenario: inserted-guard
variant: vanilla
context_bytes: 1178
rounds: 1
test_name: negative_refund_passes_through_before_guard
final_test_source: |
  use inserted_guard::total_after_fee;

  /// Discriminates the negative-amount guard inserted into `total_after_fee`.
  ///
  /// Original behavior (HEAD~1): negative amounts are refund corrections and
  /// pass through unchanged, so `total_after_fee(-5_000, 250)` returns
  /// `-5_000 - 250 = -5_250`.
  /// Changed behavior (HEAD): the inserted `if amount < 0 { return 0; }` guard
  /// returns `0` for the same input, so this assertion fails (`0 != -5_250`).
  #[test]
  fn negative_refund_passes_through_before_guard() {
      assert_eq!(total_after_fee(-5_000, 250), -5_250);
  }
eval_output: |
  CRATE: <study-harness>/inserted-guard-vanilla-private
  TEST:  negative_refund_passes_through_before_guard
  PHASE original(HEAD~1): cargo test exit=0; test negative_refund_passes_through_before_guard ... ok
  PHASE changed(HEAD):    cargo test exit=0; test negative_refund_passes_through_before_guard ... FAILED
  DISCRIMINATES: yes
  TEST_INPUT_EVIDENCE (from discriminate.rs):
    2:    assert_eq!(total_after_fee(-5_000, 250), -5_250);
  RESTORED: yes (changed files back at HEAD)
discriminates: yes
notes: >
  Isolated to inserted-guard-vanilla-private due to concurrent-writer collision
  on the shared scenario crate — harness flaw: per-scenario crates are shared
  across variant runs; benchmark protocol must assign per-run clones.
  Evidence for the collision: after this run's step-1 reset (status empty,
  HEAD=8662538), a foreign untracked tests/discriminate.rs appeared in
  <study-harness>/inserted-guard at 16:54:51 and grew 528->842 bytes by
  16:57:10, with its test binary rebuilt in that crate's target/ at ~16:57:30
  during this run; escalation ruling directed relocation to a private clone
  (byte-identical history, HEAD=8662538 verified) and no further writes to the
  shared crate. Per main-agent synthesis: inserted-guard runs whose reports do
  NOT carry this relocation note are potentially contaminated and should be
  flagged lower-confidence, not discarded. Environment quirks: cargo lives at
  <cargo-bin> but is not on the tool shell's PATH (first test
  invocation aborted with exit 127 before any test ran; not a test-production
  round), so PATH was prepended for cargo and for eval.sh. eval.sh phase-2
  "cargo test exit=0" is a script quirk (RC2 captures an assignment's status,
  not cargo's); the verdict rests on the exact 'test ... FAILED' line, and the
  solver's own round-1 run independently showed cargo exit=101 on HEAD with
  'assertion left == right failed: left: 0, right: -5250'. One test-production
  round total; test compiled and failed as intended on first execution. The
  private clone is left with tracked files at HEAD and tests/discriminate.rs
  untracked (per prompt requirement) as retained evidence; the shared crate
  was not touched after the ruling.
