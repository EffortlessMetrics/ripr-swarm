# #4510 prerequisite — verified shared extension plan

Implementation NOT_RUN. The owning baseline below passed; it does not admit an installed candidate. Fresh all-state4510 body search returned[]. Reuse existing release.rs package/install authority, candidate_registry and OwnedProcess, not a new candidate selector/controller.

## Existing seams and actual gaps

release.rs363 PackageInstallResult currently contains success/artifacts/details strings. run_packaged_install369 derives target/package/ripr-version.crate and installed binary, packages/validates traversal and link-free archive, extracts outside checkout, builds a workspace control binary, installs exact extracted crate, checks archive/workspace/installed SHA256 and reported version, then external doctor. It does not return typed candidate admission/archive inventory/executable identity, and derives ambient current checkout rather than requiring an accepted immutable candidate subject. It uses fixed install/report paths and best-effort removal, so cannot represent cleanup failures per reusable row. Refactor this owner rather than scrape details strings or duplicate install.

candidate_registry.rs owns deny_unknown_fields CandidateIdentity169 (sha/tree/ref), ValidatedRegistry309, CandidateGrant315 and resolve_candidate_authority324: actual supplied bytes rawdigest -> registered release lifecycle row -> allowed operation. Current grant exposes registered_path/state/candidate_sha, so extend same validated authority internally to expose required complete candidate identity and row/manifest digests. No arbitrary newcandidate DTO may become admitted from suppliedstrings alone; package attribution needs exact sourceSHA/tree plus admitted registry bytes. No current eligiblepin => admission refuses, without choosing or freezing one.

xtask/run.rs uses ripr::process_owner::OwnedProcess. capture_output_with_timeout475 already owns timeout/termination; capture_bytes_in_dir_with_timeout550 and capture_stdout_to_file_with_timeout607 provide narrower output/cwd forms. Extend this boundary for declared cwd/env and bounded stdout/stderr/rawbyte digests plus explicit termination/reap/cleanup states if needed. Do not create a second process runner. OwnedProcess Windows assign-before-execution Job Object containment already exists. Harness telemetry reuses this owner.

## Selected first PR: actual admitted package subject for existing corpus

The first slice is one private source-to-archive-to-installed-subject custody
spine consumed by the existing negative-corpus authentic baseline. It does not
complete the later reusable-row harness or pronounce #2769 qualified. Preserve
#3924's historical accepted lifecycle-integration defer; successor #2769's
[execution graph](https://github.com/EffortlessMetrics/ripr-swarm/issues/2769#issuecomment-5882045830)
requires explicit candidate admission before this harness.

Extend existing `release-negative-corpus --version` with an all-or-none input
group: `--controller-root`, `--candidate-source-root`, `--candidate-artifact`.
The exact controller-relative artifact bytes pass the existing registry
resolver; neither a filename nor a newest-file heuristic grants authority.
Reject partial, blank, duplicate and unsafe inputs before cleanup or spawning.
No group preserves explicitly unqualified legacy smoke. An admission error in
qualification mode never falls back to legacy. Do not gate the fifteen ambient
`release-readiness` checks or create a new command/readiness aggregate.

Controller root owns actual release policy, registry and registered artifact
bytes. Candidate source root owns actual Git HEAD/tree/ref, manifests/lock and
package inputs. The source commit precedes the control packet; controller HEAD
need not equal the pin; candidate identity is checked independently. Separate
worktrees of one repository are valid. Retain
and independently revalidate both domains. Source-promotion's different-repo
common-directory condition is not this contract. No production pin/ref or
checkout selection is created by this implementation.

Extend `candidate_registry`'s existing validated grant with complete identity
and raw-byte custody, keeping constructors private. A private release admission
module uses the existing bounded explicit-CWD process owner for actual Git.
Refactor the existing package owner only as needed to return a private installed
subject, retaining its legacy wrapper. All source builds, archive/install paths
and product execution use explicit owned roots. Do not reconstruct trusted
handles from supplied digest strings, details text, matching versions or unequal
workspace/executable hashes. No new public crate or dependency.

`release_negative::resolve_candidate` passes the admitted absolute installed
binary to existing `build_baseline` and `produce_authentic_chain_in_fixture`.
That genuine consumer creates concrete before/after commits and runs installed
RIPR analysis, verify and receipt. Retain source/archive/executable custody and
mode in the same corpus report. The existing external installed doctor is
another actual observation. Later cases consume the same subject; no synthetic
passed row or second qualification verdict.

### Archive attribution

Cargo's [package documentation](https://doc.rust-lang.org/cargo/commands/cargo-package.html)
explicitly says VCS metadata does not verify provenance. Check parsed
`.cargo_vcs_info.json` SHA, `dirty=false` and package-relative `path_in_vcs`, then
map every ordinary packaged entry to exact committed source bytes. Retain paths,
sizes, SHA256 and committed blob identities. Reject duplicates, traversal,
links, unsupported types and unexpected or ignored/untracked included files,
even when tracked Git status is clean. Initially refuse source symlink
flattening rather than admit content from outside the selected source.

Generated files have explicit rules: `Cargo.toml.orig` matches committed package
manifest; VCS metadata is checked above; normalized `Cargo.toml` and packaged
`Cargo.lock` are retained outputs of the successful bounded Cargo invocation in
fresh exclusively owned output. Parse/check package/version/bin and lock
identities. Their attribution is producer custody and retained actual bytes,
not a claim to independently reimplement every Cargo normalization rule. Unknown
generated/copied entries or unsupported normalization fail visibly. The archive
handle is constructed only by successful production plus validation, never from
an arbitrary caller archive/hash. Revalidate archive bytes/inventory before
extraction/install and executable bytes before product consumers.

Boundary checks remain unlocked: this is observed local producer custody, not
atomic checkout exclusion, hostile-host protection or authenticated build
attestation. Retain that limitation rather than strengthening hashes into
provenance claims.

### Tests first and acceptance

- Parser controls cover legacy, complete qualification group and malformed,
  partial, duplicate, blank and unsafe inputs.
- A real temporary source repository commits the candidate first and creates a
  fully qualified ref. A separate controller fixture then registers its actual
  SHA/tree/ref through the real evaluator/resolver. Controller HEAD differs.
  Independently mutate both root domains and require refusal/revalidation; the
  restored unchanged positive remains usable.
- Decoy source/foreign package/install paths remain byte-identical on admission
  refusal, with no producer invocation. Ignored included foreign input rejects.
- A dependency-free real Cargo fixture named `ripr` exercises actual package,
  extraction, install and absolute execution with a unique source token. Wrong
  same-version source/archive/executable, metadata/inventory and post-production
  substitutions reject. This proves machinery only; never fabricate doctor or
  baseline success from that fixture.
- Actual RIPR selected through a legal fixture registry must be genuinely
  packaged/installed, run external doctor and produce the existing authentic
  before/after/verify/receipt baseline with nonzero observations before delivery
  is claimed. The full corpus remains a separately reported actual denominator.

Observe behavioral RED before production; compilation/host failure is not RED.
After an explicit native lease run focused registry/package/corpus controls,
actual tiny producer, real RIPR baseline and relevant characterizations, then
fmt, committed check-fast/precommit and independent selector parity. Preserve
raw native exits, deadlines, owned process cleanup and failures. No eligible
live pin is an honest refusal, not candidate selection or a release verdict.

Rollback the coherent invocation/custody slice to restore legacy entry behavior;
preserve receipts before owned fixture cleanup. Later #4510 rows, budgets,
cleanup taxonomy, downstream packets, blind acceptance and #2769 aggregation
remain out of scope. Implementation and new tests remain NOT_RUN.

## Later shared-harness scope (not the first PR)

Private release submodule xtask/src/reports/release/candidate_harness.rs plus tests.rs, called from release.rs existing install path; candidate_registry existing grant accessor only as needed; run.rs narrow captured typed observation extension only if absent; release-server sha256_file and existing path/container helpers reused. One private CandidateProofPacket DTO/render projection exported through existing reports module for4505-4508. No new release-readiness command/qualification aggregate. No public crate/deps.

AdmittedSubject can only be constructed by existing validated registry + exact candidate source/repository/ref SHA/tree/currentness and accepted manifest/raw digests. InstalledCandidate can only be constructed by successful package/install checks retaining archive path,size,digest,validated inventory, source subject, installed absolute path,size,digest/version and workspace-control exclusion. Opaque strings remain inputs until these constructors validate them. Child corpus receives an installed handle, owned fixture/launch context and generic typed row API; cannot override admission or process cleanup.

Rows: exact argv/cwd/envpolicy/input/output/artifactdigests; generic states from4510; selected/executed/skipped/failed counts; pre/post filesystem expectedwrite inventory; cleanup retained resources. Render JSON/Markdown from same evaluated DTO, semanticdigest excludes expresslytelemetry PID/time/temp-root spelling only. Recheck registry bytes/ref/source/install digest before each row. Generic harness does not invent child semantic verdict.

## Proof-first matrix

Accept unchanged registry-authorized subject+package+absoluteinstalledbinary, nonzero rows, bounded child and successfulcleanup. Reject independentwrongSHA/tree/movedref/manifest/package/binary(sameversion),PATHdecoy, workspacebinary, foreignCWDdecoywrites, staleartifactinput, zero subjects/execution, alteredinventory/output, invalidfutureDTO. Timeout/cancel and cleanupfailure remain nongreen and preserve resources. Equivalentportable roots same semanticidentity; changedcandidate/inputdifferent. JSON/Markdown parity. Tests invoke owner API/validatedregistry rather than preconstruct admittedhandles. Existing install tests/extractionnegative controls characterize owner before refactor. Native proof uses the recorded baseline below, then focused candidate_harness/install/registry/run controls and required guards; new implementation proof remains NOT_RUN.

## #4603 reviewer authority and meaningful independent subclaim

Existing rust_judged_panel/release_judgments.rs binds a CLOSED fixed metrics/rust-judged-behavior-panel/release-judgments.json to frozen release-selection digest, roles/evidence and #3806 authority. It proves structural judgment packet consistency and checked-in source provenance; it is not a generic accepted reviewer registry and cannot be repurposed as blind prompt acceptance. eval_sweep_report accepted receipt/current pointer similarly owns only its named Python sweep. No observed reusable blind semantic reviewer acceptance authority.

A meaningful4603preliminary subclaim is closed public-input/event/intervention decoding, digest/order/visibility/privacy validation and deterministic invalid/not_run/honestlimitation projections. It may never emit passed_blind_journey until shared4510 admission AND independently accepted exactprompt/answerkey judgment exist. This is partial4603 acceptance, notfullcompletion. Separate future reviewedprompt verdict should be repository-carried retained exactbytes artifact, with reviewed change provenance; validator validates bindings/closedstatus and does not claim cryptographic reviewer authenticity. Rootmustchoose acceptance gate/location beforepositive production path. No forgeableapprovedBoolean or testonlytrust context inproduction.

## Owning baseline
Immutable source basis: 3911480a34c6ce2224932f8494576636c73e1732, tree 34485dd11f02b27ac91b8ff0a85a463f5d6e4108. The plan-only baseline executed at d1d22bd6ed7448b910bbebcf28ce51f288da65f7, tree cec1a3d1ada9dcb28f75aeabc4763da80fb9d51a, against retained selector base 53b7059f0cb2608d5a85a52fd26302b751fece24.

Pass: owning xtask all-target checking and test compilation; all six characterization controls, each with 1 passed, 0 failed and 0 ignored; all 14 check-fast gates with the complete 670-file selector and no skips. Doctor exited 0 with a warning to preserve or clean retained report artifacts at closeout. Precommit was NOT_RUN by this baseline driver. The timeout and pipe-inheriting descendant controls execute actual owned children; package/version and registry fixtures retain the narrower meanings described below. No package installation, source admission, current release pin, or installed qualification was established.

Retained receipt: target/ripr/reports/4510-d1d-baseline-native-v2/native-results.json, SHA256 AF38E6E94A6465342AB5A9157F94F29E15B4BF727D898E9A486AD1498C85AF37. Raw phase logs, exact command/commit contexts and bounded process receipts are alongside it. At 2026-09-29T09:24:47Z, all ten owned phase PIDs and the scoped shared-target executable observations were empty. Source remained clean. This evidence belongs to the baseline object, not a future implementation head.

## Baseline versus implementation oracles

The six selected existing tests characterize inherited behavior only. The package test supplies digest strings, version output and doctor JSON to private validators; it does not install a package or attribute an executable to a source producer. The archive test covers traversal components. Registry controls cover required identity fields and template refusal through the existing resolver, rather than live ref or package attribution. The two timeout controls execute real children, including the platform-specific descendant cleanup path. None constructs an accepted installed-candidate handle.

Before implementation acceptance, add controls through the real admission and row APIs:

- Bind a registry-authorized immutable source SHA/tree and retained manifest bytes to the actual package producer, validated archive inventory and actual installed executable. A matching version or unequal workspace/executable digest is insufficient.
- Independently reject changed or foreign manifest/package/executable bytes, moved source ref, wrong tree and ambient-PATH or workspace binary substitution before invoking a semantic row. Revalidate the subject before each row.
- Exercise selected-root, foreign-CWD and decoy-root writes, stale artifact consumption and portable input identity through owned fixture roots and before/after inventories.
- Observe bounded stdout/stderr, timeout, cancellation and cleanup failure through the existing process owner. Retained resources and missing termination/reap evidence remain non-green.
- Require nonzero selected/executed required subjects, explicit skipped/failed counts and deterministic JSON/Markdown parity. A packet with altered output identities or strengthened human wording must reject.

Tests must call validated constructors and the actual retained producer/consumer path. They must not create trusted handles from supplied digest strings, pre-mark rows passed, or use test-only acceptance as production authority. Observe intended behavioral rejection only after test compilation succeeds; compile or instrument failure is NOT_ESTABLISHED. The characterization baseline is recorded above; new admission controls remain NOT_RUN.

