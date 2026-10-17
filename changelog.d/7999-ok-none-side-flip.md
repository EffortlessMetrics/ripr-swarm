<!-- section: Fixed -->
- A sole-source `?` reads as exposed when a test asserts `assert_eq!(owner(..).ok(), None)`, directly, through a constant-row `for` table row whose expected cell is `None`, or through a check helper called with `None`, since `.ok()` is `None` exactly when the call returned `Err` (RIPR-SPEC-0227 decision 5) (#7999).
