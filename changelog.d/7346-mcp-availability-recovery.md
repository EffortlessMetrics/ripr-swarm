<!-- section: Fixed -->
- MCP snapshot evidence tools and snapshot/gap/repair-card resources now
  report `workspace_unavailable` with the restart-with-root recovery on an
  unavailable startup root, matching `ripr_refresh`. Healthy roots before
  their first refresh still report `no_snapshot` (#7346).
