<!-- section: Fixed -->
- Owner-return pins now credit an inline expect-test comparison,
  `expect![["w12"]].assert_eq(&label(4))`, as they do
  `assert_eq!(label(4), "w12")`, when `use expect_test::expect;` is
  verified against the manifest, in any spelling the parser accepts
  (`expect!["w12"]`, `expect! [[..]]`, a line-split call). An owner whose
  whole body is one `format!("..", args)` or `format!("{name}")` tail now
  has a direct propagation witness, so an
  exact pin on it no longer reads as a weak gap (#6578).
- An inline expect-test comparison in a test body now reads as one
  `snapshot` oracle at its own line, so the `.assert_debug_eq(..)`
  continuation of a multi-line expectation is no longer read as a separate
  exact assertion (#6578).
