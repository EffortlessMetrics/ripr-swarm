<!-- section: Changed -->
- Contributor landing guidance now leads with the live native merge queue:
  `docs/MERGE_FLOW.md`, the finish-pr skill copies, `AGENTS.md`, `CLAUDE.md`
  and the agent-context docs no longer offer the direct expected-head squash or
  `.agents/skills/finish-pr/scripts/guarded_merge.py` as a fallback, the
  queue-only CI claim matches the triggers that are still installed, and the
  rollout note no longer reads as a live imperative (#14947).
