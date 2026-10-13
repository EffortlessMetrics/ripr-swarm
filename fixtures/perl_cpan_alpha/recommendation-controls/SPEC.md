# Perl Direct-Test Recommendation Controls

Spec: RIPR-SPEC-0235

## Given

A newly authored Checkout module and separate Test::More and Test2::V0 subtests
pin literal line-item totals. A package/capability test uses `can_ok` without
calling the changed subroutine. Direct `ok` controls observe only truthiness.
All fixture implementation and test source is original under MIT OR Apache-2.0.

`packets/` contains hand-authored v1 consumer packets, based on the existing
EXACT_RETURN_PACKET schema fixture. Their producer name is a compatible wire
label, with an explicitly authored version; no exporter generated them.
On-disk file digests and the SPEC-0064 identity fingerprint are checked at
consumer ingestion. The relation IDs deliberately place the package-reference
row before the direct row even in canonical order. Sink relevance is absent,
so exact runtime assertions do not promote the static finding to `exposed`.

Pattern provenance (no upstream implementation copied):

- Test::More named subtests: [test-more examples/subtest.t](https://github.com/Test-More/test-more/blob/5d1077b4161f30229f5a77f35d5cba5c9b1b6f55/examples/subtest.t).
  Class capability checks: [t/Legacy/More.t](https://github.com/Test-More/test-more/blob/5d1077b4161f30229f5a77f35d5cba5c9b1b6f55/t/Legacy/More.t).
  [Upstream license](https://github.com/Test-More/test-more/blob/5d1077b4161f30229f5a77f35d5cba5c9b1b6f55/LICENSE)
  is Perl 5 dual terms, GPL version 1 or later / Artistic; Chad Granum, 2026.
- Test2::V0 assertions and subtests: [Test2-Suite t/modules/V0.t](https://github.com/Test-More/Test2-Suite/blob/cb58be1c7c91d1cb3599e998e82ce525183b2d7f/t/modules/V0.t).
  [Upstream license](https://github.com/Test-More/Test2-Suite/blob/cb58be1c7c91d1cb3599e998e82ce525183b2d7f/LICENSE)
  is the same dual terms; Chad Granum, 2024.

## When

Consumer tests run the real check pipeline and render JSON from these packets,
including array permutations. Separate manual `prove -lv t/<test>.t` runs use
`input/` unchanged, then an owned copy whose multiplication is replaced by
addition. `runtime-receipt.json` and `runtime-logs/` retain the actual native
statuses, diagnostics, source hashes and versions from 2026-10-13.
Readable `.log` views trim trailing line whitespace. `runtime-raw-logs.json`
retains each exact original UTF-8 log and its SHA-256; the receipt binds both
views. Authored source/provenance ranges are bounded to the fixture files.
The compact diff hunk keeps the same changed line and updates `input.diff_id`;
neither provenance ranges nor that diff ID participates in the current
identity fingerprint. No fingerprint/authenticity claim is added.

## Then

The selected recommendation and verify command identify the same direct,
reachable test for both frameworks, regardless of relation-array ordering.
A missing selected-test command stays missing. Advisory-only relations cannot
supply a direct-test recommendation. Admitted exact evidence wins over broad
or smoke evidence; stable test and relation IDs break equal-strength ties.
Multiple commands for one selected test exclude empty argv before stable
command-ID ordering; an empty-only selected test still cannot borrow another
test's command. Strict actionability retains its existing validation and the
usable command's evidence reference. Canonical gap and probe shapes come from
the selected direct row's test/owner-bound oracle, defaulting otherwise.
Mixed advisory/direct oracle permutations preserve the direct exact gap ID;
foreign test/owner shapes are refused, while a bound alternate shape is kept.
Downstream witnesses, gap projections and preview evidence resolve the complete
declared file/test-name to one retained oracle-related row. They do not rerank
equal-strength rows; missing, unknown or ambiguous identities yield no target.
Only file separators normalize; names preserve backslashes, `::` and whitespace.
Synthetic composition controls cover these consumers under permutations and
advisory/direct combinations, without establishing shared-validator
qualification (their inherited raw references and edit boundaries are separate).
Complete and partial packet guidance remains advisory; partial packets still
have no canonical gap. Exposure classes and displayed oracle strengths do not
rise as a result of selection.

The actual runner matrix is independently discriminating:

| Test | Correct multiplication | Defective addition |
|---|---|---|
| Package capability | exit 0 | exit 0 |
| Test::More exact subtest | exit 0 | exit 1: 128 != 375; 125 != 0 |
| Test2::V0 exact subtest | exit 0 | exit 1: 128 != 375; 125 != 0 |
| Test::More direct truthiness | exit 0 | exit 0 |
| Test2::V0 direct truthiness | exit 0 | exit 0 |

## Must Not

- Treat these authored packets as live producer/static Perl extraction proof.
- Treat fixture runner outcomes as RIPR runtime outcomes or broad Perl accuracy.
- Infer discrimination merely from a direct relation or truthiness assertion.
- Change ranking-corpus baselines, waivers, support tier, or gate authority.
- Execute Perl, a producer or network access during the consumer tests.
