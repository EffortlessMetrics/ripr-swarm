<!-- section: Internal / CI -->
- PR admission now refuses a ready review whose later line is a field label with a non-positive state-token value, such as `Status: REPAIR_REQUIRED`, `Status (round 2): CHANGES_REQUESTED` or `Status -> REPAIR_REQUIRED`, instead of reading it as narration; a positive value such as `- **Status:** REVIEW_READY` still admits (#14592).
