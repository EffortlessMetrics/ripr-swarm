<!-- section: Fixed -->
- A helper chain refused only because an entry name is not unique in the
  workspace no longer reads as `no_static_path` when a test calls that
  name (or a non-unique caller of it). The relation stays refused; the
  finding is `static_unknown` and names the ambiguous function. A chain
  that no test enters, including a unique wrapper of a non-unique helper,
  still reads `no_static_path` (#7080).
