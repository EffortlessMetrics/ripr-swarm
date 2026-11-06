# #10925 owner-context repair

**Scoped repair delivered:** `a80be9118d460f1fddeb68413af097f6122208cb`, tree `00d6e77bc71a0e4b75a80bdd10972a0b9b93f25f`, on `perf/10925-owner-context`. Three files change against frozen `b51aebc7e4735529befd677ebf14071c7f09e477` (tree `d3142dd59d4dcfea5e6b4981732f61c8516f3fa0`). This is an isolated handoff to integration owner `01a1a959-f983-763a-96e6-7dbe1cced019`, not a central-candidate update, merge, release qualification or new release gate. Baseline evidence coordinator remains `01a1ae31-b15f-75c8-99e9-9960427fe017`.

## Result and mechanism

On the useful-reach namespace/module-entry workload with 2,680 unrelated tests, first unchanged warm wall time fell **81.9 → 71.7 ms**, CPU **74.8 → 62.6 ms**, and maximum warm RSS **40.0 → 36.1 MiB**. Each cell is three release-profile runs on this same 4-vCPU AMD EPYC Linux host. Outputs are byte-identical. This establishes a bounded improvement on this workload, not fleet performance or the historical Xeon target.

The detailed frozen-source profile puts a median **144.2 ms in 7,467 shadow queries**, within a **167.8 ms owner context**; import-source matching costs about 6.5 ms. These inclusive spans overlap and include instrumentation overhead. Unrelated namespace members and named runner imports such as `expect` build binding indexes before the call/re-export target is rejected.

The repair reorders existing conjuncts in `related_tests.rs::owner_call_relation`: establish the called member and re-export target before requesting binding facts. Every existing local, enclosing-scope, import-identity, mock and fabrication refusal remains. There is no added cache, retained candidate list, field, schema change or ceiling change. #11949's single owner scan and #13300's adjacency work are retained. The accepted ~1.4 ms named-limitations finding stays closed; fact-load integrity is untouched.

## Same-host measurements with useful reach

| State, 2,680 unrelated tests | Wall baseline → candidate (ms) | CPU baseline → candidate (ms) | Max RSS baseline → candidate (KiB) |
| --- | ---: | ---: | ---: |
| Application-cache cold | 127.9 → 122.6 | 114.5 → 109.5 | 48,016 → 48,736 |
| First unchanged warm | 81.9 → 71.7 | 74.8 → 62.6 | 40,952 → 36,964 |
| Second unchanged warm | 81.8 → 66.7 | 73.4 → 59.8 | 40,912 → 37,268 |
| Independent production edit, warm | 76.9 → 66.7 | 74.8 → 61.5 | 40,588 → 37,536 |
| Same production state, independent cold | 132.9 → 117.7 | 119.8 → 104.4 | 48,376 → 48,996 |
| Independent assertion edit, warm | 76.8 → 66.8 | 72.5 → 59.9 | 40,896 → 37,760 |
| Same assertion state, independent cold | 130.7 → 117.4 | 115.0 → 107.0 | 48,028 → 48,848 |

N=3 per cell; wall/CPU are medians, RSS is the maximum of the three. Baseline/candidate order alternates across paired trials. Cold means an empty isolated application cache, not a flushed OS page cache. The harness uses `wait4` resource observations and a 45-second process-tree limit, polling completion every 5 ms. No compiler or second workload ran during measurements. The small cold-RSS increase (~0.7 MiB) is retained rather than claimed away; no memory-for-speed cache was introduced, and N=3 cannot establish a fleet memory bound.

Scaling controls retain exactly two findings and two real related-test rows while increasing unrelated tests 1/64/512/2680. First warm baseline → candidate medians are **31.2 → 31.1**, **31.3 → 31.4**, **41.4 → 36.1**, **81.9 → 71.7 ms**. The small controls show no material gain. Raw ranges, CPU and RSS are in `summary.json`.

## Semantic edit-loop proof and denominator

Each of three independent samples starts from the retained clean Git state, primes its own cache, performs an actual edit, and compares the warm edited result to an independent empty-cache invocation of that exact edited state. Restoring the edit must restore the exact clean output. This is repeated separately for baseline and candidate.

- **Production behavior:** change both `> 100` predicates to `> 101`; the two findings must actually describe `101`.
- **Related assertion:** change `expect(pricing.total(200)).toBe(180)` to `expect(pricing.total(100)).toBe(100)`. The public finding becomes `exposed`; a stale extraction would fail this requirement.
- **Config currentness:** redirect the namespace alias from the owning module to a different module with the same API. Both related paths must disappear as `no_static_path`, match an independent cold result, then return on restoration. These negative states are correctness controls, not speed claims.

The base and production states contain one public `weakly_exposed` finding and one helper `static_unknown` finding with propagation uncertainty. Both have nonempty related tests and reach `yes`; helper-only reach is never promoted to exposure. Assertion edits retain that helper unknown and make the public finding `exposed`. Counts and uncertainty stay identical between binaries. Every field, origin, pointer and semantic identity is compared through **full stdout byte equality**, with no normalization or exclusions: 45 paired-state comparisons plus 36 baseline/candidate edit/config-state comparisons, in addition to independent warm/cold and restoration assertions.

The pinned zod `680d19f7d1e042e55934209caeb79bc843b94f44` / base `40a5dba080b065cc430e5b0e64a8058d68c1660d` remains a negative control: 2 changed files, 23 changed lines, six `no_static_path` findings, no related rows. Its output retains SHA-256 `0560b971b1c6571ebc92b9f0e5843b1ca6453236e67e57a6383c524d1cd163eb`. No useful-reach speedup is claimed from that abstaining population.

All **227 measured invocations** completed with native exit 0, complete nonempty outcomes and no timeout. This includes 45 instrumented profiles and two setup calibrations; those 47 observations are excluded from baseline/candidate speed comparisons. Raw sample names expose the population and state, including unknown/refusal controls.

## Correctness, native proof and remaining qualification

- Frozen product source plus the new work-count test failed as intended: **one shadow index built, expected zero**. The two genuine-call/shadow safety controls passed on that base.
- After repair, all three focused tests pass, including both unrelated import forms, genuine plain/optional namespace calls, body and enclosing shadows, and genuine/shadowed renamed re-exports.
- The affected TypeScript suite passes: **2,344 passed, 0 failed**, 15,885 unrelated tests filtered. These runs consumed the final Rust/test blobs before the commit; logs and executable hashes retain that build identity. The clean committed release executable supplies exact-commit runtime comparison evidence.
- `cargo fmt -p ripr --check`: pass on the committed candidate. Strict `cargo clippy --locked -p ripr --all-targets -j 2 -- -D warnings` failed on three `cmp_owned` errors in untouched Rust files: `effect_carrier/argument_receiver.rs:180` and `owner_pin.rs:8413,8420`. A clean isolated worktree at exact frozen b51aebc7e reproduced the same three errors with `cargo clippy --locked -p ripr --lib -j 1 -- -D warnings` (exit 101). Both affected Rust files have identical Git blobs in base and candidate. `lint-attribution.json` records the proof; neither failed check is counted as passing.
- No aggregate `check-fast`/`precommit`, hosted CI or duplicate full-library qualification was requested. The installed `origin/main` selector selects **18,272 inherited paths**, with merge base `551456df5b70c4a3ae9ad95e812e193d40fb5999`, instead of this three-path frozen-base delta. `basis.json` retains the identities and successful inventory. The existing central owner retains composition qualification.

A premature green-test attempt was explicitly terminated because it began before formatting had returned; no result from it is used. Its log and disposition are retained. The subsequent stable-tree green run and affected-suite run passed. No OOM, resource-ceiling change or retry-as-pass is concealed.

Source inspection followed both import arms into relation construction and `OwnerLineContext`, checked the unchanged identity/shadow/mock conjunctions, and challenged them with the red work counter, positive calls, shadow counterexamples, all affected-language tests and final CLI parity. No actionable candidate source finding remains. Integration/merge review remains incomplete for the named native lint failure and central composition/hosted proof. No Windows, installed-package, release-publication or broader accuracy result is claimed.

## Reproduce and receive

Both binaries use Rust **1.95.0**, locked dependencies, default features and the repository `release` profile (thin LTO, 16 codegen units). Build: `CARGO_HOME=/workspace/.cargo RUSTUP_HOME=/workspace/.rustup CARGO_TARGET_DIR=/workspace/ts-cost-target cargo build --locked --release -p ripr --bin ripr -j 3`. Workload command: `RIPR_CACHE_DIR=<isolated-cache> RAYON_NUM_THREADS=4 <absolute-binary> check --base HEAD~1 --format json --mode draft`.

- Baseline source-built executable SHA-256: `ca2cedb6555259a2eaffb8864062de4cd990a3a01f48118e7aa13fab5298352f`; observed version names clean `b51aebc7e4735529befd677ebf14071c7f09e477`.
- Candidate source-built executable SHA-256: `4bd0e0c71d28a814417102e326d5159e2171dcc43cffa933258dca9f28da8a3e`; observed version names clean `a80be9118d460f1fddeb68413af097f6122208cb`.
- Disposable profiling binary SHA-256: `dadb98351d9eba3440a8b0ebf93a23c914f2a3098ce58abda34e3e4c00a59f6f`; its version is explicitly `b51aebc7e…-dirty`. Profiling patch SHA-256: `c4e1f9a4e67a37cb1c4b53f942e43c7f228d3c2b0d42d2c094b72160b17af04a`.
- Receiver-readable source patch SHA-256: `27e302c7ce8a7fc0fbdf54ca489fbf5064e99cd7b70a8367422324f7b92b10dd` (`candidate.patch`). Fetch the source branch and inspect/cherry-pick **only a80be9118** through the central owner's normal integration route. Revert that commit to roll back.
- Original accepted evidence remains unchanged at `251c9e94fc6052255d269e59a056101a62b5d887`, with original archive SHA-256 `25e37b736ac52b58b70e2727dc3449d5c82f16528cadf152e7051429bc0df379` (334,029 bytes).

The new raw archive contains all stdout/stderr, measurements, build/test/lint logs, profiler source/patch, work-count tests, ownership refresh, deterministic fixture generators and exact Git bundles, plus semantic assertions. Binaries remain retained locally with their identities and are excluded from the archive. No central-candidate ref, public #1889 draft/DO NOT MERGE hold, #1893 trust hold, release denominator or settings changed.

**Next concrete action:** the existing integration owner reviews this isolated repair and the exact-base lint attribution, routes any frozen Rust lint repair separately, and decides whether to compose a80be9118 into its next candidate. The warm performance owner has completed this scoped implementation/evidence question; no new owner or broad release gate is invented.
