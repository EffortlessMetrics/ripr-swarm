<!-- section: Changed -->
- `ripr agent status`'s inventory refusal to choose between several
  awaiting repair attempts or several open seams now follows the canonical
  next-action decision (`choose_attempt` / `choose_item`) instead of a local
  count, with the count kept as the shared fallback. Rendered text is
  unchanged (#6986, inventory slice).
