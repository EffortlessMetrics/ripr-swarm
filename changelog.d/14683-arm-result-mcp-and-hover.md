<!-- section: Fixed -->
- `ripr_get_gap` and the editor hover now name a changed match arm's new
  result too: the MCP gap's `changed_behavior` gains `arm_result` (`450` for
  `Size::Medium =>`), and the hover's changed expression and line heading
  read `Size::Medium => 450`. The same fail-closed rule as the agent gap row
  applies; diagnostic data and identities keep the head (#14683).
