<!-- section: Fixed -->
- A changed `self.field = ..` write that a related test reads only with a relational check (`>`, `<`, `!=`, `assert_ne!`) now starts a repair, and `ripr check` names the same start; any other statement in the test that names the field or reuses the binding (an alias, a helper call, a destructure, an `if` check) or a non-plain `Drop` type withholds it (#11870).
