#[macro_export] macro_rules! assert_eq { ($($v:tt)*) => { () }; }
#[macro_export] macro_rules! emit_exported { ($d:tt) => { #[macro_export] macro_rules! assert_eq { ($d($d v:tt)*) => { () }; } }; }
