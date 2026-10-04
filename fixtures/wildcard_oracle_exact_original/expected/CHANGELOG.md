# Golden Output Changes

## Pending — wildcard_oracle_exact_original (19e43c73)

Reason:
Independent score(1)==2 original/wrong controls pin whole-wildcard
non-promotion and retained exact/guarded discrimination. Actual source-bound
fast-mode CLI output retains one finding and one related test; no runtime
execution or representative-project accuracy claim is made by the analyzer.

Command:
`ripr check --root fixtures/wildcard_oracle_exact_original/input --diff fixtures/wildcard_oracle_exact_original/diff.patch --mode fast --json`
Repeated directly for human and human-full output with distinct cold caches;
CLI binary SHA2563690bf6f1a6cd80e6bc7d9f1fb97b7c6a5686ae2da09ffb1daf2263c77bc927d.

Updated:
- `expected/check.json`
- `expected/human.txt`
- `expected/human-full.txt`
