<!-- section: Added -->
- Perl verdict corpus: 36 runtime-labeled cases across Test::More, Test2::V0
  and Test::Exception in `fixtures/perl-verdict-corpus`, each with the fact
  packet the pinned `perl-ripr-facts` producer wrote, scored by
  `cargo xtask verdict-corpus check --language perl`. Cases built from
  RIPR-SPEC-0235 packet shapes the producer does not emit yet are marked
  `edited_producer` and reported apart. Perl false-verdict, false-actionable,
  false-exposed, false-silent and abstention rates are on the trust
  scoreboard (RIPR-SPEC-0238).
