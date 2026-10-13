<!-- section: Fixed -->
- TypeScript (RIPR-SPEC-0257): `expect(price(150)).toMatchSnapshot()` now
  reads as a strong oracle when its entry (`price 1`) is committed in
  `__snapshots__/<test file>.snap` beside the test, so a changed return
  value reads `exposed` instead of `weakly_exposed` with
  `typescript_snapshot_discriminator_unresolved`. The entry must be one the
  test file names without running: one snapshot call in the test body, none
  in hooks, a unique title, no second entry, and no snapshot resolver or
  update flag in a runner config or `package.json` script. On the
  TypeScript verdict corpus false verdicts drop by one (#6654).
