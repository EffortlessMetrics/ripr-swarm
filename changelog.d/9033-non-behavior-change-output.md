<!-- section: Fixed -->
- `ripr check` no longer reports a visibility-qualified module or import declaration (`pub mod new;`, `pub(crate) use a::b;`) as an unknown-state finding with a related test and a repair hint. Like the bare `mod`/`use` forms, such a line now reads as "no changed line is behavior ripr checks"; inline module bodies and lines with trailing code stay probed (#9033).
