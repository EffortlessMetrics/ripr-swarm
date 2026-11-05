<!-- section: Internal / CI -->
- Added `cargo xtask install-receipt`, which turns the published-install matrix results into a checked-in receipt (`metrics/install/receipt.json`) with per-platform install seconds, a disposition for each channel the matrix cannot check, and the R0 install verdict against the 300 s ceiling (#13064).
