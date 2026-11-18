<!-- section: Changed -->
- `ripr check` points at its own instruction when it selects a test gap. The
  `Start here` line now reads `Safe next action: follow the Next step below;
  this is static advisory evidence only` instead of `inspect or repair the
  selected non-exposed gap`. When the finding prints no `Next step`, it reads
  `look at the reason and related test below before changing a test`. Human
  text only; JSON, the agent format and classification are unchanged (#14173).
