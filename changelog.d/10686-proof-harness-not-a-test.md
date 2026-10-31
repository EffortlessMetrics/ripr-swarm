<!-- section: Changed -->
- A Kani proof harness (`#[kani::proof]`, `#[kani::proof_for_contract]`, or the
  same path under `cfg_attr`) is no longer scored as production source and is
  never credited as a test. Editing one no longer produces production findings;
  the run names the limitation `proof_harness_not_evaluated`, and a changed
  function that only a harness calls says a proof harness exists that ripr does
  not evaluate instead of advising a co-located test. No class or credit
  changes (#10686).
