<!-- section: Fixed -->
- The repair loop credits natural `assert!(predicate(boundary))` tests and
  their negated form for simple boolean integer boundaries, so a test that
  pins the boundary result can move its repair receipt from weak to exposed
  without rewriting it as `assert_eq!(.., true)` (#8610).
