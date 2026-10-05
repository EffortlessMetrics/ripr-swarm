- A changed `let` line with a type annotation or pattern
  (`let value: u8 = parse()?;`) no longer acquires a spurious
  `field_construction` probe; only a struct literal in the initializer reads as
  field construction (#6676).
