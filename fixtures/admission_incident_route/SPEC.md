# Admission incident continuation route corpus

Issue: #5076

## Given

Agent root and PR continuation routes must consume a current
`admission_incident.v1` packet (#5075 output) and select one bounded repair,
retry, handoff, or visible blocker plan. This directory pins positive and
negative routing expectations without compiling incidents or mutating live
settings.

## When

`cargo xtask check-admission-incident-route` loads `corpus.json` and requires
each scenario's routed plan to match the committed expectation exactly.
`cargo xtask check-fixture-contracts` requires this SPEC and corpus layout.

## Claim boundary

Routing-only: does not satisfy #5075 packet compilation or full skill/runbook
parity named in #5076 verification.
