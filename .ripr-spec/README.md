# .ripr-spec

`.ripr-spec/` is the durable, repo-owned knowledge base for ripr's source-of-truth rails.

It owns the long-lived chain:

- roadmap
- proposals (why)
- specs (what)
- ADRs (decision)
- lanes + implementation plans (how)
- support claim mapping (what users may believe)
- policy references (which live enforcement ledgers apply)
- closeouts (what happened)

This namespace is tool-neutral and intended to outlive any individual agent/session tool.

## External namespaces

This system is aware of (but does not own) external or tool-specific state:

- `.codex/`
- `.spec/`
- `.claude/`
- `.jules/`

Artifacts in `.ripr-spec/index.toml` must not point into those directories.
