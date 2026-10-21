<!-- section: Fixed -->
- Field pins equal to an operand retain operator evidence when a local callee
  binding, import, opaque declaration macro or same-named associated function
  prevents establishing a primitive operand type. Later local bindings preserve
  the downgrade for an earlier genuine primitive call (#7077).
