<!-- section: Fixed -->
- Tests written `#[crate::test]` (or `#[self::test]`) are now recognised when
  the crate root has a single `use ... as test;` (ungated or `#[cfg(test)]`) aliasing a
  `#[proc_macro_attribute]` of the analyzed workspace whose package names a
  test attribute (`tokio::test`, `async_std::test` or `#[test]`), as in
  tokio-rs/axum. Unresolved, external, feature-gated or ambiguous aliases and
  arbitrary `#[x::test]` paths stay non-tests, so tests added in a diff no
  longer read as missing and probes no longer fall to `weakly_exposed` for
  want of them (#9001).
