# Reverse-scope required Rust qualification

Candidate: 99ff69b63e38a3e0627d859be898e74299f8bb69; head tree24ae3ec11f39ed2dbec175dbb019f168e4db850a.
PR8931 is Ready and unmerged. The qualification-only continuation is complete; merge, release, deployment, credential/settings/protection holds remain.

Run39367076753 attempt2 completed SUCCESS at2026-10-20T04:35:58Z.
Rust job118447253983 and aggregate Ripr Rust Small Result118456789415 succeeded.
The actual Rust checkout is merge7c656d3096e5658b80ccb765a831341df66d3253, parentsd8edec61714b65774abf38ce17ba91202207de25 and99ff69b63e38a3e0627d859be898e74299f8bb69, treec3c01de526445a5afe087bdc2fd24b6c0d1e9296.
Terminal logs bind the checkout; independently fetched Git objects bind its parents/tree. The newer-main combined preview17e5c822933cee594ef6250fee80267cf723a33a was not executed. This is qualification on the recorded merge subject, not a current-main integration or merge decision.

Observed commands:
- cargo check --workspace --all-targets: pass.
- cargo check -p ripr --all-targets --all-features: pass (compile dimension).
- cargo clippy --workspace --all-targets -- -D warnings: pass.
- cargo nextest run --workspace --profile ci: 17615tests run across128binaries;17615passed,4skipped,1slow;672.768s. Profile retries0 and no filter. The workflow's fresh/nonempty JUnit guard passed.
- cargo test --workspace --doc:2passed,1ignored,0failed.
- formatting/policy/evidence-honesty/dependency/process/network gates: pass.
- goldens check, fixtures, verdict-corpus check-all and PR evidence generation/checks: pass. Corpora passed their configured expectations; this is not a zero-false-verdict claim.
- Separate Rust-only feature run:11311passed,0failed,2ignored across104harness rows on the same merge checkout.

Attempt1 was refused at admission before Rust work because PR metadata lacked the native exact Governing issue or accepted claim / Acceptance delta fields. Only that metadata was repaired and validated; an existing-run failed-jobs retry was accepted. GitHub reran lightweight route/docs prerequisites. There was no relabel or duplicate full Rust run, source edit, guard change or unchanged local Rust build/test replay.

Windows advisory wrapper succeeded but retained failed Clippy and selector substeps. The92selected library/smoke tests passed. See windows-advisory-limits.json. Known existing owners retain issues8840/8848 and PRs8845/8849; their work was not absorbed or duplicated. No clean or full Windows pass is claimed. Historical ub-review cancellation/missing evidence remains in prior archivede21bb46bbb821fca3fbd6a210636893727b04ff. Gemini quota is unavailable; source/oracle review remains the substantive carried review. Fresh Conversation/review/inline inventory found no new material source finding; Devin also returned no issues.

Files are decoded GitHub log text plus metadata and receipts. Hashes bind stored UTF-8 payloads (text line endings may be normalized); they do not claim original ZIP-byte equivalence. Artifact ZIP digests are GitHub metadata only. No denied artifact-transfer route was retried. Required nextest/context artifact12240752507 digestsha256:894a883b7bb1ea63865a3be3fb6903b6f86a86d79656493f51856d54bf6fa492 remains referenced, not independently extracted.

Source patch/bundle and original local proof remain in archive4f3e69835cf3ac49f3f5b4e25a84f3a3973eb24d. Both prior archives stay unchanged. The original working tree/build target were excluded from cleanup. Resource closeout retained this task's5.2GiB warm target because process-reader verification encountered unreadable processes; no deletion or privilege workaround was attempted. Candidate worktree, frozen RED/GREEN executable witnesses and raw evidence remain available for the parent integration reader. Only this continuation's17,259-byte ignored Python validation cache was released.

This evidence branch is not a product release or permission to integrate.
