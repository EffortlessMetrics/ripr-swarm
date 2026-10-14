# Publish boundary

Before push, `git ls-remote` and PR metadata both showed branch/head
`codex/7794-sibling-effect-credit` at `a97ec757ba2bc84292729ee8675d0f5459d3808a`.
PR #7817 was open and Draft, authored by `EffortlessSteven`, with the expected
branch and unchanged PR body.

Fast-forward push completed successfully; see `push.log`. After push, remote
branch and PR head both read back as
`366469fdd2104d630c87da8e1c83437813101324`. PR remained open and Draft, author
and branch were unchanged, and the PR body was unchanged. Comments 4266778104
and 4268030116 were read back unresolved; neither was modified or resolved.
