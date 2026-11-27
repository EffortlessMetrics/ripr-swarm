run_id: inserted-guard-ripr-tool-r01
scenario: inserted-guard
variant: ripr-tool
context_bytes: 5004
rounds: 1
test_name: negative_amount_passes_through_unchanged
final_test_source: |
  //! Discriminating test for the inserted-guard change (HEAD~1 -> HEAD).
  //!
  //! Original behavior: `total_after_fee` computes `amount - fee_cents` for
  //! every input; negative amounts (refund corrections) pass through unchanged,
  //! per the function's doc comment.
  //!
  //! Changed behavior: an `if amount < 0 { return 0; }` guard now runs before
  //! the fee subtraction, so negative amounts return 0 instead.

  use inserted_guard::total_after_fee;

  #[test]
  fn negative_amount_passes_through_unchanged() {
      // Negative amounts are refund corrections and must keep the original
      // pass-through pricing: amount - fee = -500 - 250 = -750.
      // On the changed behavior the negative-amount guard short-circuits to 0
      // before the subtraction, so this assertion observes 0 and fails.
      assert_eq!(total_after_fee(-500, 250), -750);
  }
eval_output: |
  CRATE: <study-harness>/inserted-guard
  TEST:  negative_amount_passes_through_unchanged
  PHASE original(HEAD~1): cargo test exit=0; test negative_amount_passes_through_unchanged ... ok
  PHASE changed(HEAD):    cargo test exit=0; test negative_amount_passes_through_unchanged ... FAILED
  DISCRIMINATES: yes
  TEST_INPUT_EVIDENCE (from discriminate.rs):
    5:    // before the subtraction, so this assertion observes 0 and fails.
    6:    assert_eq!(total_after_fee(-500, 250), -750);
  RESTORED: yes (changed files back at HEAD)
discriminates: yes
notes: >
  Solver ran ripr itself per tool-note.txt: `ripr check --format agent`
  (top finding probe:src_lib.rs:return_value:6085c774, weakly_exposed,
  missing=No strong discriminator) then the nominated `ripr explain`, which
  prescribes "add a separating input and assert the exact returned value" —
  the test does exactly that (input -500/250, assert -750). context_bytes is
  wc -c of the saved solver ripr output per the ripr-tool rule
  (ripr-run-output-inserted-guard-ripr-tool-r01.txt); the given tool-note.txt
  itself is 1120 bytes. Anomalies: (1) eval.sh echoes cargo exit=0 for both
  phases because run_phase() discards cargo's exit code (the function returns
  the status of its final assignment); the verdict rests on the exact
  ok/FAILED lines, which were verified present in
  logs/inserted-guard-eval-{original,changed}.log; (2) cargo is not on Git
  Bash PATH in this environment — it was added via PATH for eval.sh and
  invoked by absolute path <cargo-bin>/cargo.exe for the
  solver's own test run; (3) ripr check took ~5-6 minutes wall time on the
  2-commit crate and was moved to background once by the 120s foreground
  timeout; it completed with exit 0. Solver check per prompt requirement 4:
  `cargo test negative_amount_passes_through_unchanged` on HEAD FAILED with
  assertion `left: 0, right: -750` (assertion failure, not a compile error).
  Crate re-reset after eval per runbook step 7.
