<!-- section: Fixed -->
- Agent repair cards now spell instruction, goal, target-kind, done-when, and
  detail-family values in their stable wire form (for example
  `fix_site_ready`, `inline_unit`, `not_requested`) instead of Rust Debug
  variants (`FixSiteReady`, `InlineUnit`, `NotRequested`). The prose a user or
  LLM reads now matches the JSON contract byte for byte, so copy-pasted values
  from card prose resolve in the machine-readable card and vice versa (#9604).
