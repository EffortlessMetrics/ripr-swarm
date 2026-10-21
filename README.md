# Mixed non-text input evidence for adoption

This is a complementary proof handoff to the existing binary-input delivery lead in PR #8831. It is not a competing production candidate, an acceptance expansion for that PR, or merge clearance. That PR explicitly excludes mixed sections. #8830 badge semantics and #8847 totals are unchanged.

Source: `EffortlessMetrics/ripr-swarm` at `6be02b0bff14fe4729fdb6a39072d7ee4e9ee427`, tree `6294862ae28371e085d373f1b875d2edf09bf5ec`, clean source checkout. The CLI was built from this head using installed pinned Rust 1.95.0, default features, Linux x86_64/Bash, one build job, debug info and incremental disabled. See source-identity.json, build.jsonl's compiler-artifact executable, terminal receipt, results.json, and manifests. No broad Cargo test/qualification run.

`replay.py` constructs six independent real Git histories. Every mixed fixture must contain both a real unequal binary payload and a validated text hunk before output assertions execute. It invokes `check --diff` in JSON, human and GitHub formats and Git-backed `diff --base/--head --json`, retaining every argv/cwd/native status/stdout/stderr. The driver exits 1 when mixed binary changes are hidden by a complete outcome; setup/JSON/control failures are separate assertion failures.

Run in a fresh evidence directory (copy replay.py there):

```
python3 replay.py /absolute/path/to/source-built/ripr
```

The raw binary-only input has no text hunk, the behavior-only control produces real Rust findings, the comment-only control has no behavioral subject, and empty input is a distinct no-scope control. The mixed cases change the same binary bytes beside either a behavior or a comment edit. Results and assertions record actual native behavior rather than a blessed golden. The driver is a scratch reproduction aid; adoption into product tests should use the repository's Rust CLI harness.

Producer/consumer trace at the pinned source:

- `analysis/diff/parse.rs::handle_binary_files_sentinel` intentionally does not register a binary path as a textual ChangedFile. It clears hunk/body debt; binary input must not be called malformed merely for lacking a text hunk.
- `analysis/pipeline.rs::has_only_non_text_changes` immediately returns false on any `@@` line. The zero-file refusal guard also fails when a text ChangedFile exists. No mixed binary limitation reaches the final outcome selector.
- `output/json/report.rs` derives analysis_complete from outcome.kind; `output/github.rs` emits the empty clean notice when the outcome is complete and other state warnings are absent.
- `cli/commands/check.rs` renders the report then calls `commands_context::analysis_outcome_exit_result`. `cli/commands.rs::run_diff` follows the same owner after projecting `output/diff_report.rs`. Only unsupported_input maps to exit 2; usable partial analysis remains exit 0. A repair must preserve that distinction.
- `output/gate.rs::discloses_incomplete_analysis_outcome` checks the same false flag, and `output/gate/input.rs` uses it for incomplete-denominator refusal. This is a source trace of a real consumer, not an executed gate-admission claim.

Adoption boundary: the existing owner should judge whether to take a separate mixed-section residual after its standalone rename repair. If adopted, retain a per-section producer fact and shared typed limitation rather than inferring analysis from findings or patching renderers. Preserve supported evidence, empty/no-behavior distinctions, complete_no_findings, pure rename/deletion/submodule conventions, existing malformed section diagnostics, and advisory partial exit 0. Do not label valid binary input malformed without the existing contract owner's decision. No product source, schema, tests, settings, qualifications or merge state changed here; therefore no legitimate analyzed-zero behavior was modified. A native complete_no_findings case was not established by this evidence run.

Existing owner: PR #8831 delivery lead, head `9fdc9a18d1c2c3148e008516dc5b62476d606414`, Draft/open as read during this run. Independent review, candidate repair, hosted qualification and landing remain with that owner/integrator. The mixed-section evidence does not invalidate the explicitly narrower standalone claim by itself.

Observed native results: standalone binary = unsupported_input/incomplete/exit 2; behavior-only = complete_with_findings/2 findings/exit 0; mixed behavior+binary = same complete outcome and 2 findings, no limitations; comment-only and mixed comment+binary both = no_behavioral_candidates/complete/exit 0; empty = no_scope/complete/exit 0. Mixed comment GitHub output says `No static exposure findings found`. Both JSON consumers have the same four intended mixed-honesty assertion failures. Driver native exit 1, controls passed.

Retained local executable: `/workspace/unsupported-mixed-evidence/ripr-witness`, SHA-256 `d17728aa1a1d70604cd132b9cb2918866dbca5979b1250c2de7d3e2ed300f069`. It is intentionally not uploaded; receiver can rebuild exact source and replay. This limits remote custody to raw technical evidence, not executable bytes.
