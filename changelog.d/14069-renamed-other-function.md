<!-- section: Fixed -->
- Analysis: a test whose bare call is bound by a renaming import to another function (`use super::other as discount;` beside a changed `discount`) no longer reads as reaching, or exposing, the changed function; the test is related by name only, and that bare call lends no boundary input (#14069).
