<!-- section: Internal / CI -->
- Boundary-gap integration tests no longer share a capture directory when two parallel tests ask for a unique directory in the same clock tick (the `packaged_repair_cli` "create capture ... File exists" failure) (#13196).
