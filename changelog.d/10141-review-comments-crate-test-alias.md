<!-- section: Fixed -->
- `ripr review-comments` attributes an added module-level `#[crate::test]`
  function as a test edit when the unchanged crate root aliases a path-bound
  workspace wrapper macro; missing aliases, lookalikes, and foreign imports
  stay uncredited (#10141).
