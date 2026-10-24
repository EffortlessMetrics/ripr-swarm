<!-- section: Fixed -->
- A method of a workspace-declared trait that no source names is no longer
  treated as reachable unseen, so a same-file test of another function cannot
  lend it a strong oracle: the finding is `no_static_path`, not `weakly_exposed`
  with "Strong oracle found". A test related only by a shared name token never
  credits an oracle (RIPR-SPEC-0108; #9169).
  Trade-off: a related test that shares only a name token with an owner (common
  names such as `new`, `fmt`, `process`, including an unresolved receiver) no
  longer lends a strong oracle, so some findings read `reachable_unrevealed`
  ("inspect first") where they used to read `weakly_exposed`. Known limitation:
  a bare trait name that arrives through a glob import and collides with a
  workspace trait of the same name is read as the workspace trait, which also
  fails toward `no_static_path`. The import scan masks string and character
  literals as well as comments, so a `//` or `/*` inside a literal no longer
  hides a later `use` (#10050).
