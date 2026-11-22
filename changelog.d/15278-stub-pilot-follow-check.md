<!-- section: Fixed -->
- `ripr repair <finding id>`, `ripr agent stub` and `ripr pilot` no longer say
  the existing tests already check a spot while `ripr check` reports a gap for
  the changed code there. `ripr repair <finding id>` names check's reading
  instead of refusing with "existing tests already check it". `ripr agent stub`
  keeps `grip_class: strongly_gripped`, reports check's class in
  `classification`, and says the stub is for that gap. `ripr pilot` adds that
  `ripr check` reads the changed code as a gap or cannot tell, and points to
  `ripr check`. When check reads the change as exposed the wording is
  unchanged (#15278).
