# check-local-context

Status: pass

## Why This Matters

Repository state must be durable and portable. Machine paths, Codex memory paths, sandbox references, local transcripts, and session-state documents belong in generated artifacts or local notes, not committed repo knowledge.

## Violations

None detected.

## Rerun

```bash
cargo xtask check-local-context
```
