# Frozen c4 veto reconciliation

The decisive missing prerequisite for target67654dc6 is owner-return PrimitiveScalarIdentity across the actual crate/import/expansion scope. StoredAny is the first adapter guard; budget.rs `use crate::*` independently refuses owner primitive identity, before expectedMAX. [ACCEPTANCE-CONTRACT.md](ACCEPTANCE-CONTRACT.md) gives the exact owner/resume condition. No source change or repair/consumer clearance is claimed.

All12 controls use exact header `probe:src_proof.rs:match_arm:a83f5558` with one nonempty header per CLI run. AllCLI exits0. All24 native compilations succeed;12 original tests pass;9 zero-return variants fail101;3 deliberately weak tests still pass0. Source and native logs are retained.

| Control | c4 header | Native original/zero |
|---|---|---|
| closed-positive | exposed | 0/101 |
| stored-any: unqualified root proof::* | weak | 0/101 |
| qualified-root: crate::proof::* | exposed | 0/101 |
| owner-crate-glob: owner use crate::* | weak | 0/101 |
| isolated-expected-max: test-file use crate::* and u64::MAX | weak | 0/101 |
| isolated-expected-numeric: same test, numeric MAX | exposed | 0/101 |
| return-alias: Word = u64 | weak | 0/101 |
| overwritten-actual | weak | 0/0 |
| self-expected | weak | 0/0 |
| custom-glob-primitive: imported custom u64/always-true equality | weak | 0/0 |
| expected-glob-max: nested-test variant | weak | 0/101 |
| expected-glob-numeric: nested-test variant | weak | 0/101 |

The two nested variants are confounded refusal controls; their exact refusal stage is not established here. They do not isolate expectedMAX or establish that the owner-root namespace reader scans nested imports. Only the separate-file isolated pair establishes the expected-operand distinction. `module_use_leaves` reads direct children. Owner-crate-glob has a direct owner-root import and independently exercises that primitive refusal.

The candidate census is Names(empty) for owner-crate-glob, custom-glob-primitive and both isolated expected cases. The stored-any root is Any. Therefore Names(empty) does not prove primitive identity, and there are independently blocked positive and negative cases after removing Any. The valid imported custom u64 is defined in another file and implements always-true PartialEq<i32>; changing both returned values to its zero constructor passes. It must never be promoted on the basis of spelling/no macro names.

## Compact reproducible failure

Use an already built frozen c4 CLI; verify version and SHA256913bc60fcd8591c13b019c630f67a79882e92dd34cf02f9dedf26642b3fa68fe. After placing this directory locally:

```bash
VETO_PACKET=/absolute/path/to/veto-reconciliation
VETO_RIPR=/absolute/path/to/frozen-c4-ripr
RIPR_CACHE_DIR=/tmp/ripr-veto-owner-check "$VETO_RIPR" check \
  --root "$VETO_PACKET/cases/owner-crate-glob" \
  --diff "$VETO_PACKET/cases/owner-crate-glob/input.diff" --mode ready --format json
python3 "$VETO_PACKET/future_contract_red.py"
```

The CLI returns0 with one whole-match header at weakly_exposed. The retained future-contract assertion returns1 after checking nonempty header setup; it expects the future closed primitive-identity positive. This is an acceptance RED for missing authority, not a current regression or unsoundness claim. To test fresh output, write it to the case check.json before running the assertion. Native sources under `cases/owner-crate-glob/native/{original,zero}/src` compile with `rustc --edition=2024 --test .../lib.rs`; original test exits0, zero exits101. Other native cases have the same layout. Temporary output binaries may be placed outside this evidence directory.

## Exact consumer validation and custody

c4 and its original independent review/evidence4cafbc52 are frozen. The existing exact native0 consumer replay is reused because producer, binary, consumer head/tree/config and diff are unchanged. Fresh custody reconciliation compares all107 retained file-fact source strings against current files with zero differences, verifies clean consumer/head, and verifies binary/diff/raw-output hashes. Five Any files are enumerated. Raw outputSHAe67563d1ec9aad34b8c54d5f341a13114b93469c73bb6b97f55f6ccbe66db96b still equals frozen656: all52 finding objects/top fields unchanged,0changed/introduced/removed,targetweak,currentweak18. No duplicate expensive consumer analysis or predecessor validation was run.

`consumer-custody-empty-attempt.json` records an initial custody-reader setup mistake (read envelope instead of file_facts, zero subjects). It establishes no validation. Corrected reader asserts nonempty107sources+Any census and zero differences; its receipt supersedes the empty attempt for custody only. Prior exact consumer command is terminal0, not rerun or relabeled. No product build/source edit; native tinycontrols used Rust1.95. Frozen artifacts retained, no cleanup.
