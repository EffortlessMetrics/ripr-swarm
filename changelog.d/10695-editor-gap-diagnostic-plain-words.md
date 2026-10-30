<!-- section: Changed -->
- Editor gap diagnostics read as plain English in the Problems panel
  ("Missing boundary assertion: amount >= threshold. Suggested check:
  ...") instead of `ripr gap: MissingBoundaryAssertion; repair route:
  AddBoundaryAssertion`. The diagnostic `code` and `data` payload, including
  the gap kind and repair route, are unchanged (#10695).
