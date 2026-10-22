<!-- section: Fixed -->
- A function signature that wraps over several physical lines no longer seeds
  one `static_unknown` probe per line. The parameter, return-type and
  body-opening lines of a new function with an added body seed no probe, like
  the one-line form (the body keeps its own), and two or more changed lines of
  one existing signature are one declaration change: one probe, kept on the
  first changed line. A `where` clause or generic type bound keeps its own
  declaration `static_unknown` probe (#8999).
