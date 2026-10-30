<!-- section: Fixed -->
- `ripr repair` with several candidates now lists each one with its file:line
  and the changed code beside the command that picks it, instead of bare ids.
  When a chosen change has no test file ripr can edit, the refusal says in two
  sentences where the change is, why ripr cannot pick a place (naming the
  competing test modules), and the pre-filled `ripr agent stub` command to run
  (#10978). `ripr check`'s next step for a finding with no detected boundary
  input now says the same thing as its missing-check line.
- `ripr repair` no longer starts an attempt for a seam whose only direct test is
  an assertion-free `#[should_panic]` test that the changed code may decide:
  repair consumes the same predicate `ripr check` uses for that named
  limitation, so the two agree (#10703).
