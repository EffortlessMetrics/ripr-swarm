<!-- section: Fixed -->
- Whole-test contradictory-fold withholding now survives composition with
  unrelated file-mate tests: the infection-stage withholding disclosure fires
  when no credited related test establishes reach to the changed owner
  (`RelationReason::establishes_reach`), not merely when the credited set is
  empty, so adding same-file tests that never call the owner can no longer
  raise a `infection_unknown` finding to `exposed` by crediting proximity
  (#12412). Lone contradictory-test and mates-only `no_static_path` shapes are
  pinned in the RIPR-SPEC-0108 corpus.
