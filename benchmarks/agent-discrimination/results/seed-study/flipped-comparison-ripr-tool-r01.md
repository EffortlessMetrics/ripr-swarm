run_id: flipped-comparison-ripr-tool-r01
scenario: flipped-comparison
variant: ripr-tool
context_bytes: 4254
rounds: 1
test_name: age_65_receives_senior_discount
final_test_source: |
  //! Discriminating test for the HEAD~1 -> HEAD change to `discount_percent`.
  //!
  //! The change flipped the senior-discount threshold at src/lib.rs:7 from
  //! `age >= 65` (original) to `age > 65` (changed). The documented original
  //! behavior is "Members aged 65 and over receive the senior discount".
  //! This test pins that original behavior at the exact boundary input: a
  //! member aged exactly 65 must receive the 10% senior discount. It passes
  //! on the original behavior and fails on the changed behavior, which
  //! excludes age 65 and returns 0 there.

  use flipped_comparison::discount_percent;

  #[test]
  fn age_65_receives_senior_discount() {
      // Exactly at the boundary: original behavior (`age >= 65`) grants the
      // senior discount; the changed behavior (`age > 65`) denies it.
      assert_eq!(discount_percent(65), 10);
  }
eval_output: |
  CRATE: <study-harness>/flipped-comparison
  TEST:  age_65_receives_senior_discount
  PHASE original(HEAD~1): cargo test exit=0; test age_65_receives_senior_discount ... ok
  PHASE changed(HEAD):    cargo test exit=0; test age_65_receives_senior_discount ... FAILED
  DISCRIMINATES: yes
  TEST_INPUT_EVIDENCE (from discriminate.rs):
    4:    assert_eq!(discount_percent(65), 10);
  RESTORED: yes (changed files back at HEAD)
discriminates: yes
notes: >
  Test produced in 1 round (single write + single cargo-test verification, no
  iteration). Two extra environment rounds were needed: cargo was not on the
  Git Bash PATH (used absolute <cargo-bin>/cargo.exe) and the
  first eval.sh invocation failed with "ERROR: cargo not on PATH" (exit 2)
  before a PATH-fixed rerun. Anomaly: a concurrent flipped-comparison vanilla
  run (flipped-comparison-vanilla-r01, finished 17:05, discriminates=yes) and
  a second unidentified solver were writing tests/discriminate.rs on this same
  crate during 17:03-17:07; two of my writes were blocked by concurrent file
  changes and I waited ~6 minutes for the crate to go quiet before writing my
  test (30 tool rounds total for the run, of which 1 was test production).
  My test content never iterated. Saved ripr output:
  contexts/flipped-comparison/ripr-run-output-flipped-comparison-ripr-tool-r01.txt
  (ripr check finding: weakly_exposed src/lib.rs:7 age > 65, missing=age == 65;
  ripr explain confirmed before age >= 65 / after age > 65). tool-note.txt was
  1132 bytes; per the runbook ripr-tool rule, context_bytes counts the saved
  solver ripr output.
