<!-- section: Internal / CI -->
- PR admission refuses a review whose ready marker is followed by a blocking verdict line such as `VERDICT: BLOCK <sha>`. Before, the verdict was not in the blocker list and the review counted as a positive attestation (#14592).
