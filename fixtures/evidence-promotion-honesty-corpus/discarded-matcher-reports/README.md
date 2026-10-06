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
producer-receipt.portable.json is an explicitly labeled portable projection.
It omits only the machine-specific binary path, retaining the binary digest
and all seventy original input/diff/output hashes and records. It has different
bytes from the original receipt and must not be identified as the original.
The authentic raw receipt (9939 bytes), SHA256
b42c773c0e07d38a8c1aa1637bc79b225bf15a532f4cb81162a4e0e88c69cb2d,
remains in the official artifact, task evidence and commit666b097. The projection
records its exact source/member custody. Report root/probe paths are relative.

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

## 2026-10-05 refresh — intentional renderer drift (#5996, PR #6823)

The fourteen reports were regenerated from the current producer at merge
commit f2580e186 (branch fix/cross-surface-agentic-r3) because #5996 changes
the renderer's finding-location form on purpose: locations render through the
shared workspace-relative owner `analysis::finding_location_text`, so
`probe.file` reads `./src/lib.rs` instead of the fixture-prefix join. This is
the same intentional flip already re-blessed across 176 ordinary goldens
under RIPR-SPEC-0002. Classifications, counts, probe families, oracle
kinds/strengths and consumers are unchanged; the independent contract
(score(1) == 2; eight discarded subjects reachable_unrevealed/unknown/none,
two asserting wildcards weak, four exact/guarded asserting wrappers
exposed/strong) was re-verified per case.

Provenance: local build of the exact merge tree, `ripr --version` ->
`ripr 0.11.0 (f2580e1861ebf778f6d96d49ce80aede9e1cc3a3)`, ripr.exe SHA256
`723F31ACB987526268A98F9DE40B5EF6AA2B23F2612F67B9FA9D3974B2C250E4`, captured
2026-10-05 by rerunning the fourteen source/diff pairs through the same
commands the CLI-control test executes. Custody for a behavior-change
regeneration is two-step: provisional local bytes plus the mandatory hosted
byte-compare — the routed CI run at the same merge head re-executes this test
against the hosted build and compares the committed bytes bit for bit, so the
hosted run remains the binding check. The original hosted-only capture above
stays the protocol for unchanged-behavior refreshes.
