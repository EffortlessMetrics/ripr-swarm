# Source-of-truth stack

ripr preserves a full chain:

Roadmap -> Proposal -> Spec -> ADR -> Lane tracker -> Implementation plan -> PRs -> Proof -> Support/policy updates -> Closeout

The durable home for these rails is `.ripr-spec/`.

Tool/session directories such as `.codex/`, `.spec/`, `.claude/`, and `.jules/` are external namespaces and are not durable artifact ownership locations for this lane.
