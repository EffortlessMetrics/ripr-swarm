<!-- section: Fixed -->
- A test that calls a std collection method such as `Vec::contains` is no
  longer read as a direct call of a same-named trait default method
  (`Flags::contains`) when a macro declares the implementing type and spells
  the trait impl for it, as a bitflags-shaped crate does. The test stays a
  name-only relation that grants no reach, exactly as for a source-level impl.
  A macro whose impl self type is a metavariable, a std type or otherwise not
  established keeps the existing relation (#11113).
