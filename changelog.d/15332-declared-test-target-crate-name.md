<!-- section: Fixed -->
- A `[[test]]` target whose `path` sits outside `tests/` (for example
  `checks/score_check.rs`) pairs its `my_pkg::owner(..)` boundary call the way
  a `tests/` file does, so a discriminating test there reads `exposed` instead
  of `weakly_exposed`; `test = false`, `harness = false` and
  `required-features` entries keep the earlier reading (#15332).
