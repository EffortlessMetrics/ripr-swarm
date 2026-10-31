<!-- section: Fixed -->
- A refused gap-bound start now reports when restoring prior workflow
  artifacts itself fails instead of silently leaving clobbered state, and
  the unbounded git pipe reader propagates a mid-stream read error instead
  of presenting truncated output as complete (#11261).
