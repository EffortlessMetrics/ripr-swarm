<!-- section: Fixed -->
- `ripr review-comments` attributes an added module-level `#[crate::test]`
  function as a test edit when the unchanged crate root aliases a path-bound
  workspace wrapper macro; missing aliases, lookalikes, and foreign imports
  stay uncredited. Companion crate-root tests do not match changed-file line
  numbers, and wrapper identity is the consumer manifest at the requested
  head rather than a dirty worktree (#10141).
