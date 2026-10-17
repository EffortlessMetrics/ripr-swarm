# Unrelated return-value snapshot audit

Analyzed source: `a1ea790e923ae6036efdd9457ffc16ea2ad93348`; CLI SHA-256 `bd9348e3ecd9eeafcca1df3c04fc84f8bb56c490538e997b0139cdd8410f0d5e`; Rust-only compiler feature set. Canonical owner: [#3161](https://github.com/EffortlessMetrics/ripr-swarm/issues/3161), snapshot parent #6642, residual #8121. Merged #8251 is untouched.

The changed `make_count(2)` returns 3. A committed snapshot of unrelated `other = 2` still passes, but public repository grading gives the owner return all five stages yes, strongly_gripped, already_observed, and the canonical no-action recommendation. Removing only that unrelated snapshot assertion makes discrimination no. Direct snapshot and independent literal controls reject the same changed return.

| Case | Native pass / fail | Repo grip | Discrimination |
| --- | --- | --- | --- |
| direct-0 | 1 / 0 | strongly_gripped | yes |
| direct-1 | 0 / 1 | strongly_gripped | yes |
| unrelated-0 | 1 / 0 | strongly_gripped | yes |
| unrelated-1 | 1 / 0 | strongly_gripped | yes |
| none-0 | 1 / 0 | reachable_unrevealed | no |
| none-1 | 1 / 0 | reachable_unrevealed | no |
| literal-0 | 1 / 0 | strongly_gripped | yes |
| literal-1 | 0 / 1 | strongly_gripped | yes |
| unrelated-removed-1 | 1 / 0 | reachable_unrevealed | no |

Nine native tests: seven pass, two deliberately fail their real value assertions; zero ignored. Every fixture has its own target and a compiler-artifact receipt naming its exact source path. All committed expected snapshot hashes and source hashes stay unchanged during execution. Insta updating is disabled (`CI=true INSTA_UPDATE=no`); all nine lockfiles are byte-identical. The direct failure is a snapshot value mismatch 2 to 3; the literal failure reports left 3 / right 2.

The direct and unrelated changed-value diff findings both remain weakly_exposed. This is a repository-seam grading and repair-guidance finding, not a false exposed-diff claim. Other diff rows are retained separately in summary.json.

## Minimal subject

```rust
pub fn make_count(count: usize) -> usize {
    count + 1
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn snapshot_count() {
        let actual = make_count(2);
        let other = 2usize;
        insta::assert_json_snapshot!(other);
    }
}
```

The committed expected snapshot is `src/snapshots/snapshot_return_audit__tests__snapshot_count.snap`, with expression `other` and expected payload `2`. The positive direct case snapshots `actual`; the independent literal case asserts `assert_eq!(actual, 2)`. The baseline changes only `count + 1` to `count + 0`.

## Reproduce

Extract packet.tar.gz into an owned directory. Its fixture paths retain source, Cargo.toml, locked dependencies, expected bytes and exact baseline-to-mutant diffs. Use the pinned Rust/Cargo 1.95.0. Each receipt retains the actual original command and environment; adapt task-local absolute paths when relocating.

```bash
CI=true INSTA_UPDATE=no CARGO_BUILD_JOBS=1 cargo test --locked --lib --manifest-path fixtures/unrelated-1/Cargo.toml -- --test-threads=1
/absolute/reviewed/ripr check --root fixtures/unrelated-1 --format repo-exposure-json
/absolute/reviewed/ripr check --root fixtures/unrelated-removed-1 --format repo-exposure-json
```

Rebuild the analyzer from the exact source only if the retained local executable is unavailable: `cargo build --locked -p ripr --bin ripr --no-default-features --features lang-rust`. A new executable must be independently identified; this packet does not distribute a binary or qualify installation.

## Scope and next action

No active same-claim writer was found in issue comments and all-state/open PR searches. Preserve #8004 inherent-receiver attribution, #7900 tautological snapshots and #7135 collection observers. The bounded next repair is exact return-value-to-snapshot admission through the existing shared evidence owner, with these positive and single-assertion-removal controls. Avoid blanket snapshot promotion or loss of the positive direct/literal oracle. No competing #8251 patch or new tracker was created.

Freshly fetched main is `60a22c66851e8f5f6d932361c8e6cd758b9efd48`; its explain/context selector delta leaves these oracle, seam-grading, manifest/lock and renderer owners unchanged. This later main was not rebuilt or executed.

The initial build command failed because `/usr/bin/time` was absent before Cargo started; the corrected unchanged-source build succeeded in 6m59s. The separate discovery read `ls /root/.cargo/bin` was denied (`Permission denied`) and was not retried. The verified public pinned toolchain is task-local. Historical artifact/protection denials were untouched.

Resource admission: one build job, 600-second analyzer deadline, 12 GiB per-process virtual-memory cap, 14 GB peak additional-disk budget and 6 GB reserve, against measured 32.0 GB initial free disk and 16 GiB cgroup memory. Native commands are serial and individually bounded; original and negative-control evidence is retained. No hosted CI, default-feature/full-workspace, Windows, package, install or release claim.

All task-owned targets, fixture repositories, toolchain and raw evidence remain retained locally for parent review; the original checkout and audit source worktree are unchanged. No original/unknown resource was removed.
