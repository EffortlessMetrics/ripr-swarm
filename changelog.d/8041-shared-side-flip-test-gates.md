<!-- section: Fixed -->
- The sole-source `?` side-flip producer now uses the same fail-closed
  test gates as the guard side-flip producer (RIPR-SPEC-0227 rules 3 and
  3b): an attribute in the test body, including on a tail assertion, and
  an assertion message that can leave (`?` or a nested block) no longer
  credit the `?` as `exposed` (#8041).
