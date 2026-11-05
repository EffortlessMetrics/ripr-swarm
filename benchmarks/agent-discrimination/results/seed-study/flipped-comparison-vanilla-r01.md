run_id: flipped-comparison-vanilla-r01
scenario: flipped-comparison
variant: vanilla
context_bytes: 1099
rounds: 1
test_name: age_exactly_65_gets_senior_discount
final_test_source: |
  //! Discriminating test for the senior-discount eligibility boundary.
  //!
  //! Original behavior (HEAD~1): `if age >= 65` — a member aged exactly 65
  //! receives the 10% senior discount.
  //! Changed behavior (HEAD): `if age > 65` — a member aged exactly 65 no
  //! longer receives it.
  //!
  //! This test pins the ORIGINAL behavior at the exact boundary input
  //! (age = 65): it passes on the original code and fails on the changed code.

  use flipped_comparison::discount_percent;

  #[test]
  fn age_exactly_65_gets_senior_discount() {
      assert_eq!(discount_percent(65), 10);
  }
eval_output: |
  CRATE: <study-harness>/flipped-comparison
  TEST:  age_exactly_65_gets_senior_discount
  PHASE original(HEAD~1): cargo test exit=0; test age_exactly_65_gets_senior_discount ... ok
  PHASE changed(HEAD):    cargo test exit=0; test age_exactly_65_gets_senior_discount ... FAILED
  DISCRIMINATES: yes
  TEST_INPUT_EVIDENCE (from discriminate.rs):
    2:    assert_eq!(discount_percent(65), 10);
  RESTORED: yes (changed files back at HEAD)
discriminates: yes
notes: cargo was not on the harness Git Bash PATH; the first eval.sh attempt
  exited 2 ("ERROR: cargo not on PATH") and was re-run with
  <cargo-bin> prepended to PATH. Solver self-check
  (`cargo test age_exactly_65_gets_senior_discount` on changed HEAD) failed
  with a pure assertion failure (left: 0, right: 10) on the first try, so
  only one test-production round was needed. Crate re-reset (reset --hard +
  clean -fd) after the run.
