<!-- section: Fixed -->
- `ripr check`'s `Start here` no longer treats the advisory next step on a
  `no_static_path` or unknown finding as a repair route. Those findings never
  print one, so a test gap is now selected ahead of them. Among themselves
  they keep their previous evidence order (#14266).
