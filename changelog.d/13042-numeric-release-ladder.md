<!-- section: Docs -->
- `docs/METRICS.md` defines a numeric release ladder, R0 to R3, tied to the
  support tiers in `docs/status/SUPPORT_TIERS.md` (`preview` needs R0,
  `usable alpha` R0 and R1, `usable` R0 to R2). The R0 install bar is split:
  install to first useful result in at most 300 s on every shipped prebuilt
  channel and at most 900 s for a `cargo install` source build, judged on the
  median. The page records that R0 is not yet met (#13042, #13713).
