<!-- section: Fixed -->
- `ripr_get_repair_card` now refuses with the typed `incomplete_inventory` code, like `ripr agent card`, when the seam limit truncates the repo-wide inventory or its completeness cannot be established, instead of reporting `readiness.repair_ready` true (#9231).
