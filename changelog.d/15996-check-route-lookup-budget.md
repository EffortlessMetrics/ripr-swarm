<!-- section: Fixed -->
- `ripr check` no longer runs far past `--timeout` on a function that is one
  large `match` of literal arms. The repair-start lookup that follows the
  analysis reread the match for every arm and sat outside the budget, so 8,000
  arms held `ripr check --timeout 5` for 135 seconds. The arms are now read
  once, and the lookup spends only what the analysis left of the budget; when
  that runs out the screen says so and names `ripr repair` as the way to look
  the start up, with the findings unchanged. A test-stub start whose lookup
  the budget stopped is left out, never printed half-checked (#15996).
