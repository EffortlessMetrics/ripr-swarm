<!-- section: Internal / CI -->
- The scheduled main-health sentinel also judges the latest completed main-push run of `Routed Rust Small` and `CI`, so test-level breaks on main open the `main-red` tracker instead of going unnoticed (#13793, #13639, #13783).
