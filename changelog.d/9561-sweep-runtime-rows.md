<!-- section: Internal / CI -->
- Schema producer sweep: the `repair-attempt` and `execution_result` rows now name a reviewed narrower authority (real-producer-byte schema checks in the existing Rust tests), pinned so deleting one fails the gate (#9561).
