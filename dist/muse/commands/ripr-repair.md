---
description: Run one RIPR-guided test repair (static evidence, focused test edit, separately executed proof)
argument-hint: [file-or-gap-hint]
---

Run one RIPR repair using the `ripr-repair-workflow` skill. $ARGUMENTS,
when given, narrows the gap list (a file path or a short description of
the changed behavior); otherwise start from the first repair-ready gap.

1. `ripr_workspace_status`, then `ripr_refresh`. Record the snapshot id.
2. `ripr_list_gaps` with `repair_ready: true` (plus `file` when a path
   was given). Present the bounded summaries and pick exactly one gap.
3. `ripr_get_gap` for that item; state the changed behavior, the missing
   discriminator, and the sink the oracle must observe.
4. `ripr_get_top_limitation` and `ripr_get_repair_card`. If
   `repair_packet_ready` is false, report the typed reason and stop.
5. Make one focused test edit that asserts the changed behavior at its
   boundary. No production-code edits to improve the evidence.
6. Run the focused test yourself and record the command and exit status.
   Then run the wrong-implementation control: the same test against the
   broken or pre-change behavior must fail. Restore correct behavior.
7. `ripr_refresh` again, re-read the same gap, and report the static
   verdict (before/after class, both snapshot ids) separately from the
   executed verdict (test command, exit status, control result). Never
   call a static improvement a completed repair on its own, and never
   use the words killed, survived, untested, proven, or adequate for a <!-- ripr-allow: static-language: names the prohibited terms to forbid them -->
   static result.
