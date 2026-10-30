<!-- section: Fixed -->
- Bare `ripr repair` now finds your changed lines the way `ripr check` does,
  so a change you already committed on a branch scopes the refusal to those
  lines instead of listing other code beside them. When no base can be found,
  or nothing differs from it, repair keeps its wider selection and now says
  first that it could not tell which lines changed. `ripr repair --base REF`
  names the base explicitly, as on `ripr check` (#11246).
