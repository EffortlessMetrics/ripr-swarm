<!-- section: Fixed -->
- A method of a workspace-declared trait that no source names is no longer
  treated as reachable unseen, so a same-file test of another function cannot
  lend it a strong oracle: the finding is `no_static_path`, not `weakly_exposed`
  with "Strong oracle found". A test related only by a shared name token never
  credits an oracle (RIPR-SPEC-0108; #9169).
