### Fixed
- Require only tracked RIPR producer roots for native queue intake, and verify the configured roots against the committed Git tree so a nonexistent source cannot make every admission fail. (#4838)
