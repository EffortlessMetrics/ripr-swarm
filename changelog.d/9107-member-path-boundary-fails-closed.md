<!-- section: Fixed -->
- TypeScript preview no longer reports a repair packet as ready when the
  changed comparison's boundary is a dotted member path such as an object
  property or enum member; the packet is reported not ready with the
  unresolved operand named (#9107).
