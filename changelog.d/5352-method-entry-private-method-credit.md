<!-- section: Fixed -->
- Verdicts: a private method reached only through a public entry now earns
  helper-chain credit when every hop is an identity-bound method call (`self.`
  inside the callee's impl, or a value constructed from the callee's type) and
  the entry's exact assertion observes the forwarded result. Free-function
  chains, variable receivers, wrong-owner tails, smoke assertions and
  comparison-shaped returns stay uncredited (#5352).
