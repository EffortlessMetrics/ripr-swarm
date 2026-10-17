<!-- section: Fixed -->
- A colon inside a string literal or comment on a changed line (`format!("{:<8}{:>6}\n", ..)`, `println!("total: {}", t)`) no longer creates a spurious `field_construction` probe, so a format call that a strong assertion discriminates is no longer reported as a gap (#6578).
