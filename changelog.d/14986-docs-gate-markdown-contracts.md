<!-- section: Fixed -->
- CI docs gate: a docs-only PR now runs the `xtask` test binaries that pin
  repository markdown prose (`gh_pr_status_no_settings`,
  `ripr_public_guide_contract`, `spec_template_contract`,
  `pr_readiness_workflow_contract`, `executable_frontdoor`), so breaking an
  asserted phrase in `AGENTS.md`, `CLAUDE.md`, `.factory/skills/**`,
  `docs/AGENT_WORKFLOWS.md`, `docs/PR_AUTOMATION.md`, a public guide or the
  spec template fails a
  required check instead of reaching main with every Rust lane skipped
  (#14986). `docs/ci/PRODUCT_GATE_PLAN.md` joins the release-surface list in
  both docs-only classifiers because its asserting test lives in the 5,310-test
  `xtask` unit target that the docs gate cannot afford.
