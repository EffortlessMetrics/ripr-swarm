<!-- section: Changed -->
- When a `ripr check` diff only removes code, the default view now lists what
  was removed under `Safe next action`: up to three rows, each with the
  location, the removed code and what the static evidence on that old line
  shows, for example ``src/pricing.py:3 `raise ValueError(...)`: tests reach it
  but none appears to check it``. The rows restate existing classes; nothing
  is selected for repair and machine formats are unchanged (#15617, #15335).
