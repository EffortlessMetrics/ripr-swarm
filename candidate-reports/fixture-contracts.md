# check-fixture-contracts

Status: pass

## Why This Matters

Fixtures are the BDD control bench for analyzer behavior and output contracts.

## Violations

None detected.

## Benchmark semantic-oracle scope

Missing legacy status means unreviewed. Valid/invalid are reviewed expected-behavior declarations bound to retained semantic basis and historical native capture. This check validates their identities; it does not rerun tests, infer semantic truth or authenticate the producer. External executable bytes are NOT_REVERIFIED. Custody counts cover the six primary native test observations per reviewed view. Matcher executable and linked-library references are separately external and NOT_REVERIFIED. Invalid-oracle controls may pass this fixture contract. These labels do not change static discrimination, Lane 1 scorecards, judged-panel calibration or frozen denominators.

- Expected-behavior validity: valid=0, invalid=0, unreviewed=82 (legacy absent=82), rejected=0
- Primary native test observation custody: 0 local artifact byte checks; 0 externally retained artifact references NOT_REVERIFIED by this fixture check.
- Benchmark cases: 82. Historical semantic controls: views=2, historical_cases=1, valid=1, invalid=1, rejected=0.
- Historical semantic case completeness: complete_cases=1, incomplete_cases=0. Valid/invalid counts retain individually reviewed row judgments; incomplete or mismatched cases cannot count as complete historical cases.
- Historical control native test custody: 0 local artifact byte checks; 12 external artifact references NOT_REVERIFIED. The native custody totals cover only the six primary test observations per reviewed view. Matcher executables and linked libraries are external and NOT_REVERIFIED. Historical static executable bytes are also NOT_REVERIFIED; only their retained producer/capture identities are checked. Semantic review accepts only its exact answer-key/native-pairing subject; it does not accept the attached static analysis. Controls do not enter static/calibration cases, pilot selection or frozen denominators. The weak variant is a removal control only.
- Historical control regex-word-boundary-empty/corrected: reviewed expected-behavior declaration valid; retained historical static capture checked, without a normative static classification.
- Historical control regex-word-boundary-empty/original: reviewed expected-behavior declaration invalid; retained historical static capture checked, without a normative static classification.

## Rerun

```bash
cargo xtask check-fixture-contracts
```
