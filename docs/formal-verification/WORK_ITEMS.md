# Current formal-verification work items

This is the current execution index for the architecture in this directory.
It points to existing owners and the three newly split infrastructure leaves.
It is not a second programme or claim registry.

## Immediate dependency order

```text
#12581 bounded Kani version preflight
    │
    ├── #14469 positive-cover admission
    │
    ├── #14472 machine native-result contract
    │       │
    │       └── #14473 stable critical claims + assurance scoreboard
    │               │
    │               └── #14474 affected critical selection + advisory lane
    │
    └── #11517 declared Kani context/source/cache/public inventory
            │
            └── #12596 owner/domain/asserted observable
                    │
                    ├── #12512/#10749 guard-only transitions
                    │       │
                    │       └── #10687 bounded weakness findings
                    │
                    └── #12597/#10689 native observation correlation
                            │
                            └── #12598 stages B/C + #10746 corpus
```

Verification-authority transition work (#6452 then #6456/#5224) must complete
before any required-check promotion under #12904.

## Active candidates to finish, not replace

| Owner | Current result | Required next transition |
| --- | --- | --- |
| #12579 / PR #12581 | Bounded preflight implementation exists; branch was behind/nonmergeable at the November 11 reconciliation | Reconcile the existing candidate onto current main, preserve its delayed-probe and positive evidence, qualify exact head, then merge |
| #13768 / draft PR #14469 | Positive-cover admission candidate | Distinguish claim-required covers from harness-local/advisory covers; retain native negative evidence; compose with #14472 |
| #10686 / draft PR #11517 | Large context/source/cache/public-inventory candidate | Keep owner/domain semantics out; split producer/context and projections if needed; qualify current head instead of recreating work |
| #14472/#14473 / draft PR #14478 | Offline claim/run document auditor and strengthened design contracts; native assurance remains `not_assessed` | Reuse one semantic validator at the native producer/report boundary, generate the governed registry/result join, and avoid a second admission implementation |
| #14474 / stacked passive selector candidate | Supplied-fact critical selection, all-critical fallback, queue identity, and JSON/Markdown planning without provider execution | Qualify this contract, then generate its claim index from #14473 and bind actual `B/G/member/change` facts under #14500 before any advisory execution |

## Newly split infrastructure leaves

### #14472 — machine native Kani results

Produces versioned JSON plus Markdown from one DTO, with exact run, process,
inventory, cover, currentness, and raw-artifact identity.

It consumes the delivered runner-integrity fixes and composes with #12581 and
#14469. It does not add a proof, semantic relation, or required check.

Draft PR #14478 supplies an executable offline consistency auditor and corrects
the draft schemas, but explicitly does not emit or authenticate native evidence.
The production runner/result integration remains open.

### #14473 — stable claims and assurance scoreboard

Registers the first critical self-assurance propositions and joins them to
#14472 results. It keeps the existing source census separate and refuses to
report syntax count as established assurance.

The first claim families are:

1. classification/actionability;
2. gate/exit/result aggregation;
3. currentness/identity/causal comparison;
4. resource/budget safety;
5. edit/repair final disposition.

PR #14478 validates supplied draft claim documents but creates no governed
registry and no assurance scoreboard.

### #14474 — affected critical selection

Plans an advisory `kani-critical` route from registered semantic claims for the
actual admitted merge group. Unknown, incomplete, empty, or unmapped
formal-relevant impact falls back to all critical claims, never no work.

The stacked passive selector candidate adds the provider-free plan contract and
falsification controls. It grants no execution authority and adds no workflow
trigger. Native execution remains dependent on #14472/#14473 and #14500's
queue-only lifecycle.

It remains advisory until #12904's measured admission criteria and
#6452/#6456 transition protection are satisfied.

## First product path

The first installed user-facing formal-verification result remains Kani-first:

1. land/split #11517 as context/source inventory;
2. implement #12596 exact owner/domain/observable relation;
3. implement #12512/#10749 guard-only transitions;
4. implement #10687 bounded static weakness findings;
5. implement #12597/#10689 exact native observation correlation;
6. execute #12598 stages B/C;
7. qualify through #10746's adversarial corpus.

Static guard facts, native result, adequacy experiment, and trusted execution
remain separate axes.

## Next assurance layers

### TLA+ repair-lifecycle pilot

The pilot branch `codex/repair-lifecycle-model-20261106` has
[direct native TLC evidence](https://github.com/EffortlessMetrics/ripr-swarm/issues/12109#issuecomment-6813436649)
at source `30ec9ac187025985314858ff9ba9d72bf1ad02e9`: the positive model run
explored 199 distinct states with an empty frontier, and all three original
negative configurations failed their named invariants. These are model-only
observations, not Rust adapter or implementation-conformance evidence.

Remaining qualification includes:

- Rust adapter compilation/tests;
- producer-integrated model/config/tool/source commitments and retained native results;
- state/action-to-Rust mapping;
- six-case bounded conformance comparison;
- current-main integration.

Model safety does not establish Rust implementation conformance by itself.

### Verification-authority transition

Implement #6452 and #6456/#5224 so a PR cannot use its own weaker proof,
model, bound, assumption, provider, or CI policy to erase applicable base
protection.

### Verus and later providers

After the Kani path is end-to-end:

- complete Verus body-level trust facts;
- implement #10748 native result adapter;
- execute #10753 adequacy experiment;
- exercise one same-behavior Kani/Verus journey without equivalence claims;
- admit Miri, Loom/Shuttle, fuzz, mutation, or coverage providers only through
  #6381/#6385/#6390 with an exact proposition, denominator, provider contract,
  negative corpus, and cost boundary.

## Maintenance rule

When an owner lands, update this file in the same integration slice:

- move delivered work out of “active candidates”;
- record the accepted commit and exact remaining successor;
- preserve failed, stale, or superseded evidence rather than deleting it;
- do not mark a downstream claim complete merely because a prerequisite
  merged.
