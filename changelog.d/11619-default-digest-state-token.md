<!-- section: Changed -->
- The default human digest no longer leads the missing-check line with a
  banned snake_case state token: a finding whose classifier wording starts
  `propagation_unknown: `, `weakly_exposed: `, `reachable_unrevealed: `,
  `no_static_path: `, `infection_unknown: ` or `static_unknown: ` now prints
  the prose that follows the label, so the line reads as plain language. JSON
  `missing` and `human-full.txt` keep the classifier wording unchanged (#11619).
