<!-- section: Fixed -->
- A TypeScript returned-expression edit whose changed number sits far from every
  asserted input (a clamp bound moved from `100` to `200`, tests asserting at
  `40`) now reads `weakly_exposed` instead of `exposed`: the separation probe
  also tries the constants the two expressions name. An equivalent rewrite is
  still not reported as unexercised (#9436, RIPR-SPEC-0243 rule 18).
