<!-- section: Internal / CI -->
- `cargo xtask agent-eval` treats a run whose `cli-agent-loop` FIND step and `cli-agent-find` capture disagree (exit, bytes or tokens) as an instrument failure: the same command on identical workspaces must match, so a mismatch is host noise. It fails every mode, blocks `--write-baseline`, and `--compare` refuses a baseline that recorded one (#14726).
