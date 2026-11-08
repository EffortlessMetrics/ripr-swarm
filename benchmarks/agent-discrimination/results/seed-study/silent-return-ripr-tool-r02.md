run_id: silent-return-ripr-tool-r02
scenario: silent-return
variant: ripr-tool
context_bytes: 4794
rounds: 1
test_name: cash_on_weekend_still_waits_for_reconciliation
final_test_source: |
  //! Discriminating test for the silent early return on the CashOnDelivery arm.
  //!
  //! Original behavior (HEAD~1): the `CashOnDelivery` arm yields `base_hours = 0`
  //! and control flows on to the weekend reconciliation block, so cash captured
  //! on a weekend settles in 0 + 24 = 24 hours ("Every method, including cash,
  //! waits for the end-of-day reconciliation when captured on a weekend").
  //!
  //! Changed behavior (HEAD): the arm is `PaymentMethod::CashOnDelivery => return 0`,
  //! which silently bypasses the weekend surcharge and returns 0 on weekends too.
  //!
  //! The input below selects the changed match arm AND the bypassed weekend path.
  //! A weekday cash input would not discriminate: it returns 0 on both sides.

  use silent_return::{settlement_hours, PaymentMethod};

  #[test]
  fn cash_on_weekend_still_waits_for_reconciliation() {
      assert_eq!(
          settlement_hours(PaymentMethod::CashOnDelivery, true),
          24,
          "cash captured on a weekend must still wait the end-of-day reconciliation (+24h), not bypass it via an early return"
      );
  }
eval_output: |
  CRATE: <study-harness>/silent-return
  TEST:  cash_on_weekend_still_waits_for_reconciliation
  PHASE original(HEAD~1): cargo test exit=0; test cash_on_weekend_still_waits_for_reconciliation ... ok
  PHASE changed(HEAD):    cargo test exit=0; test cash_on_weekend_still_waits_for_reconciliation ... FAILED
  DISCRIMINATES: yes
  TEST_INPUT_EVIDENCE (from discriminate.rs):
    2:    assert_eq!(
  RESTORED: yes (changed files back at HEAD)
discriminates: yes
notes: >
  Run executed under r02 after an owner-authorized takeover (escalation
  answered: "wait for quiescence, then take over under your own run id").
  COLLISION: a second solver lane was concurrently executing this same
  scenario+variant on this crate during my run. Evidence: tests/discriminate.rs
  appeared during my session (mtime 17:19:16) although my start-of-run reset
  showed a clean tree; it was rewritten at 17:23:42 with a renamed function;
  silent-return/target was touched 17:20:03 and Cargo.lock 17:19:16 though I
  had run no cargo here (other scenario crates' targets are hours-old);
  git status showed staged "M src/lib.rs" at 17:24:00 which vanished by
  17:24:30 - that lane transiently violated the no-src-modify rule, so its
  eval evidence (had it reported) would only be valid if the violation never
  overlapped its eval window. I wrote nothing into the crate during the
  collision: my first Write failed read-first (which exposed the collision),
  I diagnosed with ripr/process/stat evidence, and escalated. The 30-minute
  bounded wait (17:27-17:57) saw no r01 report and the crate went clean with
  no report - the takeover reset at ~17:58 orphaned that in-flight r01 writer.
  The orphaned lane's test encoded the SAME discriminator I derived
  independently (assert settlement_hours(CashOnDelivery, true) == 24), i.e.
  pair replication evidence, not a rival result. My test content never
  iterated: one write + one cargo-test verification (FAILED with
  left: 0 / right: 24 on changed code, the intended behavioral red witness).
  Rounds: 1 test-production round; ~36 total tool rounds including reset,
  ripr check/explain, collision diagnosis, escalation, the bounded wait
  (1 background poll + 4 wait-state calls), takeover reset, and eval.
  Environment: cargo was not on the Git Bash PATH (used absolute
  <cargo-bin>/cargo.exe); the first eval.sh invocation failed
  with "ERROR: cargo not on PATH" (exit 2) and was rerun unchanged with
  PATH prepended - recorded as an instrument failure, not a verdict.
  Saved solver ripr output: contexts/silent-return/ripr-run-output-silent-return-ripr-tool-r01.txt
  (4794 bytes, single-writer intact, mtime 17:19:41; filename carries r01
  because it was saved before adopting r02). ripr check: 2 findings, both
  weakly_exposed at src/lib.rs:19 (match arm "PaymentMethod::CashOnDelivery =>"
  and its "return 0"), missing=No strong discriminator; ripr explain confirmed
  before "CashOnDelivery => 0," / after early return and flagged the existing
  weekday-cash assertion as observation-unconfirmed. tool-note.txt was 1117
  bytes; per the runbook ripr-tool rule, context_bytes counts the saved solver
  ripr output. HARNESS DESIGN FINDING (extends the inserted-guard finding):
  scenario crates in shared <host-temp> are discoverable and mutable by other
  fleet runtimes; run-id-namespaced result files are not enough - the crates
  themselves need per-run clones (run-id-namespaced) and benchmark dirs should
  not live in shared temp space.
