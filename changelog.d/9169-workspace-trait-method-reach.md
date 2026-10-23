<!-- section: Fixed -->
- A method of a workspace-declared trait that no test calls no longer reads
  `weakly_exposed` with a "strong oracle" borrowed from other functions' tests.
  A proximity-only test cannot reach it unseen, so the finding is
  `no_static_path`; std and foreign trait impls keep their allowance (#9169).
