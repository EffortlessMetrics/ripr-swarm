<!-- section: Changed -->
- The nightly `first-run-main` job now gates its verdict rows against `metrics/dx-scoreboard/first-run-main-baseline.json` (3 of 3 first-run crates decisive on a main build), so a regression in the first-run verdicts fails the job instead of surfacing at release (#9276).
