<!-- section: Fixed -->
- `ripr check`: a changed line whose names merely contain `async`, `await`
  or `spawn` (`respawn_limit`, `awaiting_ack`, a `"async"` string) no longer
  gets the `async_boundary_opaque` stop reason or the "if a task boundary is
  involved" advice. Only real `async`/`.await` code and `spawn`/`spawn_*`
  calls count (#14875).
