<!-- section: Internal / CI -->
- `cargo xtask verdict-corpus report` splits abstained rows by the side they missed and the observed classification, so a corpus abstention rate separates promotions that are safe to attempt from `gap_missed:no_static_path` rows (#13657).
