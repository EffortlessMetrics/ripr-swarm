# Counted-check join falsifier

The JSON files are byte-preserving copies of the field-preserving projections
retained by EffortlessMetrics/perl-lsp-swarm PR #16717 at `4fd2dbb890`:

- `xtask/tests/fixtures/ripr-0.10/hosted-16619-static-limitations.json`, Git blob
  `183179410b10938ade8dea7b35a9257ad149dd8e`;
- `xtask/tests/fixtures/ripr-0.10/hosted-16619-counted-raw-gaps.json`, Git blob
  `6a9390642af219ba52a17756fe6c688ca886b37f`.

Source artifact: perl-lsp-swarm run `36419991913`, artifact `10974153790`, RIPR
0.10.0. The three changed-literal probes at lines 66/126 are distinct from all
ten guidance cards (nine static limitations and one actionable card).

The regression replays the new disclosure over the retained projection. It does
not rerun the historical analyzer, reproduce a hosted job, or implement consumer
clearance. The separate renderer regression exercises both current producers.
