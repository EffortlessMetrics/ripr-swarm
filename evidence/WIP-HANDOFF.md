# PR 7817 handoff

- PR #7817 remains open and Draft. Candidate branch `codex/7794-sibling-effect-credit` remains at `a4e22c0c0ee52a5a858077b2639b3737934b55fa` (tree `0acdc48cdb96f6267fd7b2b799b918d42910323e`).
- Intake head/tree: `9d4d4adf74726854bc106a1e910a1edc2467aafd` / `5b932730ea11e44a07e61f921e51a3d9dfe67612`. That merge has parents `b61141690a0dcd9e679ea2ca6f3e9261357c5e62` and PR base `main` at `ef2bce9017dde50949f4dbf26c7ff25ec6446247` (tree `76e9686acb199752075568fbe36fa4efb430ad01`). Candidate repair commits are descendants of this composition.
- The focused receipt records seven passing commands after commit; post-commit raw logs for reveal/transitive are included. The 13/13 public suite result is recorded for the final repaired source, but its raw stdout was not retained. The preserved older 13/13 log is pre-final and is labeled as such.
- Native Rustc controls compile fixture programs with/without the write. They were run before commit a4, so they are not represented as immutable-a4 analyzer runs. Their sources, compile/run logs, and outcomes are retained for negative-control validity evidence.
- The independent final source review’s two recorded file hashes match candidate a4; see the review binding.
- The inherited #7880 pinned external-corpus setup failure remains separate and blocks full external-corpus qualification.

This is evidence preservation only: no candidate code, PR body, status, gates, or lifecycle state is changed.
