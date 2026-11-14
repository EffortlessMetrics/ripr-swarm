<!-- section: Fixed -->
- An MCP workspace refresh whose seam-evidence pass failed closed now reports
  its seam inventory as unverified instead of complete, so repair admission
  refuses with `incomplete_inventory` rather than reading opaque stand-in
  evidence as a finished inventory; healthy refreshes are unchanged (#13340).
