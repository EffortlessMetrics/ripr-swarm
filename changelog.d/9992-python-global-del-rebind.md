<!-- section: Fixed -->
- A Python protocol-entry receiver named by a `global r`, `del r` or
  `type r = ...` statement in the test body is no longer credited `exposed`
  through `protocol_entry_call`; the rebind scan now reads those keywords like
  `nonlocal`. A `del` or `global` that lists the name after another target
  (`del x, r`) is a known limit tracked with #9531 and keeps its credit
  (#9992).
