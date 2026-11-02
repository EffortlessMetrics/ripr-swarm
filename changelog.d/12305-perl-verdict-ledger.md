<!-- section: Internal / CI -->
- `verdict-corpus` now requires `fixtures/perl-verdict-corpus/spec-coverage.toml` like the other verdict corpora; a Perl corpus without that ledger fails validation instead of scoring rows without the spec-example coverage gate (#8775).
