<!-- section: Added -->
- Added a nightly advisory `Size Scaling` workflow that builds the release binary and runs `cargo xtask size-scaling` at a 512 KiB base, keeping the report as an artifact so the cost trend is visible without adding a step to the required workflow (#10362).
