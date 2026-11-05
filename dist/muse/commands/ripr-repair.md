---
description: Run one RIPR-guided test repair (static evidence, focused test edit, separately executed proof)
argument-hint: [file-or-gap-hint]
---

Run one RIPR repair using the `ripr-repair-workflow` skill as the canonical
workflow; load it before taking action rather than running a shortened
sequence here. $ARGUMENTS, when given, narrows the gap list (a file path
or a short description of the changed behavior); otherwise select the
first repair-ready gap. Pass a path through the supported `file` filter,
not a free-text hint as a fabricated tool argument.

Keep the skill's durable-receipt decision before any edit, preserve its
explicit refresh scope, and follow its isolated control and refusal
boundaries. Return the actual before/after snapshot identities and the
separate static/executed verdicts, with a durable attempt id only when
the CLI produced one.
