# Fresh Rust pilot exclusions — 2026-07-13

This note records the six observations reported by the fresh hardened-branch
scout for issue #1560. The source evidence is the issue #1560 follow-up comment
dated 2026-07-12. The scout reported five completed `static_limitation` results
without repair packets and one timeout:

| Repository | Source | Analyzed head | Outcome |
| --- | --- | --- | --- |
| `EffortlessMetrics/ripr-swarm` | PR #1545 | `0db4754aa57845d9473ada2e73f40ede2d75138f` | static limitation |
| `EffortlessMetrics/perl-lsp-swarm` | PR #3703 | `f4b3095e81f16a2e931acccaf2a5d479719fbdfc` | static limitation |
| `EffortlessMetrics/perl-lsp-swarm` | PR #3738 | `81025bcc89515eb30659e1e7388927d2bb1f9d99` | static limitation |
| `EffortlessMetrics/ub-review` | PR #747 | `fc486dc2be564c34b3047e4b5e90accc853ec13e` | static limitation |
| `EffortlessMetrics/ub-review` | PR #738 | `98aea1868f92c6c0ffe89d9faae83fba11de3019` | static limitation |
| `EffortlessMetrics/ripr-swarm` | PR #1515 | `11192c85cbf5016aaf45d44abd9ce53e0b7557d7` | analysis timeout |

The scout comment states that artifacts were written under each isolated
worktree at `target/ripr/fresh-pilot-*.json`. The exact filenames and original
invocation strings were not preserved in the durable comment or current
worktrees. The corpus records that evidence limitation explicitly; it does not
pretend the glob is a single receipt or reconstruct a command from memory.

The five completed observations were limited by missing producer-owned
discriminator or direct-owner evidence, cross-language target context, or
local/closure boundary operands. The timeout has no route state. All six rows
are exclusions, not repair attempts: they remain outside attempt, improvement,
and success denominators. The fresh `ub-review` PR #747 observation has a
distinct fresh-scout artifact from the earlier workflow-comments exclusion for
the same PR/head; it is retained as a separate observation but is not promoted
or counted as an attempt.

This update does not claim route quality, support promotion, a successful
repair, or CallPresence closure. A fresh two-per-repository pilot is required
from current `main` and must produce complete producer-owned test-only routes
before any row can enter `cases`.
