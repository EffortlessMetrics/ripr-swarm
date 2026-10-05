# Fixture Corpus: perl-verdict-corpus

Spec: RIPR-SPEC-0238

## Given

Three small authored Perl distributions under `subjects/`, our own code under
this repository's license: `authored-perl-testmore-shop` (Test::More),
`authored-perl-test2-ledger` (Test2::V0) and
`authored-perl-testexception-account` (Test::Exception). Each case is a
one-line behavior-preserving rewrite under `cases/` with the fact packet the
pinned `perl-ripr-facts` producer wrote for that diff under `packets/`.
Each case is labeled with what `prove -l t` discriminates when each listed
hand-written mutant of the rewritten line is applied.

Cases named `perl-0235-*` start from another case's producer packet and edit
it to carry a RIPR-SPEC-0235 fact the producer cannot emit yet. Their
`perl_facts.provenance` is `edited_producer` and `perl_facts.edits` names the
edit; their rates are reported apart from producer-packet rates.

## When

`cargo xtask verdict-corpus check --language perl` applies each edit to a
run-owned copy of its subject, runs `ripr check --json --perl-facts
<packet>` with the `lang-perl` build, and projects the anchored findings to
one verdict. Perl findings carry `unresolved_subject` currentness (#3280),
so this corpus counts them.

## Then

Each case scores as ideal, abstained, false actionable, false exposed, or
false silent against its label; contradictions inside ripr's own output are
counted; producer and edited-packet rates are reported apart; and the
report must equal `expected/report.json`.

## Must Not

- Run Perl, `prove`, the fact producer, mutation testing, or network access.
- Treat the rates as a population estimate.
- Edit a subject file or a packet; a changed byte fails its sha256.

## Refreshing

When a ripr change moves a verdict, `check` fails and names the first
differing line. Read `target/ripr/reports/verdict-corpus/perl/report.md`.
A row marked `changed_since_labeling` must be re-checked against its
labeling notes before the expected report is refreshed with
`cargo xtask verdict-corpus report --language perl --out fixtures/perl-verdict-corpus/expected`.
A new producer pin regenerates every producer packet and relabels each case
whose packet bytes moved.
