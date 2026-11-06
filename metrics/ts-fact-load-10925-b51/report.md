# TypeScript fact-load finding: no product patch recommended

The repeated checksum serialization is real, but neither tested repair demonstrates the requested efficiency improvement without a repeatable RSS increase. Leave the central fact-loader implementation unchanged. Both source variants are archived experiments, **not selected integration patches**. The earlier owner-call reorder is a separate, already-composed result.

On frozen b51, instrumented warm loading took 98.13 ms for zod: read 17.51, typed JSON decode 50.37, checksum reserialization 29.20, reshare 0.27. The useful 2680-test workload took 16.38 ms: read 1.41, decode 9.24, reserialization 5.02, reshare 0.25. This load component decodes cached JSON rather than TypeScript source. The old checksum already streams through an 8 KiB buffer; it repeats graph traversal/scalar encoding, shared-ID bookkeeping and scratch allocation. All 501 retained zod producer bodies independently matched the existing checksum when hashed directly.

The smaller candidate c7e123521 changes only TypeScript fact admission and its changelog. It recovers the unsigned compact body inside the owned read buffer and uses the existing byte-digest helper. Schema 6, writer, key/currentness checks, shared decoding, dependency refusal and header inventory stay unchanged. A failed byte shortcut falls back to the original semantic verifier, including pretty/reordered/whitespace-varied input.

Fresh same-host release comparisons below use median of 3 independent trials; arrows are b51 → c7e123521. RSS is the median of each process's peak. Full ranges and every cold/warm/edit state are in [summary.json](byte-admission/summary.json).

| State | Wall ms | Child CPU ms | Peak RSS MiB |
| --- | ---: | ---: | ---: |
| Small control, first warm | 31.03 → 31.25 | 23.65 → 25.83 | 26.05 → 28.79 |
| 2680, application-cold | 138.18 → 130.27 | 119.28 → 117.68 | 44.67 → 46.61 |
| 2680, first warm | 82.00 → 76.75 | 74.85 → 67.58 | 35.84 → 39.19 |
| 2680, unchanged warm | 76.89 → 76.80 | 71.31 → 69.40 | 35.83 → 38.74 |
| Production edit, warm | 81.85 → 71.74 | 73.81 → 69.69 | 37.16 → 38.92 |
| Assertion edit, warm | 81.87 → 81.85 | 73.83 → 72.41 | 37.87 → 39.50 |
| Zod negative control, warm | 412.84 → 408.11 | 362.18 → 352.30 | 66.61 → 68.84 |

All 150 bounded CLI invocations completed, exit 0, with nonempty findings and no timeout: 25.89 s summed child wall, 20.11 s child CPU, 68.86 MiB maximum peak. Both real edit kinds started clean for each trial and matched independent cold same-state output; 45 corresponding states also matched across binaries byte-for-byte, including origins. Useful states retain two related-test findings: one static_unknown and one weakly_exposed; the assertion edit makes the public finding exposed. Alias redirection's two no_static_path findings and zod's six no_static_path findings are explicit negative controls, with no useful-reach speed claim. Corruption and a valid foreign-build body at the current filename each recompute correctly; repaired cache bytes and complete outputs match clean state.

The nonempty baseline work witness fails 1 versus 0 reserializations after successfully serving facts; the current witness passes 0. Focused tests 22/22 and TypeScript tests 2344/2344 pass on exact c7e123521. Independent source review found no blocking correctness defect. Strict Clippy still reports only three untouched, exact-base-confirmed cmp_owned errors; native fragment validation reports the same inherited 12010 missing-reference defect on base and candidate. No aggregate or central qualification pass is claimed.

The earlier framing candidate 9c64ecada also passed semantics, but had mixed useful edit-loop gains and about 3–4 MiB more file-backed RSS in its diagnostic controls. Its complete 160-run dataset (including 12 attribution runs) and 18 completed memory diagnostics remain separate. The smaller candidate still increases useful-control RSS by about 2–3.5 MiB; its increase is not separately attributed. No third variant was attempted.

Limits: 4-vCPU/16 GiB AMD host, N=3, 5 ms wall polling, application-cold rather than OS-page-cache-cold, and block-ordered edit timing. These are scoped observations, not a universal speed or installed-artifact claim. Exact identities, commands and raw archives are in [reproduce.md](reproduce.md). Nothing is ported to the newer central semantic candidate.

Next action: central integration owner 01a1a959-f983-763a-96e6-7dbe1cced019 should disposition both experiments as unselected and retain its loader. Any selected continuation of decoder/footprint work belongs with this existing TypeScript performance owner on a newly supplied settled source. No new release gate, denominator, public #1889 draft/DO NOT MERGE or #1893 trust-hold change is proposed.
