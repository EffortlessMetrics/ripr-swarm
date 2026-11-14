# Current formal-verification work items

This is the current execution index for the architecture in this directory.
It points to existing owners and the three newly split infrastructure leaves.
It is not a second programme or claim registry.

## Immediate dependency order

| Claim | Prerequisite or composition | Result |
| --- | --- | --- |
| #12579 / #12581 | Preserve delivered attribution and process-completion owners | Bounded, diagnosable version preflight |
| #14460 / #14469 | Independent collector repair; retain native controls | Complete positive cover evidence |
| #14472 / #12470 | Compose the two runner repairs and reuse #14478 admission predicates | Native JSON/Markdown, raw custody and exact inventory |
| #14473 | Consume current #14472 results | Stable critical claims and assurance scoreboard |
| #14474 | Consume #14473; retain #6452/#6456 protection | Conservative affected plan on admitted qualification subject |
| #10686 / #11517 | Separate #10745 product track | Declared Kani context/source/cache/public inventory |
| #12596 and product successors | Consume #11517 without crediting it as self-proof | Exact owner/domain/observable and public native correlation |

Verification-authority transition work (#6452 then #6456/#5224) must complete
before any required-check promotion under #12904.

## Active candidates to finish, not replace

| Owner | Current result | Required next transition |
| --- | --- | --- |
| #12579 / PR #12581 | Bounded preflight implementation; current repair requires distinct sanitized failure classes and an independent test deadline | Resolve actual conflicts, preserve native process controls, qualify the repaired head and its integration |
| #14460 / PR #14469 | Positive-cover admission; historical hosted run reported 73 VERIFIED rows and 29,346 complete workspace test subjects; the candidate now includes the missing provenance-fixture policy entry | Retain exact historical source identities and qualify the repaired integration; registered/advisory cover policy belongs to #14472/#14473 |
| #14472 / PR #14478 | Native-tested offline document auditor; exit zero means consistent documents and always retains native assurance as not assessed | Reuse its DTO/admission owner in the native producer; source/configuration/custody must be measured, not declared into existence |
| #10686 / draft PR #11517 | Large context/source/cache/public-inventory candidate | Keep owner/domain semantics out; split producer/context and projections if needed; qualify current head instead of recreating work |

## Newly split infrastructure leaves

### #14472 — machine native Kani results

Produces versioned JSON plus Markdown from one DTO, with exact run, process,
inventory, cover, currentness, and raw-artifact identity.

It consumes the delivered runner-integrity fixes and composes with #12581 and
#14469. It does not add a proof, semantic relation, or required check.

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

### #14474 — affected critical selection

Plans and executes an advisory `kani-critical` route from registered semantic
claims. Unknown or partial impact falls back to all critical claims, never no
work.

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

The pilot at `30ec9ac187025985314858ff9ba9d72bf1ad02e9` has a
[positive native TLC observation and three expected counterexamples](https://github.com/EffortlessMetrics/ripr-swarm/issues/12109#issuecomment-6813436649).
It still requires Rust adapter compilation/tests, typed raw-artifact custody,
state/action-to-Rust mapping, six-case bounded conformance, refusal controls
and current-main integration. Retain the one-attempt/two-finisher finite scope
and abstract atomic-commit assumption.

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
