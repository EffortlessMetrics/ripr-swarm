<!-- section: Fixed -->
- TypeScript preview: a new function, method or arrow whose signature is
  wrapped over several lines no longer gets a finding on each signature line
  (the opening line, every parameter line and the `): T {` closer). The body
  lines carry the change, as they already did for a one-line signature. A
  wrapped signature with a default value, a parameter property or a decorator
  keeps its findings (#14097).
