<!-- section: Fixed -->
- `ripr check`: a changed line whose names merely contain `async`, `await`
  or `spawn` (`respawn_limit`, `awaiting_ack`, `asynchronous_mode`, a
  `"async"` string) no longer gets the `async_boundary_opaque` stop reason or
  the "if a task boundary is involved" advice. A name part that is exactly
  `async`, `await` or `spawn` still counts (`.await`, `spawn_blocking`,
  `async_trait`, `spawnSync`, `asyncio`) (#14875).
