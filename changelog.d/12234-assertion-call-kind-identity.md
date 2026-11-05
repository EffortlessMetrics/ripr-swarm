<!-- section: Fixed -->
- Repository exposure reports no longer credit a free function's match arm
  with an assertion on an established same-named receiver method, or the
  reverse. The selected assertion's binding is checked; unresolved receivers,
  local callables and wrappers retain their existing uncertainty (#12234).
