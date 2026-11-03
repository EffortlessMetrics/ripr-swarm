<!-- section: Internal / CI -->
- `check-evidence-promotion-honesty` now recomputes the `content_sha256` of every committed report in the evidence-promotion honesty corpus and fails when a file was edited after it was generated; a report with a canonicalization the check does not know is reported as unverified, not passed (#12602).
