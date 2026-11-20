<!-- section: Changed -->
- Repair starts and agent packets for a changed predicate boundary now draft
  the concrete assertion their own evidence supports — for example
  `assert_eq!(discounted_total(100, 100), 90)` instead of
  `assert_eq!(discounted_total(/* boundary input where ... */), /* expected */)`
  — when the related tests' own call rows pin the equality-boundary input to
  one value and the owner's branch value folds there under bounded integer
  arithmetic. Where the expected value is not statically derivable the
  assertion keeps its `/* expected */` placeholder while still emitting the
  concrete input side, and every draft carries the standing honesty note:
  drafted from the current behavior; ripr has not run or verified anything,
  and the developer confirms the expected value before relying on the test.
  The packet JSON marks such assertions with the new `drafted_static_fold`
  assertion basis and carries the typed parts as `drafted_boundary` on the
  missing-discriminator fact (#12414).
