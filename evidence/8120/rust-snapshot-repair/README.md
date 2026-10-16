# Rust snapshot field admission repair evidence

Source candidate: [PR #8251](https://github.com/EffortlessMetrics/ripr-swarm/pull/8251), head `014238d1256fa283fece5679d520e511bcd460d0`, tree `4e6847a9336fa1488884a886b2b7c1882ec5fa30`, base `0b8dad52d85dc7271e4ddfeef8b6d375dc02784c`.

This bounded guard keeps repository field-construction discrimination weak for unsupported snapshot observations. A same-named field on another receiver, an absent/stale expected snapshot, or a configured raw snapshot strength cannot independently clear the owner field as already observed. The existing literal and expect-test equality admission remains intact. Correct direct-owner snapshots also remain conservative until exact binding is implemented; #8120/#8121 and their broader owners remain open.

## Executed evidence

- Baseline doctor, selector-verified check-fast and ripr all-target compile passed at the exact base before mutation.
- Pre-repair integration execution compiled successfully and ran six tests: four intended weak-grade failures, two positive controls passed. Original test source hash, native exit 101 and frozen executable identity are retained. The final literal positive was strengthened later; whole-file red-source equivalence is not claimed.
- At committed `8f5f0de`, all six integration controls and the changed full/compact unit passed. Literal and expect-test compatibility units passed individually. Corrected `014238d` changes only the two corpus reports; production/test/spec/manifest bytes are unchanged. These earlier executions retain their actual head identity.
- At clean `014238d`, the independent reviewer ran 15 rendered CLI controls successfully, then independently rendered two unchanged inputs through frozen base and head binaries: base strong/yes/already-observed versus head weak/weak/static-limitation. Source hashes and nonempty field selectors are retained.
- Native literal challenge: the unrelated snapshot stayed valid; baseline 1/1 passed, changing only the owner value 2 → 3 failed on its separate literal equality, restoration 1/1 passed. It used an exclusive target and pinned/locked Insta 1.49.0 and expect-test 1.5.1.
- The existing honesty gate rejected both deliberately false strong field grades. Exact original report bytes were restored and the gate passed.
- Corrected-head check-fast ran 15 gates over 11 independently resolved changed paths, including strict workspace/all-target Clippy, no-panic, file policy and relevant process/network policy. All passed.
- Prepared goldens passed 454 fixtures / 1,131 surfaces, zero drift. Dogfood passed. Precommit and final JSON CLI build returned native 0.

## Retained failure and exact preparation attribution

The original golden command exited 1 only on `owner_return_pin_fallible_literal` (expected exposed, actual weakly exposed). Isolated clean exact-base and repaired frozen CLIs both showed the same weak result. Fetching fixture-locked anyhow 1.0.100 into the same task Cargo home made both same binaries render exposed. All 10 fixture files stayed byte-identical; archive SHA256 matches the locked `a23eb6b1614318a8071c9b2521f36b424b2c83db5eb3a0fead4a6c0809af6e61`. No golden or source was changed. This known preparation condition belongs to merged [#8174](https://github.com/EffortlessMetrics/ripr-swarm/pull/8174); the actual GitHub merge subject already includes its readiness hook. The original native 1 is retained separately from the justified prepared rerun 0.

The first candidate review also rejected a corpus selector that matched an enclosing constructor return as well as the intended field. Actual regenerated fixture inputs return a local record binding; each existing substring selector now selects exactly one field, while every actual report seam remains present. Old candidate `8f5f0de` remains in PR history.

## Identity and limits

Final CLI: `ripr 0.11.0 (014238d1256fa283fece5679d520e511bcd460d0)`, SHA256 `22ef862f765cfe9588a96d1979dc968e702f2a62c1e758784e4934806862b6f7`. The final Cargo compiler-artifact message identifies default Rust/TypeScript/Python features and the executable actually consumed by independent controls. The frozen reviewed copy is byte-identical. Exact executable witnesses remain local; this archive contains hashes and receipts, not binaries or installers.

Pinned Rust/Cargo 1.95.0; Linux x86_64; task-owned Cargo home/targets, one Cargo job, one test thread and one Rayon worker. Cargo compile jobs and CLI fixture worker pools were bounded separately. The base investigation worktree was removed after terminal execution and verified clean state; source candidate, warm target and exact witnesses remain for the open PR's parent review/CI repair. No background watcher remains.

A reported 08:12 transport callback did not terminate the original validation. The original workspace Clippy returned native 0; OOM counters remained zero. Transport availability, source validity, build identity and runtime behavior are distinct evidence dimensions.

No full workspace test, non-Linux platform, package/install, release, full snapshot architecture, hosted required-result or merge claim is made. Live PR metadata is authoritative for later delivery state.

## Inspect and replay

`summary.json` binds commands, subjects, outcomes and limitations. `archive-manifest.json` lists every member's hash, bytes and mode. `SHA256SUMS` checks the published packet files. Decode the Base64 file, verify its gzip/tar hashes against the manifest, then extract:

```bash
base64 --decode repair-evidence.tar.gz.b64.txt > repair-evidence.tar.gz
tar -xzf repair-evidence.tar.gz
```

The archive retains raw native stdout/stderr/exits, compiler-artifact messages, exact fixture inputs and locks, reviewer controls, source patch and the initial failure. Local driver paths are locators recorded at execution time; adapt task roots when replaying. Public source/test bytes are pinned to the candidate commit above. Snapshot presence or a passing test is never treated as proof of owner-field relevance.
