# #4510 prerequisite — verified shared extension plan

Source-only; native NOT_RUN. Fresh all-state4510 body search returned[]. Reuse existing release.rs package/install authority, candidate_registry and OwnedProcess, not a new candidate selector/controller.

## Existing seams and actual gaps

release.rs363 PackageInstallResult currently contains success/artifacts/details strings. run_packaged_install369 derives target/package/ripr-version.crate and installed binary, packages/validates traversal and link-free archive, extracts outside checkout, builds a workspace control binary, installs exact extracted crate, checks archive/workspace/installed SHA256 and reported version, then external doctor. It does not return typed candidate admission/archive inventory/executable identity, and derives ambient current checkout rather than requiring an accepted immutable candidate subject. It uses fixed install/report paths and best-effort removal, so cannot represent cleanup failures per reusable row. Refactor this owner rather than scrape details strings or duplicate install.

candidate_registry.rs owns deny_unknown_fields CandidateIdentity169 (sha/tree/ref), ValidatedRegistry309, CandidateGrant315 and resolve_candidate_authority324: actual supplied bytes rawdigest -> registered release lifecycle row -> allowed operation. Current grant exposes registered_path/state/candidate_sha, so extend same validated authority internally to expose required complete candidate identity and row/manifest digests. No arbitrary newcandidate DTO may become admitted from suppliedstrings alone; package attribution needs exact sourceSHA/tree plus admitted registry bytes. No current eligiblepin => admission refuses, without choosing or freezing one.

xtask/run.rs uses ripr::process_owner::OwnedProcess. capture_output_with_timeout475 already owns timeout/termination; capture_bytes_in_dir_with_timeout550 and capture_stdout_to_file_with_timeout607 provide narrower output/cwd forms. Extend this boundary for declared cwd/env and bounded stdout/stderr/rawbyte digests plus explicit termination/reap/cleanup states if needed. Do not create a second process runner. OwnedProcess Windows assign-before-execution Job Object containment already exists. Harness telemetry reuses this owner.

## Coherent files and API

Private release submodule xtask/src/reports/release/candidate_harness.rs plus tests.rs, called from release.rs existing install path; candidate_registry existing grant accessor only as needed; run.rs narrow captured typed observation extension only if absent; release-server sha256_file and existing path/container helpers reused. One private CandidateProofPacket DTO/render projection exported through existing reports module for4505-4508. No new release-readiness command/qualification aggregate. No public crate/deps.

AdmittedSubject can only be constructed by existing validated registry + exact candidate source/repository/ref SHA/tree/currentness and accepted manifest/raw digests. InstalledCandidate can only be constructed by successful package/install checks retaining archive path,size,digest,validated inventory, source subject, installed absolute path,size,digest/version and workspace-control exclusion. Opaque strings remain inputs until these constructors validate them. Child corpus receives an installed handle, owned fixture/launch context and generic typed row API; cannot override admission or process cleanup.

Rows: exact argv/cwd/envpolicy/input/output/artifactdigests; generic states from4510; selected/executed/skipped/failed counts; pre/post filesystem expectedwrite inventory; cleanup retained resources. Render JSON/Markdown from same evaluated DTO, semanticdigest excludes expresslytelemetry PID/time/temp-root spelling only. Recheck registry bytes/ref/source/install digest before each row. Generic harness does not invent child semantic verdict.

## Proof-first matrix

Accept unchanged registry-authorized subject+package+absoluteinstalledbinary, nonzero rows, bounded child and successfulcleanup. Reject independentwrongSHA/tree/movedref/manifest/package/binary(sameversion),PATHdecoy, workspacebinary, foreignCWDdecoywrites, staleartifactinput, zero subjects/execution, alteredinventory/output, invalidfutureDTO. Timeout/cancel and cleanupfailure remain nongreen and preserve resources. Equivalentportable roots same semanticidentity; changedcandidate/inputdifferent. JSON/Markdown parity. Tests invoke owner API/validatedregistry rather than preconstruct admittedhandles. Existing install tests/extractionnegative controls characterize owner before refactor. Nativeproof planned doctor/checkfast/xtaskalltargetsbaseline then focused candidate_harness/install/registry/run controls and requiredguards; all NOT_RUN.

## #4603 reviewer authority and meaningful independent subclaim

Existing rust_judged_panel/release_judgments.rs binds a CLOSED fixed metrics/rust-judged-behavior-panel/release-judgments.json to frozen release-selection digest, roles/evidence and #3806 authority. It proves structural judgment packet consistency and checked-in source provenance; it is not a generic accepted reviewer registry and cannot be repurposed as blind prompt acceptance. eval_sweep_report accepted receipt/current pointer similarly owns only its named Python sweep. No observed reusable blind semantic reviewer acceptance authority.

A meaningful4603preliminary subclaim is closed public-input/event/intervention decoding, digest/order/visibility/privacy validation and deterministic invalid/not_run/honestlimitation projections. It may never emit passed_blind_journey until shared4510 admission AND independently accepted exactprompt/answerkey judgment exist. This is partial4603 acceptance, notfullcompletion. Separate future reviewedprompt verdict should be repository-carried retained exactbytes artifact, with reviewed change provenance; validator validates bindings/closedstatus and does not claim cryptographic reviewer authenticity. Rootmustchoose acceptance gate/location beforepositive production path. No forgeableapprovedBoolean or testonlytrust context inproduction.

## Owning baseline
Immutable basis: 3911480a34c6ce2224932f8494576636c73e1732, tree34485dd11f02b27ac91b8ff0a85a463f5d6e4108. Native admission is NOT_RUN. This plan-only edit precedes tests-first implementation; no accepted subject/installed handle exists yet.

