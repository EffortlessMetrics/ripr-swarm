<!-- section: Fixed -->
- Analysis: a test whose bare call can only be bound by a standard-library glob import (`use std::cmp::*;` with no import, definition or other glob that could name the changed `max`) no longer reads as reaching, or exposing, the changed function; the test is related by name only (#14069).
