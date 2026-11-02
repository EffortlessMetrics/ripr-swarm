### Fixed

- Proc-macro derive reach witnesses no longer treat file-wide `#[derive(Name)]`
  text: each derive must sit on a live `struct`/`enum`/`union` item, ignoring
  `cfg(any())`-disabled items, inactive enclosing `cfg`, and derive-shaped tokens
  inside macro delimiters such as `stringify!` (#6932).
