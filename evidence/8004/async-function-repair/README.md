# #8004 async-function helper repair — final Draft handoff

Source: **a5c69b1cd2ef73d47a11163bca0432f9fe285c3d**, tree **cc498e204e08facb92cab0d1dad36b36bddd428d**; normal push on `rook/7415-private-target-attribution`. Open/Draft verified. **CANDIDATE_REPAIRED / REVIEW_INCOMPLETE**: parent assigns fresh independent exact-head review. No Ready/merge or thread-resolution action.

The helper traversal now requires an identified synchronous declaration at every entry/intermediate/last hop. It reuses the complete-source async declaration query; indexed body slices start at `fn`, so they omit `async` and cannot supply this authority. Unknown/ambiguous declarations fail closed for helper execution; existing owner-return-pin fallback behavior is preserved. Genuine synchronous/mixed calls and complete receiver/module/type/macro/helper identity controls remain. No general polling, type/value flow, Cargo-target resolver or macro expansion is added. #8005 remains substantive and unbenchmarked.

## Exact subjects

Executed frozen-main integration: main98c8f172bfd6aa2b808f63431ed4eac9c224c102 + originala2b489b9d7d5b78daebf9fc1b3e4201dae3c9524 -> composition6b38d4e74623743d851f5dc7756725662a250e2b, tree834b90117415f3afc54753576fcf83ae39d727a3. Complete keyed297/316/298 ->317 rows, no divergent same-ID edits, all metadata preserved. Eight additions ->325 without modifying any existing complete row. Authored repair commits86d038ef62e5232bc15bfe0e4ff3da22cd0f5257 and a5c69b1cd2ef73d47a11163bca0432f9fe285c3d preserve history.

Later main758a7c082c1c140d1d818bb3d7e5be78643d3f22 is one unrelated TypeScript __proto__ fix in three files. GitHub's observed merge3f795f5d951d68afd1119f8760308b79dee48e81, tree011a651890b49e58df3c5a3af631dd527a179d85, has actual parents758a7c08/a5c69b1c and differs from tested a5 only in those three files. That later merge tree was **not executed**; no endless behind-only restack.

The committed frozen CLI is `/workspace/8004-repair/candidate-ripr`, SHA256097e6832231684cb191b9f2088b6af4abe1f47e3397b2722dfd10f720db1803a; its actual Cargo artifact message and clean build commit are in `committed-cli-identity.json`. The golden producer has identical bytes. Local receipts are not hosted required-check, Windows, full-workspace runtime or release qualification.

## Observed proof

-50 cases: all original34 complete rows/native truth unchanged,16 additions.150 full/fresh-compact/diff reports pass with complete named subjects; three same-line reports and three formatting-probe reports also pass. Full/compact metrics agree. Native fixture digests match the production inputs:50 originals pass,22 mutants fail,28 survive (100 matrix executions); formatting probe adds original0/mutant101. Runtime and bounded static admission remain independent.
-927 distinct selected library units and94 selected integrations pass across12 command groups:433 grip,290 related and228 owner-pin selections (overlap deduplicated); receiver9, assertion-helper23, wrapper33, match-arm helper3, native-root5, rerun16, suppression1 and structural4. Exact argv, head/tree and raw logs are retained.
-Actual committed-head `cargo xtask check-fast` returns0:15 gates, zero skips,170 paths independently equal between raw Git diff/tree inventories and the complete typed policy inventory.
-Actual committed-head `cargo xtask precommit` returns0. Typed precommit status pass, complete change set, zero skipped/limitations. Explicit selected Clippy for ripr and xtask passes. The original tool handle was lost during reconnection, but each direct subprocess return is retained; original gate driver stopped at the subsequent golden setup failure rather than being mislabeled all-green.
-All465 selected/actual/compared golden fixtures pass with zero drift/errors after fetching the exact locked anyhow1.0.100 fixture dependency. Same source, producer bytes and existing expected files; no old re-bless. Only the new async-function fixture snapshots were initialized.
-325-case honesty corpus and nonempty advisory dogfood pass (two basic scenarios, three findings, plus retained product-flow controls).
-Rust485, Python69 and TypeScript169 verdict corpora match their expected observations. Accuracy costs remain visible: Rust20/485 false verdicts; TypeScript5/169; Python0/69. The five prior false_actionable case/observation blobs are independently unchanged (`truth-retention.json`). No authored verdict-case/truth/expected-row edits.

## Bounded unsupported synchronous control

`sync-chain.rs` is natively mutation-sensitive but already lacks a candidate route with its sparse names and one-line captured tail-call text. The bare-call direct-delegation recognizer sees the function-header prefix. `named-sync-chain.rs` supplies the existing full-callee-name candidate tokens and retains helper credit. A formatting-only multiline variant of the sparse chain also retains helper/high, compact parity, conservative diff and native original0/mutant101. This is candidate absence before the new async declaration gate, not a rejection of a resolved synchronous hop. `format-probe.tar.gz` and its exact receipts preserve the check. The existing `bridge` bare-call limitation remains unchanged. No native truth was relabeled to make this pass.

## Failures and instrument boundaries retained

Initial behind-main worktree doctor; transient same-executor transport disconnect before batch admission; newly staged sparse-chain expectation correction with native truth unchanged; ineffective indexed-body guard tree8c9aa28ab66caadc24efe3dce6bb98accf5884a8/patch75b62ef457cfba2da7b28b741310cfb51559a86ec1da35ac837eab2883e5573f (24 false owner rows remained); overly strict new corpus-class initializer and outer-batch continuation (local86d ancestor), corrected by complete a5 corpus; dirty/clean build stamp byte-equality assertion, followed by a fresh committed executable and rerun; oversized quarantine admission refusal; executor reconnection losing handle16684; `/proc/24151/cwd` read denied and left unread, with permitted argv/receipts confirming the original driver; golden dependency-cache failure and same-head retry after locked fetch. No candidate/source pass is inferred from a failed wrapper or missing exit.

## Remaining and resume

Perl corpus **NOT_ESTABLISHED / NOT_ADMITTED**: available9,029,906,432 bytes cannot fit the unchanged8GiB reserve plus a conservative3GiB fresh lang-perl build allowance. Its command was not run, and it is not reported pass. Resume that qualification on suitable capacity (roughly11GiB free for this allowance), with the same source and corpus bytes. No lower reserve or inherited-target cleanup was used.

Parent's next action is independent exact-a5 review using this packet, then applicable hosted/latest-main/Windows/full-workspace runtime qualification. #8005 remains open/unbenchmarked. Known Rust-only uv fixture repairs stay with #8114/#8141; no duplicate patch. Thirteen returned review threads were snapshotted, including the new documentation clarity note explained above; none was resolved by this attempt. Source worktree is clean and normally pushed; no active source writer or pending validation driver remains.

## Retention and replay

Canonical executed argv/identities are the JSON receipts; historical drivers preserve their original path/stage assumptions and should not be assumed to work unchanged in an arbitrary checkout. `native-controls.tar.gz` retains initial46 and subsequent47 stages, `native-additional.tar.gz` the final three added cases and staged controls. `cli-reports.tar.gz` preserves historical red, ineffective, dirty-correct and clean committed reports; `format-probe.tar.gz` adds the separate format control. `gate-reports.tar.gz` contains complete typed/raw report snapshots; `golden-actuals.tar.gz` contains the actual producer outputs. `final-validation.json` gives the compact acceptance projection.

Three owned-witness archives each passed complete separate restoration, SHA256 and required metadata verification before expanded copies were retired. All201 completed executable witnesses (2,016,368,280 bytes) remain recoverable in local archives identified by `resources/*/{manifest,verification}.json`; current committed executable and all raw sources/logs remain expanded. The archives are retained locally rather than duplicated into this compact Git packet. No permanent purge is authorized. Cargo targets/tool homes are retained and were not cleaned; their ownership is mixed. The8GiB reserve was never lowered; admission and observed free-space receipts are explicit.

Independent reviewer packet b78adbd0ce0dfd5f0baf438497b292133e1c1dc0 was recovered and every entry verified, manifest64ccc9865e405993c126665bb7c4d7766246737739a2f0845b4aaa12e5b5a445. Earlier packet commits3e2e2347f8734116bc4c79f3b7ea724f43936b98 and8400a8d74931b1017b0fc63792cb80e7508643d3 remain immutable in artifact history.
