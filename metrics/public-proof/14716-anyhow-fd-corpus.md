# anyhow + fd corpus increment

- Population: **anyhow+fd increment at db01ef5618: two-repository pilot, not a replacement for the five-crate receipt or the named anyhow+fd+bat population**.
- Analyzer: `ripr 0.11.0`; checkout revision `db01ef5618f43a6af4291d94e3b849c27c581b9b` (verified independently).
- Completed in one lane job: **yes**; runner wall time **784.49 s (13.07 min)**.
- Command: `cargo xtask mutation-spot-check --repo anyhow=<checkout> --repo fd=<checkout> --mutants-arg fd=--package=fd-find --run-mutants --jobs 3 --mutant-timeout-secs 60`.
- Raw receipts: `14716-anyhow-fd-mutation-spot-check.json` and `14716-anyhow-fd-mutation-spot-check.md`.
- Separate named anyhow+fd+bat evidence is 10/14 = 0.714; this two-repository run does not include bat and does not recompute that population.

## Mutant counts

| Mutants | Caught | Missed | Timeout | Unviable | Canonical precise | No containing seam |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 679 | 399 | 141 | 3 | 136 | 146 | 197 |

## Pilot recommendation tiers

Each denominator is that tier's confirmed + refuted recommendations. Unscored rows stay separate; canonical-precise is not substituted for the denominator.

| Tier | Confirmed | Refuted | Unscored | Scored denominator | Total | Confirmed fraction |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| seam | 0 | 4 | 0 | 4 | 4 | 0/4 |
| line | 1 | 0 | 0 | 1 | 1 | 1/1 |
| owner | 1 | 4 | 0 | 5 | 5 | 1/5 |
| none | 0 | 0 | 10 | 0 | 10 | n/a |

This two-repository incremental receipt does not publish a single precision rate for the separate named population and does not assert completion of bat.
