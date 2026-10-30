<!-- section: Internal / CI -->
- A twice-hourly scheduled main-health sentinel now probes main directly (`cargo check -p ripr` plus one focused binary smoke test), fails visibly on red or missing evidence, and opens/updates a `main-red` tracking issue that the first green probe closes, so a broken main whose push-gate run was cancelled still lands a durable red record (#10991).
