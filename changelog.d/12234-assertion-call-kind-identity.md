<!-- section: Fixed -->
- Repository exposure reports no longer credit a free function's match arm
  with an assertion on an established same-named receiver method, or the
  reverse. The selected assertion's binding is checked; unresolved receivers,
  local callables, wrappers and unresolved value dependencies retain their
  existing uncertainty. Refusal requires literal inputs and a closed asserted
  value, including the candidate body and comparison behavior (#12234).
