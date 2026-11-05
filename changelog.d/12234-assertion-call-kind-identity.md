<!-- section: Fixed -->
- Repository exposure reports no longer credit a free function's match arm
  with an assertion on a same-named receiver method, or the reverse. Actual
  owner calls and wrappers that may reach the owner retain their evidence
  (#12234).
