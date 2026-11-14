<!-- section: Changed -->
- `ripr pr-review front-panel --assistant-proof <path>` names the read cause on
  stderr and in the JSON warning when the path exists but cannot be read,
  instead of reporting it as missing; a genuinely absent path still says
  missing (#14553).
