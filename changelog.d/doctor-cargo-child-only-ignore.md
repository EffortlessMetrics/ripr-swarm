<!-- section: Fixed -->
- `ripr doctor` now also flags a Cargo repo whose `.gitignore` covers only `/target/ripr/`, because the repair preflight needs `target/` itself ignored, and the "ripr artifact file(s) ... not counted" note from `ripr check` names `/target/` for Cargo projects. (#9006)
