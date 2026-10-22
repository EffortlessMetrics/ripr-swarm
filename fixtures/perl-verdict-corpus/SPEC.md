# Fixture Corpus: perl-verdict-corpus

Spec: RIPR-SPEC-0245

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

`spec-coverage.toml` lists RIPR-SPEC-0235 as the one in-scope spec and counts
each of its 31 numbered examples once a case cites it in `spec_examples`. A
`perl-0235-*` case derived from another case keeps that case's diff and run
truth, because a packet edit changes which facts ripr reads, not what
`prove -l t` does with the rewrite; its base is named in `perl_facts.edits`.
Where the spec expects a gap, the base is a case whose own tests do not
discriminate the rewrite, so the spec's class and the label agree.

## When

`cargo xtask verdict-corpus check --language perl` applies each edit to a
run-owned copy of its subject, runs `ripr check --json --perl-facts
<packet>` with the `lang-perl` build, and projects the anchored findings to
one verdict. The pinned producer writes zero-based lines (#3221), so ripr
leaves its findings `unresolved_subject` (#6586) and this corpus counts them.
`check-all` runs it with every other corpus.

## Then

Each case scores as ideal, abstained, false actionable, false exposed, or
false silent against its label; contradictions inside ripr's own output are
counted; producer and edited-packet rates are reported apart; and each row
must equal its `expected/rows/<case>.json`.

## Must Not

- Run Perl, `prove`, the fact producer, mutation testing, or network access.
- Treat the rates as a population estimate.
- Edit a subject file or a packet; a changed byte fails its sha256.

## Refreshing

When a ripr change moves a verdict, `check` fails and names each drifted
row. Read `target/ripr/reports/verdict-corpus/perl/report.md`. A row marked
`changed_since_labeling` must be re-checked against its labeling notes
before the rows are refreshed with
`cargo xtask verdict-corpus bless --language perl`.
A new producer pin regenerates every producer packet and relabels each case
whose packet bytes moved.
