<!-- section: Docs -->
- Point the PowerShell native-exit-status example in
  `docs/agent-context/validation.md` at `cargo xtask check-agent-skills`, which
  actually runs the agent-skills gate, instead of `cargo test -p xtask
  agent_skills`, a filter that matched no tests (#7456).
