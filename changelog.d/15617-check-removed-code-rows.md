<!-- section: Changed -->
- When a `ripr check` diff only removes code, the default view now lists what
  was removed under `Safe next action`: up to three rows, each with the
  location, the removed code, the finding kind and what the static evidence on
  that old line shows, for example ``./src/lib.rs:4 `if x == 0 { return
  Vec::new(); }` (kind: return_value): tests reach it but check it only
  loosely``. The rows restate existing classes; nothing
  is selected for repair and machine formats are unchanged (#15617, #15335).
