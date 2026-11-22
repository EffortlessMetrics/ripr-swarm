<!-- section: Changed -->
- When `ripr review-comments` runs out of time in `canonical_analysis`, the
  error and the run receipt's repair route now name the recovery routes: run
  `ripr check --root <root> --base <base> --committed --json` at the head
  checkout for findings without review guidance, or narrow `--base`/`--head`
  to a smaller diff. A larger `--timeout-ms` is listed last because the phase
  restarts from the beginning (#12940).
