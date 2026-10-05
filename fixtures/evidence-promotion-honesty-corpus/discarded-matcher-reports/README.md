# Discarded matcher canonical reports

These fourteen raw JSON/human/full outputs were copied byte for byte from the
checked-in canonical roots at fd2f11dcf7204fdccf6a15919590b8f820b2b4c7,
[run37251340685](https://github.com/EffortlessMetrics/ripr-swarm/actions/runs/37251340685),
job111579495938, attempt1. No captured JSON was normalized or synthesized.
Each source/diff pair was independently checked against the canonical input.
The independent contract is score(1)==2: original returns2, wrong returns3.

The fourteen per-case CLI assertions completed before receipt emission. The
aggregate test then failed because its invocation directory had not been
created before cleanup. b5af748 repairs that lifecycle error in source;
current-head execution and canonical negative-control acceptance remain
separate requirements. This failure is not a new production behavioral RED.

Producer binary SHA256:
2ca25d0aea67a83289e598cedd422289a4e4dc50c3bec9cdfe2dd0224f9640e7.
Nextest artifact11322060680, ZIP313384 bytes, SHA256:
ed2af0e59f509ceec3a3e89b7ab89422fee5bb20883a62d281aef4eb7edc6f3b.
producer-receipt.json retains original bytes (9939), SHA256:
b42c773c0e07d38a8c1aa1637bc79b225bf15a532f4cb81162a4e0e88c69cb2d.
Its seventy input/diff/output hashes were checked. Its absolute binary path
is producer custody, while report root/probe paths are relative canonical paths.

The existing corpus.json registers all14 via typed source_report assertions:
eight discarded subjects require reachable_unrevealed/unknown/none; two
asserting wildcards remain weak; four exact/guarded asserting wrappers remain
exposed/strong. Every case requires one finding and the intended consumer.
The shared honesty evaluator and required manifest validator are challenged
with synthetic dishonest strong and weak credit, missed strong credit, empty
findings and removed consumers. Synthetic negative reports are temporary
controls and never replace these canonical producer bytes.

These are authored static controls, not compiled fixture-test execution,
actual mutation outcomes, a representative population or an accuracy estimate.
