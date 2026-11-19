<!-- section: Internal / CI -->
- PR admission now reads an exact column-zero `VERDICT: APPROVE <full head SHA>` first line as a source-review attestation and `VERDICT: BLOCK`/`VERDICT: BLOCKED <sha>` as a refusal, so a standalone automated verdict review no longer supersedes an earlier readiness marker from the same login as neutral chatter (#8157).
