<!-- section: Fixed -->
- MCP `ripr_get_repair_card` no longer serves a card labeled current over source that changed after `ripr_refresh`: each read re-verifies the captured digests of only the files the card's evidence is bound to and refuses with `stale_snapshot`, naming the changed path and telling the agent to run `ripr_refresh` again. Unrelated edits keep the card (#7357).
