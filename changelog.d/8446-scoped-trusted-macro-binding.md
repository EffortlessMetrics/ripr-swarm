<!-- section: Fixed -->
- A trusted macro name bound only inside an inline module or a body
  (`mod m { use ext::vec; }`) no longer makes a `vec!` call outside that scope
  opaque, so an `assert_eq!` in its arguments does not withhold exact pins in
  other files; a call inside the scope stays opaque (#8446).
