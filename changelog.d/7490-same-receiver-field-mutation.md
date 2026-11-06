<!-- section: Fixed -->
- `ripr check` recognizes exact reads of a field after a mutation on the same
  receiver, including resolved primitive getters and declared enum variants,
  while retaining ambiguous receiver and observer refusals. Discarded async
  calls and integer literals or arguments outside their declared type do not
  supply mutation observation identity (#7490).
