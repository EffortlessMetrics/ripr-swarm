# release-challenge judgments

packet: metrics/rust-judged-behavior-panel/release-judgments.json
selection: metrics/rust-judged-behavior-panel/release-selection.json (sha256:69c9b45616bd4c9bd13e8da0d668e52c39bca32485f93335589009e77c3fdd72)
rows: 21
terminal confirmed_should_gap: 8
terminal confirmed_should_stay_quiet: 10
terminal confirmed_should_limit: 3
terminal inconclusive_missing_evidence: 0
terminal inconclusive_disagreement: 0
terminal invalid_case_identity: 0
expected should_gap: 6
expected should_stay_quiet: 12
expected should_limit: 3
reference false_exposed: 5 true of 21 established, 21 rows
reference false_actionable: 0 true of 3 established, 21 rows
reference under_credit: 4 true of 10 established, 21 rows
reference limitation_correct: 1 true of 3 established, 21 rows
departs from expected: p1741-helper-rows-quiet (expected should_stay_quiet, judged should_gap)
departs from expected: p1745-catalog-row-quiet (expected should_stay_quiet, judged should_gap)
departs from expected: p1745-triple-rows-gap (expected should_gap, judged should_stay_quiet)
departs from expected: p1617-command-rows-quiet (expected should_stay_quiet, judged should_gap)
departs from expected: p1705-headline-rows-gap (expected should_gap, judged should_stay_quiet)
departs from expected: p1705-passthrough-rows-quiet (expected should_stay_quiet, judged should_gap)
reference run: ripr 0.11.0 (dev profile, binary sha256:6e51f363cb017d26...) (EffortlessMetrics/ripr-swarm main c989c30a8 (2026-09-27))
